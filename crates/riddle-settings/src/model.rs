//! The settings model: `Settings` (config, hand-editable TOML) and `State`
//! (runtime, JSON). Split by ownership per ARCHITECTURE.md §12 so a form save
//! can never clobber sync cursors or update state, and high-churn data
//! (Activity, traces) lives in the store, never here.
//!
//! Every field has a serde default and the struct is `#[serde(default)]`, so
//! a file written before a feature existed still loads (the compatibility
//! tests in `tests/compat.rs` prove it with committed fixtures). Secrets are
//! never in these files: only `*Present` booleans, derived at read time from
//! `SecretPresence`. The field names are the IPC contract in
//! `docs/COMMANDS.md`; the ts-rs binding is generated from this file.

use riddle_actions::policy::AutonomyPolicy;
use riddle_core::{Autonomy, DigId, FindType, PrivacyMode, ProjectionMode, UiLevel};
use riddle_errors::{Result, RiddleError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Bumped when a field changes meaning; additions need no bump because of
/// the serde defaults. Stamped into every file we write.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const STATE_SCHEMA_VERSION: u32 = 1;

const FILE_HEADER: &str = "# Riddle settings. Hand-editable: unknown keys are ignored, missing keys take defaults.\n# Secrets are never stored here.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
    pub fn to_level_filter(self) -> log::LevelFilter {
        match self {
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Trace => log::LevelFilter::Trace,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum JudgeProvider {
    #[default]
    Jev,
    Null,
    Scripted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum KeySource {
    Env,
    Keychain,
    #[default]
    Ssm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LlmProvider {
    #[default]
    Bedrock,
    Null,
    Scripted,
}

/// TOML has no null, and a missing key takes the default on load, so an
/// optional string is written as `""` to mean "none" and read back the same.
mod opt_str {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
        v.as_deref().unwrap_or("").serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
        let v = Option::<String>::deserialize(d)?;
        Ok(v.filter(|s| !s.trim().is_empty()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct JudgeSettings {
    pub provider: JudgeProvider,
    /// Pinned to an exact version (§56).
    pub model: String,
    pub key_source: KeySource,
    #[serde(with = "opt_str")]
    #[ts(type = "string | null")]
    pub ssm_profile: Option<String>,
    #[serde(with = "opt_str")]
    #[ts(type = "string | null")]
    pub ssm_parameter: Option<String>,
    /// Derived at read time from the keychain or SSM; never written to disk.
    pub api_key_present: bool,
}

impl Default for JudgeSettings {
    /// Production: the key comes from the keychain, and there is none until
    /// the person pastes one. The defaults used to name the owner's AWS SSM
    /// profile, so every other install silently had no judge and no way to
    /// see why (17 Oct 2026). Without a key Riddle sorts by rules and hides
    /// nothing; `dev_defaults` keeps SSM for the owner's dev profile.
    fn default() -> Self {
        Self {
            provider: JudgeProvider::Jev,
            model: "jev-1.13.0".into(),
            key_source: KeySource::Keychain,
            ssm_profile: None,
            ssm_parameter: None,
            api_key_present: false,
        }
    }
}

/// The owner's AWS setup for the dev profile, read from the environment
/// so the repository names nobody's account: `RIDDLE_DEV_AWS_PROFILE` is
/// the profile the judge key and Bedrock use, `RIDDLE_DEV_JEV_SSM_PARAMETER`
/// the Parameter Store name of the Jev key. A git-ignored `.env` at the
/// repository root carries them and `scripts/run-signed.sh` exports it for
/// every binary cargo runs. Unset, the dev profile is the production
/// profile: keychain, and no judge until a key is pasted. The values used
/// to be compiled in, which put the owner's account names in the source
/// (18 Oct 2026).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevAws {
    pub profile: Option<String>,
    pub jev_parameter: Option<String>,
}

pub const DEV_AWS_PROFILE_VAR: &str = "RIDDLE_DEV_AWS_PROFILE";
pub const DEV_JEV_SSM_PARAMETER_VAR: &str = "RIDDLE_DEV_JEV_SSM_PARAMETER";

impl DevAws {
    /// The environment as the process sees it.
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// `lookup` stands in for the environment; blank values count as unset.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let read = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        Self {
            profile: read(DEV_AWS_PROFILE_VAR),
            jev_parameter: read(DEV_JEV_SSM_PARAMETER_VAR),
        }
    }
}

impl JudgeSettings {
    /// The dev profile's judge: the key resolved from the owner's SSM when
    /// the environment names it, else the production default.
    pub fn dev_defaults() -> Self {
        Self::dev_defaults_for(&DevAws::from_env())
    }

    pub fn dev_defaults_for(aws: &DevAws) -> Self {
        match (&aws.profile, &aws.jev_parameter) {
            (Some(profile), Some(parameter)) => Self {
                key_source: KeySource::Ssm,
                ssm_profile: Some(profile.clone()),
                ssm_parameter: Some(parameter.clone()),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }

    /// No judge can run: there is no key where the settings say to look.
    /// An SSM or environment source is a lookup that may fail, which is a
    /// failure, not a missing setup.
    pub fn not_set_up(&self) -> bool {
        match self.provider {
            JudgeProvider::Null => true,
            JudgeProvider::Scripted => false,
            JudgeProvider::Jev => {
                matches!(self.key_source, KeySource::Keychain) && !self.api_key_present
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct LlmSettings {
    pub provider: LlmProvider,
    pub profile: String,
    pub region: String,
    pub escalation_model: String,
    pub director_model: String,
}

impl Default for LlmSettings {
    /// Production: Bedrock through the default AWS credential chain (an
    /// empty profile), not the owner's named profile.
    fn default() -> Self {
        Self {
            provider: LlmProvider::Bedrock,
            profile: String::new(),
            region: "us-east-1".into(),
            escalation_model: "us.anthropic.claude-haiku-4-5-20251001-v1:0".into(),
            director_model: "us.anthropic.claude-sonnet-4-5-20250929-v1:0".into(),
        }
    }
}

impl LlmSettings {
    /// The dev profile's LLM: the owner's AWS profile when the environment
    /// names one, else the default credential chain.
    pub fn dev_defaults() -> Self {
        Self::dev_defaults_for(&DevAws::from_env())
    }

    pub fn dev_defaults_for(aws: &DevAws) -> Self {
        Self {
            profile: aws.profile.clone().unwrap_or_default(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct BudgetSettings {
    pub daily_judge_usd: f64,
    pub daily_llm_usd: f64,
    pub escalation_per_dig_usd: f64,
}

impl Default for BudgetSettings {
    fn default() -> Self {
        Self {
            daily_judge_usd: 1.0,
            daily_llm_usd: 5.0,
            escalation_per_dig_usd: 1.0,
        }
    }
}

/// The Google OAuth client every Gmail account signs in through. A build
/// can carry one (`RIDDLE_GOOGLE_CLIENT_ID` and `RIDDLE_GOOGLE_CLIENT_SECRET`
/// at compile time; README "Google sign-in"), so a new person signs in
/// without creating a client in Google Cloud first, which every new user
/// had to do before (17 Oct 2026). A person's own client, imported or
/// typed, is used instead whenever its id is set, and `use_builtin_client`
/// off makes the built-in one count as absent: bring-your-own remains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GmailSettings {
    pub client_id: String,
    /// Derived at read time from the keychain; never written to disk.
    pub client_secret_present: bool,
    /// Derived at read time from the build; never written to disk.
    pub builtin_client_available: bool,
    /// Sign in through the built-in client while no own client id is set.
    pub use_builtin_client: bool,
}

impl Default for GmailSettings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret_present: false,
            builtin_client_available: false,
            use_builtin_client: true,
        }
    }
}

/// A Google OAuth client to sign in with: the person's own, or the one
/// baked into the build. A Desktop client's secret is not confidential
/// (Google says so: the client is public and uses PKCE), and still never
/// appears in a log: `Debug` says only whether there is one.
#[derive(Clone, PartialEq, Eq)]
pub struct GoogleClient {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub builtin: bool,
}

impl std::fmt::Debug for GoogleClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleClient")
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret.is_some())
            .field("builtin", &self.builtin)
            .finish()
    }
}

impl GmailSettings {
    /// The client baked into this build, when the build had one.
    pub fn builtin_client() -> Option<GoogleClient> {
        Self::builtin_client_from(
            option_env!("RIDDLE_GOOGLE_CLIENT_ID"),
            option_env!("RIDDLE_GOOGLE_CLIENT_SECRET"),
        )
    }

    /// `builtin_client` over given values: what a build bakes in, and what
    /// a test hands over to stand in for one.
    pub fn builtin_client_from(id: Option<&str>, secret: Option<&str>) -> Option<GoogleClient> {
        let id = id.map(str::trim).filter(|s| !s.is_empty())?;
        Some(GoogleClient {
            client_id: id.to_string(),
            client_secret: secret
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            builtin: true,
        })
    }

    /// The client sign-in uses: the person's own when its id is set, else
    /// the built-in one when the build has it and it is not turned off.
    pub fn effective_client(&self) -> Option<GoogleClient> {
        self.effective_client_with(Self::builtin_client())
    }

    /// `effective_client` with the built-in client given, for tests.
    pub fn effective_client_with(&self, builtin: Option<GoogleClient>) -> Option<GoogleClient> {
        let own = self.client_id.trim();
        if !own.is_empty() {
            return Some(GoogleClient {
                client_id: own.to_string(),
                client_secret: None,
                builtin: false,
            });
        }
        builtin.filter(|_| self.use_builtin_client)
    }

    /// Whether any client can sign in: "not set up" otherwise.
    pub fn client_configured(&self) -> bool {
        self.effective_client().is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AutonomySettings {
    pub default: Autonomy,
    /// Keyed by Dig id.
    pub per_dig: BTreeMap<String, Autonomy>,
    /// Keyed by Action name; applies across Digs.
    pub per_action: BTreeMap<String, Autonomy>,
}

/// How Riddle interrupts (Flow 12). `Stream` notifies for every Find at or
/// above the priority threshold as it is raised. `Brief` interrupts at once
/// only for an allow-list (Never Hidden senders, and the Find types below
/// when due within `due_within_hours`) and says everything else in a brief
/// at the chosen times: "7 need you, 5 Unresolved, 41 filed." Notifications
/// carry the reason, never the subject, so a locked screen shows nothing
/// private.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NotificationMode {
    Stream,
    #[default]
    Brief,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct NotificationSettings {
    pub mode: NotificationMode,
    /// Local times as "HH:MM", at most four.
    pub brief_times: Vec<String>,
    /// Find types that may interrupt at once when due soon.
    pub immediate_types: Vec<FindType>,
    pub due_within_hours: u32,
    /// A Find from a Never Hidden sender always interrupts at once.
    pub never_hidden_immediate: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            mode: NotificationMode::Brief,
            brief_times: vec!["09:00".into(), "16:00".into()],
            immediate_types: vec![FindType::Pay, FindType::Respond],
            due_within_hours: 24,
            never_hidden_immediate: true,
        }
    }
}

impl NotificationSettings {
    /// "09:00" → (9, 0); None for anything else.
    pub fn parse_time(t: &str) -> Option<(u32, u32)> {
        let (h, m) = t.trim().split_once(':')?;
        let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
        (h < 24 && m < 60).then_some((h, m))
    }
}

/// TOML has no null, and a missing key takes the struct default (70) on
/// load, so "no threshold" would silently come back as 70 after a restart.
/// It is written as `0`, which means the same thing: any open Find.
mod opt_priority {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<u8>, s: S) -> Result<S::Ok, S::Error> {
        v.unwrap_or(0).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u8>, D::Error> {
        let v = Option::<u8>::deserialize(d)?;
        Ok(v.filter(|p| *p > 0))
    }
}

/// What the Priority inbox surfaces (the `priority_inbox` read). The
/// criteria are OR-combined: a message appears if it carries any chosen
/// Stratum, any chosen Gmail label, is at or above `min_importance`, or is
/// a source of an open Find at or above `min_find_priority` (of the chosen
/// types; empty means any type).
/// One real mailbox had 6,000 messages and 400 open Finds, and no view
/// showed only what its owner had marked as mattering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct PriorityInboxSettings {
    pub strata: Vec<String>,
    /// Provider label ids verbatim (`STARRED`, `IMPORTANT`).
    pub labels: Vec<String>,
    /// 0–100; `None` (written as 0) means any open Find.
    #[serde(with = "opt_priority")]
    #[ts(type = "number | null")]
    pub min_find_priority: Option<u8>,
    /// Empty means every type.
    pub find_types: Vec<FindType>,
    /// Messages at this level or above, whatever their decision; `None`
    /// leaves Importance out of it.
    pub min_importance: Option<riddle_core::Importance>,
}

impl Default for PriorityInboxSettings {
    fn default() -> Self {
        Self {
            strata: vec![],
            labels: vec!["STARRED".into()],
            min_find_priority: Some(70),
            find_types: vec![],
            min_importance: Some(riddle_core::Importance::High),
        }
    }
}

/// Model comparison (Developer → Models): the candidate models, and whether
/// challengers run alongside every real escalation under a daily cap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ComparisonSettings {
    /// Bedrock model ids to compare. A model the account may not call yet
    /// stays listed and joins once its access check passes.
    pub roster: Vec<String>,
    /// Run every usable roster model on each real escalation too, recorded
    /// and never applied.
    pub challengers: bool,
    /// USD a day that challengers and benchmarks may spend together.
    pub daily_usd: f64,
}

/// The roster as of October 2026: the newest of each family the owner's
/// account could call, then the ones Bedrock lists that it could not yet.
pub const DEFAULT_ROSTER: &[&str] = &[
    "us.anthropic.claude-haiku-4-5-20251001-v1:0",
    "us.anthropic.claude-sonnet-4-6",
    "us.anthropic.claude-opus-4-6-v1",
    "us.anthropic.claude-haiku-5-5",
    "us.anthropic.claude-sonnet-5-5",
    "us.anthropic.claude-opus-5-5",
    "us.openai.gpt-6-luna",
    "us.openai.gpt-5.6-luna",
];

impl Default for ComparisonSettings {
    fn default() -> Self {
        Self {
            roster: DEFAULT_ROSTER.iter().map(|s| s.to_string()).collect(),
            challengers: true,
            daily_usd: 3.0,
        }
    }
}

/// The config file (`settings.toml`). See the module header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    pub schema_version: u32,
    /// Derived from `ui_level`: true exactly when the level is Developer.
    /// Kept as a field because the backend and older files read it; the
    /// level is the one switch (see `reconcile_level`).
    pub developer_mode: bool,
    /// How much of Riddle the person sees: Essentials, Everything or
    /// Developer (`riddle_core::UiLevel`). The rail, the go-to shortcuts, the
    /// help sheet and the Director's register read it; every route stays
    /// reachable at every level.
    #[serde(default)]
    pub ui_level: UiLevel,
    pub log_level: LogLevel,
    /// Finds at or above this priority (0–100) interrupt in Stream mode; the rest wait.
    pub notify_priority_threshold: u32,
    pub notifications: NotificationSettings,
    #[serde(default)]
    pub priority_inbox: PriorityInboxSettings,
    pub shadow_period_days: u32,
    pub judge: JudgeSettings,
    pub llm: LlmSettings,
    pub budgets: BudgetSettings,
    pub gmail: GmailSettings,
    pub autonomy: AutonomySettings,
    pub projection_default: ProjectionMode,
    pub privacy_default: PrivacyMode,
    /// Derived at read time, never written: the privacy a new account and
    /// the sample mailbox actually start in. `privacy_default` once a judge
    /// key is configured, Local-only until then: the stored default is
    /// Cloud judgment, and with no judge that promised what nothing
    /// provided (`effective_privacy`).
    #[serde(default)]
    pub effective_privacy_default: PrivacyMode,
    /// When an account's first listing completes, turn on the rule packs
    /// every mailbox starts with (`[detect] always`), Active. On in
    /// production and dev; test boots turn it off so a fixture mailbox
    /// carries only the Meshes a test installs, because Core's throwaway
    /// domain rule caught every `.example` sender first (17 Oct 2026).
    pub install_packs_on_first_sync: bool,
    pub launch_at_login: bool,
    pub close_to_hide: bool,
    #[serde(default)]
    pub comparison: ComparisonSettings,
}

impl Default for Settings {
    /// Production defaults. Dev builds use `dev_defaults()`; the store merges
    /// the right set under a loaded file.
    fn default() -> Self {
        Self::prod_defaults()
    }
}

impl Settings {
    pub fn prod_defaults() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            developer_mode: false,
            ui_level: UiLevel::Essentials,
            log_level: LogLevel::Info,
            notify_priority_threshold: 70,
            notifications: NotificationSettings::default(),
            priority_inbox: PriorityInboxSettings::default(),
            shadow_period_days: 7,
            judge: JudgeSettings::default(),
            llm: LlmSettings::default(),
            budgets: BudgetSettings::default(),
            gmail: GmailSettings::default(),
            autonomy: AutonomySettings {
                default: Autonomy::Suggest,
                ..Default::default()
            },
            projection_default: ProjectionMode::Full,
            privacy_default: PrivacyMode::CloudJudgment,
            effective_privacy_default: PrivacyMode::LocalOnly,
            install_packs_on_first_sync: true,
            launch_at_login: false,
            close_to_hide: true,
            comparison: ComparisonSettings::default(),
        }
    }

    /// Dev builds start in OBSERVE (§59), log at debug, show Everything and
    /// resolve the judge key and the LLM's credentials from the owner's
    /// AWS profile. Developer is not the default even here: the level is
    /// the person's choice, and a file that already says
    /// `developerMode = true` loads as Developer (`parse_settings`), so
    /// nobody loses the pages they had.
    pub fn dev_defaults() -> Self {
        Self {
            developer_mode: false,
            ui_level: UiLevel::Everything,
            log_level: LogLevel::Debug,
            judge: JudgeSettings::dev_defaults(),
            llm: LlmSettings::dev_defaults(),
            autonomy: AutonomySettings {
                default: Autonomy::Observe,
                ..Default::default()
            },
            ..Self::prod_defaults()
        }
    }

    /// The privacy a new account starts in: the stored default once a
    /// judge key is configured, Local-only until then. Read, never stored.
    pub fn effective_privacy(&self) -> PrivacyMode {
        if self.judge.api_key_present {
            self.privacy_default
        } else {
            PrivacyMode::LocalOnly
        }
    }

    pub fn defaults_for(dev: bool) -> Self {
        if dev {
            Self::dev_defaults()
        } else {
            Self::prod_defaults()
        }
    }

    /// The level is the one switch: `developer_mode` is true exactly when
    /// `ui_level` is Developer. Run on every load and every save, so a
    /// client that sends one without the other, or a hand-edited file where
    /// they disagree, cannot leave the rail and the Developer page reading
    /// different answers. A file written before the level existed is the
    /// one case where the flag speaks first, and `parse_settings` handles it
    /// before calling this.
    pub fn reconcile_level(&mut self) {
        self.developer_mode = self.ui_level.is_developer();
    }

    /// Validation before any write. Errors are `invalid` with the field named.
    pub fn validate(&self) -> Result<()> {
        let mut problems = Vec::new();
        for (name, v) in [
            ("budgets.dailyJudgeUsd", self.budgets.daily_judge_usd),
            ("budgets.dailyLlmUsd", self.budgets.daily_llm_usd),
            (
                "budgets.escalationPerDigUsd",
                self.budgets.escalation_per_dig_usd,
            ),
        ] {
            if !v.is_finite() || v < 0.0 {
                problems.push(format!("{name} must be a number ≥ 0"));
            }
        }
        if self.notify_priority_threshold > 100 {
            problems.push("notifyPriorityThreshold must be between 0 and 100".into());
        }
        if self.notifications.brief_times.len() > 4 {
            problems.push("notifications.briefTimes allows at most four times".into());
        }
        for t in &self.notifications.brief_times {
            if NotificationSettings::parse_time(t).is_none() {
                problems.push(format!("notifications.briefTimes: {t:?} is not HH:MM"));
            }
        }
        if self.notifications.due_within_hours > 24 * 14 {
            problems.push("notifications.dueWithinHours must be at most 336".into());
        }
        if self
            .priority_inbox
            .min_find_priority
            .is_some_and(|p| p > 100)
        {
            problems.push("priorityInbox.minFindPriority must be between 0 and 100".into());
        }
        if self.shadow_period_days > 365 {
            problems.push("shadowPeriodDays must be at most 365".into());
        }
        if self.judge.model.trim().is_empty() {
            problems.push("judge.model must not be empty".into());
        }
        if self.llm.escalation_model.trim().is_empty() {
            problems.push("llm.escalationModel must not be empty".into());
        }
        if self.llm.director_model.trim().is_empty() {
            problems.push("llm.directorModel must not be empty".into());
        }
        if self.judge.key_source == KeySource::Ssm
            && self
                .judge
                .ssm_parameter
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
        {
            problems.push("judge.ssmParameter is required when keySource is ssm".into());
        }
        if self.llm.provider == LlmProvider::Bedrock && self.llm.region.trim().is_empty() {
            problems.push("llm.region must not be empty".into());
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(RiddleError::invalid(problems.join("; "))
                .with_detail(serde_json::json!({ "problems": problems })))
        }
    }

    /// The Action layer's policy (§51). `dev` and `writes` are runtime facts
    /// the integrator sets: `dev` from the build profile, `writes` preserved
    /// from `Runtime::writes()` across settings reloads.
    pub fn autonomy_policy(&self) -> AutonomyPolicy {
        AutonomyPolicy {
            default: self.autonomy.default,
            per_dig: self
                .autonomy
                .per_dig
                .iter()
                .map(|(k, v)| (DigId::from(k.as_str()), *v))
                .collect(),
            per_action: self
                .autonomy
                .per_action
                .iter()
                .map(|(k, v)| ((None, k.clone()), *v))
                .collect(),
            writes: riddle_core::ProviderWrites::Hold,
            dev: false,
        }
    }

    /// Digs (or the default) this candidate raises to Trusted relative to
    /// `self`. Non-empty means the save needs press-and-hold (§51).
    pub fn trusted_raises(&self, candidate: &Settings) -> Vec<String> {
        let mut raised = Vec::new();
        if candidate.autonomy.default == Autonomy::Trusted
            && self.autonomy.default != Autonomy::Trusted
        {
            raised.push("default".to_string());
        }
        for (dig, level) in &candidate.autonomy.per_dig {
            if *level == Autonomy::Trusted
                && self.autonomy.per_dig.get(dig) != Some(&Autonomy::Trusted)
            {
                raised.push(dig.clone());
            }
        }
        raised
    }

    /// The file representation: derived presence flags are dropped so a
    /// secret's existence is never recorded on disk.
    pub(crate) fn to_toml_string(&self) -> Result<String> {
        let mut value = toml::Value::try_from(self)
            .map_err(|e| RiddleError::internal(format!("settings serialize: {e}")))?;
        if let Some(judge) = value.get_mut("judge").and_then(|v| v.as_table_mut()) {
            judge.remove("apiKeyPresent");
        }
        if let Some(gmail) = value.get_mut("gmail").and_then(|v| v.as_table_mut()) {
            gmail.remove("clientSecretPresent");
            gmail.remove("builtinClientAvailable");
        }
        // The schema version leads the file; the rest is sorted by the TOML writer.
        if let Some(table) = value.as_table_mut() {
            table.remove("schemaVersion");
            table.remove("effectivePrivacyDefault");
        }
        let body = toml::to_string_pretty(&value)
            .map_err(|e| RiddleError::internal(format!("settings serialize: {e}")))?;
        Ok(format!(
            "{FILE_HEADER}schemaVersion = {}\n{body}",
            self.schema_version
        ))
    }
}

/// Window geometry as last saved; the app rescues off-screen windows itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// The updater banner state machine (§6), persisted so a dismissed version
/// stays dismissed across launches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateState {
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub available_version: Option<String>,
    pub dismissed_version: Option<String>,
    /// The Update Preview (§6) has been applied or declined for this version.
    pub preview_decided_for: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OnboardingFlags {
    pub completed: bool,
    pub hotkeys_sheet_shown: bool,
    pub whats_new_seen_for: Option<String>,
    pub test_notification_verified: bool,
}

/// The runtime state file (`state.json`): things the app writes on its own
/// behalf. Never the place for sync cursors (those are per-Dig store rows)
/// or anything high-churn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub schema_version: u32,
    pub window: Option<WindowState>,
    pub update: UpdateState,
    pub last_seen_version: Option<String>,
    pub last_launch_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_clean_shutdown_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_sleep_detected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_wake_at: Option<chrono::DateTime<chrono::Utc>>,
    pub onboarding: OnboardingFlags,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            window: None,
            update: UpdateState::default(),
            last_seen_version: None,
            last_launch_at: None,
            last_clean_shutdown_at: None,
            last_sleep_detected_at: None,
            last_wake_at: None,
            onboarding: OnboardingFlags::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prod_defaults_match_the_spec() {
        let s = Settings::prod_defaults();
        assert_eq!(s.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(s.shadow_period_days, 7);
        assert_eq!(s.judge.model, "jev-1.13.0");
        assert_eq!(s.judge.key_source, KeySource::Keychain);
        assert_eq!(s.llm.region, "us-east-1");
        assert_eq!(s.budgets.daily_judge_usd, 1.0);
        assert_eq!(s.budgets.daily_llm_usd, 5.0);
        assert_eq!(s.autonomy.default, Autonomy::Suggest);
        assert!(s.close_to_hide);
    }

    #[test]
    fn dev_defaults_start_in_observe_and_log_at_debug() {
        let s = Settings::dev_defaults();
        assert_eq!(s.autonomy.default, Autonomy::Observe);
        assert_eq!(s.log_level, LogLevel::Debug);
        assert_eq!(s.ui_level, UiLevel::Everything);
        assert!(!s.developer_mode, "Everything is not Developer");
    }

    #[test]
    fn production_defaults_carry_no_ssm_profile_and_the_dev_profile_takes_it_from_the_environment()
    {
        let prod = Settings::prod_defaults();
        assert_eq!(prod.judge.key_source, KeySource::Keychain);
        assert_eq!(prod.judge.ssm_profile, None);
        assert_eq!(prod.judge.ssm_parameter, None);
        assert_eq!(
            prod.llm.profile, "",
            "the default AWS chain, not the owner's profile"
        );
        assert!(prod.judge.not_set_up(), "no key until one is pasted");
        prod.validate()
            .expect("a keychain source needs no SSM parameter");
        assert!(prod.install_packs_on_first_sync);
        assert!(Settings::dev_defaults().install_packs_on_first_sync);
        let aws = DevAws::from_lookup(|name| match name {
            DEV_AWS_PROFILE_VAR => Some("owner".into()),
            DEV_JEV_SSM_PARAMETER_VAR => Some("/owner/jev_api_key".into()),
            _ => None,
        });
        let judge = JudgeSettings::dev_defaults_for(&aws);
        assert_eq!(judge.key_source, KeySource::Ssm);
        assert_eq!(judge.ssm_profile.as_deref(), Some("owner"));
        assert_eq!(judge.ssm_parameter.as_deref(), Some("/owner/jev_api_key"));
        assert_eq!(LlmSettings::dev_defaults_for(&aws).profile, "owner");
        assert!(
            !judge.not_set_up(),
            "an SSM lookup is a lookup, not a missing setup"
        );
        let unnamed = DevAws::from_lookup(|_| Some("  ".into()));
        let judge = JudgeSettings::dev_defaults_for(&unnamed);
        assert_eq!(
            judge.key_source,
            KeySource::Keychain,
            "without the owner's environment the dev profile is the production profile"
        );
        assert_eq!(judge.ssm_profile, None);
        assert_eq!(judge.ssm_parameter, None);
        assert_eq!(LlmSettings::dev_defaults_for(&unnamed).profile, "");
        let mut null = prod.clone();
        null.judge.provider = JudgeProvider::Null;
        assert!(null.judge.not_set_up());
        let mut pasted = prod;
        pasted.judge.api_key_present = true;
        assert!(!pasted.judge.not_set_up());
    }

    #[test]
    fn the_built_in_google_client_is_absent_by_default_and_an_env_baked_one_counts_as_configured() {
        let s = GmailSettings::default();
        assert!(s.use_builtin_client);
        assert!(!s.builtin_client_available);
        assert_eq!(GmailSettings::builtin_client_from(None, None), None);
        assert_eq!(
            GmailSettings::builtin_client_from(Some("  "), Some("x")),
            None,
            "a blank id is no client"
        );
        assert_eq!(s.effective_client_with(None), None);
        let baked = GmailSettings::builtin_client_from(
            Some("123-abc.apps.googleusercontent.com"),
            Some("GOCSPX-secret"),
        )
        .expect("a baked-in client");
        assert!(baked.builtin);
        assert_eq!(baked.client_secret.as_deref(), Some("GOCSPX-secret"));
        assert!(
            !format!("{baked:?}").contains("GOCSPX"),
            "Debug never prints the secret"
        );
        let effective = s
            .effective_client_with(Some(baked.clone()))
            .expect("configured");
        assert_eq!(effective.client_id, "123-abc.apps.googleusercontent.com");
        assert!(effective.builtin);
        // Bring your own: an own id wins, and turning the built-in one off
        // leaves nothing when there is no own id.
        let mut own = s.clone();
        own.client_id = "own.apps.googleusercontent.com".into();
        let chosen = own.effective_client_with(Some(baked.clone())).unwrap();
        assert_eq!(chosen.client_id, "own.apps.googleusercontent.com");
        assert!(!chosen.builtin);
        assert_eq!(
            chosen.client_secret, None,
            "an own client's secret lives in the keychain"
        );
        let mut off = s;
        off.use_builtin_client = false;
        assert_eq!(off.effective_client_with(Some(baked)), None);
    }

    #[test]
    fn the_effective_privacy_default_is_local_only_until_a_judge_key_is_configured() {
        let mut s = Settings::prod_defaults();
        assert_eq!(s.privacy_default, PrivacyMode::CloudJudgment);
        assert_eq!(s.effective_privacy(), PrivacyMode::LocalOnly);
        s.judge.api_key_present = true;
        assert_eq!(s.effective_privacy(), PrivacyMode::CloudJudgment);
        s.privacy_default = PrivacyMode::CloudReasoning;
        assert_eq!(s.effective_privacy(), PrivacyMode::CloudReasoning);
    }

    #[test]
    fn production_starts_at_essentials_and_the_level_is_the_one_switch() {
        let s = Settings::prod_defaults();
        assert_eq!(s.ui_level, UiLevel::Essentials);
        assert!(!s.developer_mode);
        let mut dev = s.clone();
        dev.ui_level = UiLevel::Developer;
        dev.reconcile_level();
        assert!(dev.developer_mode, "Developer implies the flag");
        let mut disagree = s;
        disagree.developer_mode = true;
        disagree.ui_level = UiLevel::Everything;
        disagree.reconcile_level();
        assert!(!disagree.developer_mode, "the level wins over the flag");
    }

    #[test]
    fn validation_names_every_problem() {
        let mut s = Settings::prod_defaults();
        s.budgets.daily_judge_usd = -1.0;
        s.notify_priority_threshold = 101;
        s.judge.model = "  ".into();
        let err = s.validate().unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        let problems = err.detail.unwrap()["problems"].as_array().unwrap().len();
        assert_eq!(problems, 3);
    }

    #[test]
    fn the_priority_inbox_defaults_to_starred_and_finds_at_70_and_no_threshold_survives_toml() {
        let s = Settings::prod_defaults();
        assert_eq!(s.priority_inbox.labels, vec!["STARRED".to_string()]);
        assert!(s.priority_inbox.strata.is_empty());
        assert_eq!(s.priority_inbox.min_find_priority, Some(70));
        assert!(s.priority_inbox.find_types.is_empty());
        let mut any = s.clone();
        any.priority_inbox.min_find_priority = None;
        let text = any.to_toml_string().unwrap();
        assert!(text.contains("minFindPriority = 0"), "{text}");
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.priority_inbox.min_find_priority, None);
        let mut high = s;
        high.priority_inbox.min_find_priority = Some(101);
        assert_eq!(
            high.validate().unwrap_err().kind,
            riddle_errors::ErrorKind::Invalid
        );
    }

    #[test]
    fn the_file_form_never_carries_presence_flags() {
        let mut s = Settings::prod_defaults();
        s.judge.api_key_present = true;
        s.gmail.client_secret_present = true;
        s.gmail.builtin_client_available = true;
        s.effective_privacy_default = PrivacyMode::CloudReasoning;
        let text = s.to_toml_string().unwrap();
        assert!(!text.contains("apiKeyPresent"));
        assert!(!text.contains("clientSecretPresent"));
        assert!(!text.contains("builtinClientAvailable"), "{text}");
        assert!(!text.contains("effectivePrivacyDefault"), "{text}");
        assert!(text.contains("useBuiltinClient = true"), "{text}");
        assert!(text.contains("schemaVersion = 1"));
    }

    #[test]
    fn autonomy_policy_maps_per_dig_and_per_action_levels() {
        let mut s = Settings::prod_defaults();
        s.autonomy.per_dig.insert("dig-1".into(), Autonomy::Act);
        s.autonomy
            .per_action
            .insert("archive_messages".into(), Autonomy::Trusted);
        let p = s.autonomy_policy();
        assert_eq!(
            p.level_for(Some(&DigId::from("dig-1")), "move_to_spoil"),
            Autonomy::Act
        );
        assert_eq!(
            p.level_for(Some(&DigId::from("dig-2")), "archive_messages"),
            Autonomy::Trusted
        );
        assert_eq!(p.level_for(None, "move_to_spoil"), Autonomy::Suggest);
    }

    #[test]
    fn trusted_raises_lists_the_default_and_each_dig_raised() {
        let current = Settings::prod_defaults();
        let mut next = current.clone();
        next.autonomy.default = Autonomy::Trusted;
        next.autonomy
            .per_dig
            .insert("dig-1".into(), Autonomy::Trusted);
        next.autonomy.per_dig.insert("dig-2".into(), Autonomy::Act);
        assert_eq!(
            current.trusted_raises(&next),
            vec!["default".to_string(), "dig-1".to_string()]
        );
        assert!(next.trusted_raises(&next).is_empty());
    }
}
