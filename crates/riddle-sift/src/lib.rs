//! The Sift pipeline (ARCHITECTURE.md §17): facts, gates, Signal Selection,
//! judge, routes, confidence policy, escalation, guards, disposition, trace.
//!
//! Sift takes already-normalized text (`SiftInput`) so it never depends on
//! the mail crates; the integrator builds the input from the message, the
//! Thread State and the People facts. Three rules are enforced here and
//! nowhere else:
//!
//! - The Never Hidden gate runs before any Signal and no route overrides it
//!   (invariant 42). A route that would Spoil a never-hidden sender yields
//!   Unresolved with the gate recorded.
//! - Contamination is decided by deterministic evidence and never reaches
//!   the judge (§58, §2): the judge does not treat adversarial content as
//!   hostile, and a phish that is judged is a phish that was read.
//! - When the judge is unavailable the answer is Unresolved and nothing is
//!   hidden (invariant 37); deterministic routes still file the obvious.
//!
//! Every outcome carries a `DecisionTrace` with every stage's timing, the
//! cost and the build stamp (§60, §68): no intelligent decision without
//! structured telemetry (§92).

pub mod actions;
pub mod expiry;
pub mod explain;
pub mod facts;
pub mod gates;
pub mod pipeline;

pub use explain::explain;
pub use facts::{compute_facts, gmail_categories_from_labels, parse_deadlines};
pub use gates::{contamination_evidence, never_hidden_reason};

use riddle_core::*;
use riddle_errors::Result;
use riddle_mesh::MeshDecision;
use std::collections::BTreeMap;
use std::sync::Arc;

/// What Sift knows about the Dig the message belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigContext {
    pub dig_id: DigId,
    pub group: Option<String>,
    pub privacy: PrivacyMode,
    /// The Dig is in its Shadow Period: record and show, apply nothing.
    pub shadow: bool,
}

impl DigContext {
    pub fn from_dig(d: &Dig) -> Self {
        Self {
            dig_id: d.id.clone(),
            group: d.group.clone(),
            privacy: d.privacy,
            shadow: matches!(d.status, DigStatus::Shadow { .. }),
        }
    }
    pub fn fixture(dig_id: &str) -> Self {
        Self {
            dig_id: DigId(dig_id.into()),
            group: None,
            privacy: PrivacyMode::CloudReasoning,
            shadow: false,
        }
    }
}

/// People facts about the sender, computed by `riddle-people`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PeopleFacts {
    pub in_contacts: bool,
    pub replied_to_before: bool,
    pub first_time_sender: bool,
    pub automated_sender: bool,
    pub mailing_list: bool,
    pub auth_failures: u32,
    pub spam_verdicts: u32,
    pub relationship_strength: f64,
    pub cadence: Option<riddle_core::Cadence>,
    pub on_schedule: bool,
    /// The thread has mail from the user or someone they know.
    pub known_thread: bool,
    /// The user knows someone at the sender's (non-consumer) domain.
    pub known_domain: bool,
}

impl PeopleFacts {
    pub fn from_person(p: Option<&Person>) -> Self {
        match p {
            None => Self {
                first_time_sender: true,
                ..Default::default()
            },
            Some(p) => Self {
                in_contacts: p.in_contacts,
                replied_to_before: p.replied_to_by_user,
                first_time_sender: p.messages_from <= 1,
                automated_sender: p.automated,
                mailing_list: p.mailing_list,
                auth_failures: p.auth_failures,
                spam_verdicts: p.spam_verdicts,
                relationship_strength: p.relationship_strength,
                cadence: p.cadence,
                on_schedule: p.on_schedule,
                // Set by the caller, which has the stores (`known_context`).
                ..Default::default()
            },
        }
    }
    pub fn first_time() -> Self {
        Self {
            first_time_sender: true,
            ..Default::default()
        }
    }
    pub fn known() -> Self {
        Self {
            in_contacts: true,
            replied_to_before: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct SiftInput {
    pub message: Message,
    pub normalized_text: String,
    pub thread_state: Option<ThreadState>,
    pub profile: UserProfile,
    pub people: PeopleFacts,
    /// Gmail categories from labels, e.g. `promotions`.
    pub gmail_categories: Vec<String>,
    /// ISO 639-1, `und` when unknown.
    pub language: String,
    pub has_calendar_part: bool,
    pub has_structured_receipt: bool,
    pub dig: DigContext,
    /// Named Context slices the state builders may include (`user_profile`,
    /// `sender_history`, ...). Retrieved, not stuffed (§44).
    pub context_slices: BTreeMap<String, String>,
}

impl SiftInput {
    /// A minimal input for a message whose text is its body or snippet.
    pub fn simple(
        message: Message,
        profile: UserProfile,
        people: PeopleFacts,
        dig: DigContext,
    ) -> Self {
        let normalized_text = message
            .body_text
            .clone()
            .unwrap_or_else(|| message.snippet.clone());
        let gmail_categories = gmail_categories_from_labels(&message.labels);
        let mut context_slices = BTreeMap::new();
        let mut profile_line = String::new();
        if let Some(n) = &profile.name {
            profile_line.push_str(&format!("The recipient is {n}"));
            if let Some(r) = &profile.role {
                profile_line.push_str(&format!(", {r}"));
            }
            profile_line.push_str(". ");
        }
        if !profile.active_projects.is_empty() {
            profile_line.push_str(&format!(
                "Active projects: {}.",
                profile.active_projects.join(", ")
            ));
        }
        if !profile_line.is_empty() {
            context_slices.insert("user_profile".into(), profile_line);
        }
        Self {
            message,
            normalized_text,
            thread_state: None,
            profile,
            people,
            gmail_categories,
            language: "en".into(),
            has_calendar_part: false,
            has_structured_receipt: false,
            dig,
            context_slices,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SiftOptions {
    pub token_budget: u32,
    /// The confidence policy: the margin the weakest winning Signal must
    /// clear above its threshold, per disposition (§39), and whether
    /// non-English mail is held for escalation (§18). It is the simulator's
    /// own type, so a preview never promises what Sift would hold back.
    pub policy: riddle_mesh::ConfidencePolicy,
    pub objective: String,
    /// Escalate even when the judge's scores were decisive. Only model
    /// comparison sets it: every candidate must be asked, or each would
    /// just repeat the judge on the messages it was sure about.
    pub force_escalation: bool,
}

impl Default for SiftOptions {
    fn default() -> Self {
        Self {
            token_budget: 2_000,
            policy: riddle_mesh::ConfidencePolicy::default(),
            objective: "classify the message and decide whether it deserves attention".into(),
            force_escalation: false,
        }
    }
}

/// Whether escalation to the LLM has budget left for this Dig (§55).
pub trait EscalationBudget: Send + Sync {
    fn allows(&self, dig: &DigId) -> bool;
}

#[derive(Debug, Clone)]
pub struct SiftOutcome {
    pub classification: Classification,
    pub trace: DecisionTrace,
    pub find: Option<FindProposal>,
    pub thread_state_update: Option<ThreadStateUpdate>,
    pub timing: TraceTiming,
    pub facts: Facts,
    pub evaluations: Vec<SignalEvaluation>,
    pub cached: usize,
    pub fresh: usize,
    pub judge_unavailable: bool,
    pub shadow: bool,
    pub mesh_decision: Option<MeshDecision>,
    /// Provenance note: "13 matched on cached Signals, 1 needed a fresh evaluation".
    pub note: String,
    /// Per-Set state truncation, for the Raw Jev Inspector.
    pub truncated: Vec<String>,
    /// Messages of this decision's `+ Replaces` series it made moot; filled
    /// by `persist`.
    pub superseded: Vec<MessageId>,
}

pub struct Sift {
    pub stores: Arc<riddle_store::Stores>,
    pub judge: Option<Arc<dyn SemanticJudge>>,
    pub refiner: Option<Arc<dyn SignalRefiner>>,
    pub clock: Arc<dyn Clock>,
    pub events: Arc<dyn EventSink>,
    pub build: BuildStamp,
    pub opts: SiftOptions,
    pub budget: Option<Arc<dyn EscalationBudget>>,
    pub projection: Option<Arc<dyn ProjectionHook>>,
}

impl Sift {
    pub fn new(
        stores: Arc<riddle_store::Stores>,
        clock: Arc<dyn Clock>,
        events: Arc<dyn EventSink>,
        build: BuildStamp,
    ) -> Self {
        Self {
            stores,
            judge: None,
            refiner: None,
            clock,
            events,
            build,
            opts: SiftOptions::default(),
            budget: None,
            projection: None,
        }
    }

    /// Wire from the Action layer's services.
    pub fn from_services(services: &riddle_actions::Services) -> Self {
        let mut s = Self::new(
            services.stores.clone(),
            services.clock.clone(),
            services.events.clone(),
            services.build.clone(),
        );
        s.judge = services.judge();
        s.refiner = services.refiner();
        s.projection = services.get::<ProjectionHookHandle>().map(|h| h.0.clone());
        s
    }

    pub fn with_judge(mut self, judge: Arc<dyn SemanticJudge>) -> Self {
        self.judge = Some(judge);
        self
    }
    pub fn with_refiner(mut self, refiner: Arc<dyn SignalRefiner>) -> Self {
        self.refiner = Some(refiner);
        self
    }
    pub fn with_options(mut self, opts: SiftOptions) -> Self {
        self.opts = opts;
        self
    }
    pub fn with_budget(mut self, budget: Arc<dyn EscalationBudget>) -> Self {
        self.budget = Some(budget);
        self
    }
    pub fn with_projection(mut self, hook: Arc<dyn ProjectionHook>) -> Self {
        self.projection = Some(hook);
        self
    }

    /// Run the pipeline without persisting anything.
    pub async fn run(&self, input: SiftInput) -> Result<SiftOutcome> {
        pipeline::run(self, input).await
    }

    /// Run and persist: facts, classification, trace, events, projection.
    pub async fn run_and_persist(&self, input: SiftInput) -> Result<SiftOutcome> {
        let mut outcome = self.run(input).await?;
        self.persist(&mut outcome)?;
        Ok(outcome)
    }

    /// Persist an outcome (facts, classification, trace), emit events, hand
    /// the classification to the projection hook, and return the Find
    /// proposal for `riddle-finds`. Updates the outcome's persist timing.
    pub fn persist(&self, outcome: &mut SiftOutcome) -> Result<Option<FindProposal>> {
        pipeline::persist(self, outcome)
    }
}

/// Seed the system Signals and the default Meshes (idempotent); the
/// integrator calls this on every start before the first Sift.
pub fn bootstrap(stores: &riddle_store::Stores) -> Result<(usize, usize)> {
    let signals = riddle_signals::Registry::new(stores.index()).seed_system()?;
    let meshes = riddle_mesh::MeshService::new(stores.index()).seed_defaults()?;
    Ok((signals, meshes))
}

/// The Actions this crate exposes, for registration by the integrator.
pub fn actions() -> Vec<Arc<dyn riddle_actions::Action>> {
    actions::all()
}
