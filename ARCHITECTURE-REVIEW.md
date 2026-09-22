# Riddle — Review of README.md and ARCHITECTURE.md

Reviewed: 2026-09-21. Scope: logical soundness, internal consistency, missing functional and non-functional requirements. Line references are to the current files.

**Resolution (2026-09-22).** The findings below have been applied to `README.md` and `ARCHITECTURE.md`. ARCHITECTURE was renumbered to plain integers on 2026-09-22 (sections 5 and above shifted; the review's section references were remapped by script). README references use section names rather than line numbers because the README was restructured. Each item in §6 and §7.5 is marked as applied.

## 1. Verdict

The idea is sound and the architecture is unusually strong on the things most AI products skip: provenance, undo, versioned policy, simulation, and a hard capability boundary (Actions). Those parts hold up.

The documents have five structural weaknesses that would surface as rework if building started from them as written:

1. **Jev is load-bearing but never defined.** Ten of the thirty-eight invariants and the whole Sift pipeline depend on a component the documents never specify. Jev is a real product (TypeSafe AI, early access since 15 September 2026) whose documented limits and weaknesses the architecture does not design for.
2. **Policy thresholds sit on uncalibrated numbers.** Meshes threshold Signal probabilities (`review_requested > .80`) with no calibration methodology, no pinning of thresholds to a Signal version and model version, and no plan for what happens when the underlying model changes.
3. **"Local-first desktop" conflicts with the core promise.** A desktop app that is closed or asleep cannot sift mail. The "Good morning" experience, multi-device consistency, and "what happened while I was away" all depend on processing that the stated topology cannot do while the user is away.
4. **No security threat model.** The Director reads untrusted content and holds tools that send mail, create rules, and run multi-step tasks. Indirect prompt injection, data exfiltration, and rule poisoning are not mentioned once.
5. **Several product concepts are named but not defined.** The classification model (what is exclusive, what overlaps), the Find lifecycle, the "uncertain" state, cold start, and how Riddle state is projected onto Gmail are each referenced but never specified. Each one changes the data model.

The rest of this document itemises the findings. Section 2 is internal contradictions and logical gaps. Section 3 is missing functional requirements. Section 4 is missing non-functional requirements. Section 5 is externally verified platform and research constraints. Section 6 is a prioritised list of edits. Section 7 audits both documents against the desktop application playbook at `APPLICATION_FEATURES.md` in this project (master copy at `~/Projects/application_features.md`) and lists the proven patterns, with source files, that Riddle should reuse.

## 2. Logical flaws and internal contradictions

### 2.1 Jev is never defined, and its documented limits are not designed for

Neither document says what Jev is. It is TypeSafe AI's "System One" decision model, released in early access on 15 September 2026 (details and sources in §5.1). That matters because the vendor documents concrete limits and weaknesses that the architecture currently contradicts or ignores:

- **State window.** 32k tokens for state plus the longest question, 64k per request. §25 evaluates a whole Signal Set "against the same message/context in one operation". A long thread plus retrieved Context can exceed that, and the vendor says accuracy "falls as the state grows with content unrelated to the decision". The architecture needs a per-Signal-Set state builder with a token budget, not an implicit "the message and its Context".
- **Adversarial content.** The vendor states Jev "does not treat it as hostile by default". Every Signal is therefore steerable by the sender. See 2.17 and §5.3.
- **Dates read as text.** Jev "reads dates as text, not as ordered quantities". Any Signal that implies a date comparison (overdue, upcoming, stalled) must be computed deterministically and passed in as a fact, not asked as a question.
- **Literal reading and contradictory instructions.** Signal definitions are the prompt. The Signal Registry needs authoring rules (one property per Signal, no negations, no "and/or" in a definition) and the adversarial tests in §32 should target these failure modes specifically.
- **Score primitive is poorly calibrated.** The vendor says "score levels are weak in numerical calibration" and a third-party audit measured it as barely above chance on a policy-derived task while reporting a mean probability of .74. Signals should use Choice and Noul (boolean) primitives only.
- **Language.** English is primary; other languages are "less reliable". A third-party audit measured an 11-point accuracy drop and 3x calibration error on Russian inference tasks. Riddle needs a language fact and a per-language policy (lower autonomy or LLM path for non-English mail).
- **Rate limits and versioning.** 1,200 requests per minute, 250k tokens per second, a single model version (`jev-1.13.0`) with `jev-latest` and `jev-preview` aliases. Sift must pin the exact version, never an alias, and historical backfill must be paced.

Separately, §95 invariant 18 says Jev "remains replaceable", but the architecture never defines the interface it would be replaced behind. §56 versions models and §27 versions Signal definitions, but nothing versions the *evaluator*. A cached `requires_response:v3 = YES .94` is only reusable if the same engine and model version produced it. The cache key in §26 must include engine and model identity. Also note that Anthropic's API does not expose token log-probabilities, so an LLM-backed replacement would have to use verbalised probabilities or a self-hosted model; the replacement path is not free.

Recommendation: add a "Semantic Judgment Engine" section that (a) names Jev and links the vendor docs, (b) defines the Rust trait Sift depends on (state, typed questions, per-question distribution, model identity, usage, latency, determinism flag, calibration map), and (c) records the known weaknesses as design constraints.

### 2.2 Thresholds on uncalibrated probabilities

§2 and §75 show Mesh policy such as `IF review_requested > .80`. §39 says thresholds vary by decision. Nowhere does the architecture say how a threshold is chosen, validated, or maintained. Three problems follow:

- **Calibration is assumed.** A score of .80 from an LLM-style judge does not mean 80% of such cases are positive unless the score has been calibrated on labelled data. Without that, `> .80` is an arbitrary knob per Signal.
- **Thresholds are coupled to versions but Meshes reference Signals by name only.** When `review_requested` moves from v3 to v4 (§27), every Mesh threshold tuned against v3 is silently invalidated. Meshes must pin a Signal version or promotion must trigger re-simulation of every dependent Mesh.
- **Model drift.** Provider models are updated and deprecated. §64 replay comparisons across time are confounded by this unless the model identity is recorded with every evaluation and thresholds are re-derived per (Signal version × model).

Recommendation: state that every Signal version carries a calibration artifact (labelled set, method, date, model) and that Mesh thresholds are expressed against calibrated scores. See §5.3.

### 2.3 The Signal cache is only valid for context-free Signals

§26 claims "Changing a Mesh usually should not require rerunning Jev" and §38 shows a simulation with "New Jev calls required 0". This is true for Signals that depend only on message content (`ci_failure`, `newsletter`). It is false for Signals that depend on Context (`personally_relevant`, `routine`, `known_sender`), because Context changes as the user's relationships, projects, and corrections evolve. A cached `personally_relevant:v3 = .91` from six months ago may be wrong today.

Recommendation: Signals declare their inputs (message-only, thread, Context slice). Cache keys include a hash or version of the Context slice consumed. Context-dependent Signals get a staleness policy.

### 2.4 The LLM escalation path bypasses the policy layer

The Sift diagram in §17 routes uncertain cases to the LLM and then directly to "Classification". §24 and invariant 13 say Signals are judgments and Meshes are policy. If the LLM emits a classification directly, then for the escalated tail the LLM *is* the policy, and Mesh versioning, simulation, and provenance no longer explain those decisions.

Two consistent designs exist. Either (a) the LLM refines Signals (re-estimates them with more Context) and the Mesh re-runs, keeping Meshes as the only policy authority; or (b) the LLM emits a classification and the architecture explicitly says Meshes only govern the confident path. The documents should pick one. Option (a) preserves the invariants.

Related ambiguity: "Escalate" means "to the LLM" in §2 and §39 but the Mesh example's `ELSE → Escalate` has no defined terminal behaviour if the LLM is also uncertain. See 2.6.

### 2.5 Actor and learning inconsistencies between README and ARCHITECTURE

- README, Activity & Provenance, first example: "Director moved 14 messages to Spoil, Triggered by Mesh". In the architecture (§17, §47) a Mesh firing inside Sift is actor SIFT or MESH, not DIRECTOR. The Director is the interactive agent. Conflating them defeats the provenance goal.
- README, the same example: "13 matched previous examples. 1 was classified semantically." Example-based matching (nearest-neighbour on prior corrections) is a distinct classification mechanism that appears nowhere in the architecture's pipeline (§17), decision mechanisms (§49), or crate list (§4). Either drop it from the product story or add it as a component.
- README, Activity & Provenance, second example: "Riddle updated Mesh: AWS Notifications based on your correction" (immediate rewrite). ARCHITECTURE §46: "Riddle should not blindly rewrite policy after every correction." The README example should show a proposal, or the architecture should permit single-correction updates for narrow Meshes.
- README, Mesh section says Meshes can be "learned from repeated corrections". The architecture's only learning mechanism is Director-proposed changes after accumulated evidence (§46). "Learned" implies something automatic. Clarify.

### 2.6 Autonomy levels contradict the product principles

§51 lists `create_mesh` as "autonomous + visible". A Mesh decides what the user never sees. It is the highest-leverage mutation in the system, higher than a single archive. Meanwhile §30 requires explicit approval to promote a Signal, and the README's Mesh Feedback flow (Mesh Feedback & Repair section) ends in "Preview · Apply", which is a suggest-then-approve flow. "Automation earns autonomy" (README) argues for SUGGEST as the default for any policy change.

Recommendation: Mesh creation and modification default to SUGGEST; ACT is unlocked per Mesh after a shadow period with an observed agreement rate.

### 2.7 The classification model is undefined

The documents use "classification" for at least four different things: a disposition (Find, Spoil, Contamination), a tag (Strata), a derived entity (Find), and a Signal evaluation. Questions the data model must answer and the documents do not:

- Are Find, Spoil, and Contamination mutually exclusive? Can a receipt be Spoil and in the Purchases Stratum? (README says "Receipts → Purchases" and lists Receipts under Spoil-like categories.)
- Where does ordinary mail go that is neither a Find nor Spoil, such as a conversation that is informational and already resolved? The morning summary (README, The Core Experience) has four buckets that sum exactly to 312, which implies "organized into your Strata" is a disposition, but Strata are defined as overlapping tags.
- What is the Mesh output vocabulary? README Mesh examples route to a Stratum ("Engineering Alerts"), ARCHITECTURE Mesh examples route to Find types, Spoil, or Escalate.

Recommendation: define Disposition as an enum {Find, Filed, Spoil, Contamination, Unresolved}, Strata as an independent multi-label set, and Find as an entity derived from one or more messages. Enumerate Mesh outputs as {Disposition, Find type, add Stratum, Notify, Escalate}.

### 2.8 No user-visible "uncertain" state

The product promise is "when Riddle says there's nothing you need to deal with, you should be able to trust it." The architecture has an escalation rate (§84 shows 5.2%) and a failure state `UNCLASSIFIED` (§88), but the README has no surface for mail Riddle could not decide on. Either those messages appear as Finds (inflating Finds and eroding trust in the Find list), get hidden (violating the promise), or need a third surface. The product needs an explicit "Riddle wasn't sure" tray with its own count in the morning summary, and the Attention Zero promise should be restated to include it.

### 2.9 Mesh conflict resolution is unspecified

§36 says "more specific policy may refine or override broader policy." Undefined:

- Two Meshes at the same scope match one message with different outcomes. First match? Priority? Most specific predicate?
- "Never classify family as Spoil" is not a routing rule. It is a veto (guard) that must run after routing regardless of scope order. The Mesh model needs at least two rule kinds: routes and guards.
- A Dig in two Dig Groups. Is that allowed, and if so which group's policy wins?
- What "refine" means operationally versus "override".

### 2.10 Sift is message-triggered but several Find types are time- or thread-triggered

Sift runs when a message arrives (§17, §53). These Find types cannot be produced or retired by an arrival-triggered pipeline alone:

- **Follow Up** ("a conversation has stalled") requires a timer: no reply after N days.
- **Attend** and **Pay** have due dates after which the Find should change state or expire.
- **Respond** must be retired when the user replies from another client. That is a provider-side change to the thread, not a new inbound message, and the sync path (§53) feeds only Sift.
- Snooze expiry.

There is no scheduler or "temporal Sift" component, and the Find structure in §41 has no lifecycle (open, snoozed, resolved, dismissed, expired, superseded) or auto-resolution rules. §54 durable jobs is the natural home, but it lists only batch work.

### 2.11 Find aggregation and cross-Dig duplication are unspecified

§41 says "twenty messages may become one Find" but no mechanism creates that grouping. Thread-based grouping is easy; topic-based grouping across threads ("the same production problem") requires clustering or LLM merging, and needs rules for when a new message joins an existing Find versus creates a new one. Separately, the same thread often arrives in two Digs (cc'd to work and personal). Without identity resolution across Digs the user gets duplicate Finds.

### 2.12 Riddle state is never projected onto the provider, or is, and neither is stated

§14 separates provider state from Riddle state. §11 says Gmail is authoritative for Gmail-owned state. The documents never say whether moving a message to Spoil changes anything in Gmail. Both answers have consequences:

- If Spoil is Riddle-only, the user's phone, Gmail web, and any other client still show all 238 messages in the inbox. The product's value only exists inside the desktop app.
- If Spoil archives and labels in Gmail, then every classification is a provider mutation that must be idempotent, undoable in Gmail, and reconciled when the user acts elsewhere (archives on the phone, applies a label in Gmail web). That is a bidirectional sync problem with conflicts, and §53 only describes inbound change tracking.

Also missing: an outbox. Local-first plus "Gmail is authoritative" means local mutations are queued and applied later. Offline behaviour, ordering, retries against a moved historyId, and the UI state of pending writes are not described. `PROVIDER` appears as a decision mechanism in §49 but is never elaborated. User actions in other clients are a rich feedback source, but only if Riddle can tell a human move from a move made by a Gmail filter, another automation, or Riddle's own projection. SaneBox documents exactly this failure: another tool's reminders moving mail back to the inbox retrained it to treat those senders as important (§5.5). The sync layer needs to attribute provider-side changes before the learning layer sees them.

### 2.13 Undo is under-specified for the cases that matter

§50 covers the simple case. Not covered:

- Non-linear undo: undoing an Activity after later Activities touched the same messages (the user re-filed 3 of the 127 before pressing Undo All).
- Undo of a Mesh change that reclassified 439 messages, including any provider projections and any Finds created and since resolved.
- Whether undoing an action also retracts the correction signal it generated (otherwise undo teaches the wrong thing).
- Undo windows and expiry, and undo of actions with external effects (send is irreversible; Gmail has no server-side undo send).
- Undo when the provider state changed in between (message deleted or moved by another client).

### 2.14 "Riddle remains useful independently of Jev" has no fallback design

Invariant 37 is asserted, not designed. If Jev is removed, either the LLM handles all Signals (cost and latency change by an order of magnitude and the §84 economics collapse) or a different fast path exists. The document should name the fallback: deterministic facts plus Gmail's own categories plus sampled LLM evaluation, with a stated cost envelope.

### 2.15 Historical backfill cost is glossed over

§93 proposes a new Signal (step 6), tests it against historical mail (step 7), and then shows "Signal evaluations: All cached, Expected additional Jev cost: $0". A new Signal has no cache. Evaluating a new Signal over a 100k-message history is the dominant cost in that flow. The design needs a backfill policy: bounded windows, sampling, on-demand for older mail, and a per-Dig budget.

### 2.16 Evaluation corpora are biased in a way that hides the worst errors

§31 and §65 build corpora from user corrections and measure "observed agreement". Users only correct what they see. A message wrongly sent to Spoil is, by design, not seen, so the most damaging error class (missed Find) is systematically under-represented. Find recall cannot be measured from corrections. The evaluation design needs periodic sampled audits of Spoil and Filed mail with explicit labelling, and the trust promise needs a recall target derived from those audits.

### 2.17 Contamination is treated as a semantic judgment, which is the least safe path

§17 sends uncertain cases to an LLM, and Contamination is listed as a Riddle classification. Phishing and scam mail is the content most likely to contain adversarial instructions, so routing it through an LLM for judgment is exactly backwards, and the vendor states that Jev does not treat adversarial content as hostile either (§5.1). Contamination should be decided deterministically first (Gmail SPAM label, Authentication-Results, sender reputation, lookalike domains, first-time sender with a payment or credential ask), and anything marked Contamination should be excluded from Director and Expedition context by default.

There is also a cheaper attack than exfiltration that the documents do not consider: attention hijacking. A sender who writes "this requires your urgent response" is steering `requires_response` and `personally_relevant` directly. Marketing already does this to humans; with Jev it works on the classifier. Find creation from unknown or low-reputation senders needs a deterministic gate that no Signal can override. See §5.3.

### 2.18 Sent mail is never sifted

Sift is defined as understanding "incoming mail" (README, Sift section, ARCHITECTURE §2). Several product promises depend on understanding the user's own sent mail: "What commitments have I made over email this month?", "What am I waiting for other people to respond to?", and the Follow Up Find type. Waiting-for detection requires classifying sent messages (`asks_question`, `makes_commitment`, `requests_action`) and tracking whether a reply arrived. Add sent mail to the Sift scope and add a thread-state model (last speaker, open asks, my commitments).

### 2.19 Strata governance is missing

README says Strata "should emerge naturally" and ARCHITECTURE says they are overlapping categories. Emergent taxonomies drift: near-duplicate names, one-off categories, and inconsistent granularity. Nothing says who creates a Stratum, how one is merged or split, or whether Strata are Dig-scoped or global. Strata also lack versioning even though Meshes route to them.

### 2.20 Cold start is unaddressed

Day one has no Context, no corrections, no user profile, and no Meshes beyond system defaults. Invariant 21 says "shadow before autonomy" but the README never describes an onboarding or shadow period, and the architecture never says what Sift does with a brand-new Dig. A first-time user experience that immediately hides 238 messages on untested defaults is the fastest way to lose trust.

### 2.21 Background work has no concurrency model and no user-visible surface

§54 says long-running work "should be modeled as durable jobs" and lists nine job types. §4 names a `riddle-jobs` crate and §11 a `jobs` table. §87 and §88 expose queue depths, Jev concurrency, rate-limit utilisation, and failures, but only in Developer Mode. That is the entire treatment. Riddle is a system whose normal state is "several long jobs running at once while the user works", and nothing describes how those jobs share resources or how the user sees them.

**Resource contention is unmodelled.** Every job competes for the same four limited resources, and each has a different ceiling:

- Gmail API: 6,000 units per user per minute, so about 300 message fetches per minute per Dig (§5.2).
- Jev: 1,200 requests per minute and 250k tokens per second, shared across all Digs and all job types (§5.1).
- The LLM provider: rate limits and a cost budget.
- SQLite: one writer at a time; a long write transaction from a backfill job blocks every UI mutation.

Without priority classes and per-resource rate limiters, a 100k-message historical Sift will starve the interactive path. The user asking the Director a question, or an incoming message that should become a Find within seconds, waits behind a backfill. The document needs an explicit ordering: interactive Director requests, then incoming-mail Sift, then wake-up catch-up (newest first, headers before bodies), then historical backfill, then experiments and shadow evaluation. Fairness across Digs, back-pressure when a resource saturates, and pre-emption of low-priority work all need stating.

**Durability is asserted, not designed.** "Where appropriate, jobs should resume after Riddle closes and reopens" (§54) requires checkpointed progress, idempotent steps (§52 covers Actions but not job steps), retry with backoff, dependencies between jobs (embeddings after bodies, Mesh simulation after Signal backfill), cancellation that leaves consistent state, and pause on sleep, network loss, or provider outage.

**The user has no view of any of it.** The README describes the morning summary as if processing were complete, but on the stated topology the app wakes with a backlog and works through it for minutes to hours (§5.4). JetBrains IDEs solve exactly this problem for indexing: a status-bar indicator with aggregate progress, a task list with per-task progress, estimated time, pause and cancel, and a notification on completion. Riddle needs the equivalent as a product feature, not a Developer Mode panel:

- a persistent indicator showing that Riddle is caught up, catching up, or degraded (provider or model unavailable)
- a task list with progress and cancel for initial sync, catch-up Sift, historical backfill, Mesh simulation, Expeditions, embeddings, and bulk Actions
- honesty in the summary itself: "7 Finds so far; 1,240 of 4,981 messages still being sifted" rather than a count that implies completion, because Attention Zero cannot be promised for mail that has not been processed
- an onboarding progress view for the multi-hour initial sync, since the first session is otherwise a blank screen

**The threading model for the Rust core is unstated.** Tauri commands run on a tokio runtime (§5.4); CPU-bound work (HTML parsing, quoted-reply stripping, local embeddings) must go to blocking threads or a dedicated pool, SQLite access needs a connection strategy (one writer, pooled readers, short transactions), and progress must flow to the webview over Channels rather than events. None of this is in §4 or §7.

Recommendation: add a "Jobs and Scheduling" section covering priority classes, per-resource rate limiters as first-class components, SQLite write discipline, checkpointing and cancellation semantics, and a user-facing Task Centre; add "the summary states whether processing is complete" to the README's core experience.

### 2.22 Minor inconsistencies

- README, Activity & Provenance, first example uses the Director as the actor for a Sift decision (see 2.5).
- "Escalate" is used for escalation to the LLM and, implicitly, to the user.
- The Signal Set concept (§25) and the "Signal Selection" pipeline stage (§17) are not tied to Meshes. Which Signals to evaluate should be derived from the Meshes in scope for the Dig plus a system baseline, otherwise every message either pays for every Signal or the selection is a guess.
- README "Engineering Alerts" is a Mesh target that is neither a Find type nor Spoil; it is presumably a Stratum, which means Meshes need a Stratum output (2.7).
- The crate list (§4) has no home for scheduling, people/relationship modelling, redaction, credentials, or embeddings.

## 3. Missing functional requirements

Each item below is absent from both documents or mentioned only in passing.

1. **Classification model and Mesh output vocabulary** (2.7).
2. **Find lifecycle**: states, auto-resolution when the user replies anywhere, expiry for dated Finds, snooze, priority ordering across Digs, merging and splitting.
3. **Unresolved surface**: a tray for mail Riddle could not decide, counted in the morning summary (2.8).
4. **Onboarding and shadow period**: what is synced first (recent N days), what runs in shadow, when autonomy unlocks per Mesh, and how the user is shown what *would* have happened.
5. **User profile / self-model**: the user's addresses and aliases, group addresses they receive via, role, organisation, active projects, VIPs, family. `personally_relevant` and `directly_addressed` are undefined without it. Nothing in the domain state list (§11) holds it.
6. **People and relationship model**: sender reputation, first-contact screening, mailing-list detection, organisation detection, contact import. §11 lists `participants` and `contacts` tables but no behaviour.
7. **Provider projection policy**: which Riddle states map to Gmail labels or archive, conflict rules, an offline outbox, and treating actions in other clients as corrections (2.12).
8. **Temporal engine**: scheduled re-evaluation for follow-ups, due dates, event times, snooze expiry (2.10).
9. **Digest and notification policy**: Spoil digest cadence, which Finds may interrupt the user, quiet hours, per-Dig settings.
10. **Unsubscribe as a first-class Action** using List-Unsubscribe headers, surfaced from Spoil summaries.
11. **Deterministic Facts vocabulary**: List-Id, List-Unsubscribe, Precedence, Auto-Submitted, Authentication-Results, Reply-To mismatch, Gmail category labels, calendar MIME parts, structured-data markup, first-time sender, thread statistics, whether the user has replied in the thread.
12. **Normalization spec**: HTML to text, quoted-reply and signature stripping, size caps, attachment handling, language detection. Quoted-reply stripping in particular determines whether `requires_response` works on long threads.
13. **Sent mail in Sift scope** and a thread-state model (2.18).
14. **Director and Expedition UX**: approval prompts, budgets (cost, time, API calls), progress, cancellation, resume, output as Finds, scope across Digs, and how Expeditions interact with concurrent Sift.
15. **Corrections taxonomy**: the set of user gestures that count as corrections, what each teaches, and how Riddle asks for a reason without nagging.
16. **Search spec**: operator parity with Gmail search, cross-Dig search, and the scope of the semantic index.
17. **Multi-Dig identity resolution** and duplicate suppression (2.11).
18. **Data management**: remove a Dig and purge everything derived from it (including traces and exports), export, backup and restore, retention for traces and evaluations, storage caps.
19. **Autonomy and privacy settings**: per-Dig and per-Action autonomy level; per-Dig cloud AI exclusion. §57 calls this "future", but a work Dig on an employer account makes it a launch requirement.
20. **Table-stakes client features and explicit scope**: compose with attachments and rich text, reply quoting, signatures, send-as aliases, contacts autocomplete, calendar invite handling, threading, notifications, keyboard shortcuts, offline reading, undo send, print, multiple windows. The README says "build an excellent email client" and then describes none of it. An explicit in/out scope list for v1 is needed.
21. **Strata governance** (2.19).

## 4. Missing non-functional requirements

1. **Security threat model.** Indirect prompt injection via mail content, exfiltration channels (drafts, sends, rendered links and images in Director output), rule and memory poisoning through corrections and proposed Meshes, capability separation for the Director when it has ingested untrusted content, HTML email rendering sandbox and remote-image blocking, attachment handling, OAuth token storage, local database encryption, Developer Mode export redaction, LLM provider data retention terms. See §5.3.
2. **Privacy and compliance.** Google API Services User Data Policy "Limited Use" obligations for Gmail data, Workspace administrator restrictions on third-party apps, per-Dig cloud exclusion, redaction rules for what leaves the machine, telemetry and crash-report scrubbing, data deletion. See §5.2.
3. **Google platform constraints.** OAuth app verification and security assessment, the unverified-app user cap, testing-mode token expiry, quota budgeting, history-cursor expiry and the full-resync path, absence of push for a serverless desktop app and the resulting polling cadence, batch limits. See §5.2.
4. **Performance targets.** Per-message Sift latency, burst throughput (a 5,000-message backlog after a week away), initial sync time for a 100k-message mailbox, UI responsiveness with large lists, memory and disk budgets, idle CPU and battery, cost per 1,000 messages, LLM escalation rate target.
5. **Resilience and degradation.** Offline mode, provider outage, model-provider outage or rate limiting (mail must never be hidden because a classifier was unavailable), retry and backoff, crash-safe transactions, schema migrations.
6. **Background execution model.** Menu-bar or tray agent, launch at login, sleep and wake catch-up, macOS App Nap, and an honest statement that nothing is processed while the machine is off. See §5.4.
7. **Multi-device.** Declare single-device for v1 and state the consequence (Riddle state lives on one machine), or design state sync. See §5.4.
8. **Observability volume and retention.** Storing raw request and response traces per Jev evaluation per message multiplies the mailbox size. Reference content by message ID and prompt template version; define retention.
9. **Evaluation methodology.** Ground-truth labelling, sampled audits for recall, metric targets that define "Attention Zero", promotion gates for Signals and Meshes, handling non-determinism and model drift.
10. **Internationalisation and accessibility.** Non-English mail (Signal quality by language), locale and timezone for deadlines, keyboard-first operation, screen reader support.
11. **Cost governance.** Per-Dig budgets, hard caps, alerts, model selection policy, and a stated target cost per user per month.
12. **Testing strategy.** Fixture mailboxes, a synthetic mail generator, an offline replay harness, Action contract tests, idempotency tests.
13. **Operability.** Auto-update with signed releases, crash reporting, a redacted diagnostics bundle.

## 5. Externally verified constraints

Facts below were checked against primary sources on 2026-09-21. Items marked UNVERIFIED come from secondary sources.

### 5.1 Jev (TypeSafe AI)

Sources: vendor launch post (https://typesafe.ai/blog/introducing-system-one-models-and-jev), models page (https://docs.typesafe.ai/models), confidence page (https://docs.typesafe.ai/confidence), known-weaknesses page (https://docs.typesafe.ai/model-jaggedness/jev-1.13), Simon Willison's write-up (https://simonwillison.net/2026/Sep/21/jev/).

| Property | Value |
|---|---|
| Status | Early access since 15 Sep 2026 |
| Model | `jev-1.13.0`; aliases `jev-latest`, `jev-preview` |
| Primitives | Choice (option + probabilities + confidence), Score (rubric level + probabilities + confidence), Noul (true/false as 0 to 1) |
| Input | Text only: string, JSON object, or array. No images |
| Limits | 64k tokens per request; 32k for state plus the longest question; cardinality up to 255 |
| Rate limits | 1,200 requests/min; 250k tokens/s; "adjusting dynamically" |
| Latency | 70 to 500 ms claimed |
| Price | $0.042 per million input tokens; output free |
| Training | "Not trained on customer requests or responses" |
| Language | English primary; others "less reliable" |

Vendor-documented weaknesses that constrain Riddle's design: literal reading of questions, unreliable counting and arithmetic, dates read as text, weak on double negatives and indirection, accuracy falls as irrelevant state grows, adversarial content is not treated as hostile by default, contradictory instructions degrade results, "score levels are weak in numerical calibration".

The vendor's own `confidence` field is a derived statistic: for three options it is `(3 × max probability − 1) / 2`. The confidence page gives no calibration metrics and says thresholds "depend on your domain and the performance of the model for your use case".

Third-party audits (both public repositories, both verified to exist and to report these figures):

- https://github.com/scienthoon/jev-ood-calibration — on in-distribution benchmarks ECE is 0.024 to 0.032. On a synthetic support-ticket task: Choice ECE 0.082 (overconfident), Boolean ECE 0.079 (underconfident), Score ECE 0.325 with 44.7% accuracy and a mean stated probability of 0.74. The author's conclusion: "Don't threshold on the `confidence` field"; it calibrated worse than max probability.
- https://github.com/AHTOOOXA/jev-cyrillic-audit — XNLI accuracy 88.3% English versus 77.3% Russian; ECE 0.032 versus 0.096. Intent classification showed no measurable difference. Russian cost about 3x the tokens.

Cost sanity check for the architecture's economic hypothesis: a 2,000-token state for each of 100,000 historical messages is 200M input tokens, or about $8.40 at list price, before re-runs. That supports §20's hypothesis for the Jev leg. The initial-sift cost will be dominated by the LLM escalation tail (5% of 100k messages is 5,000 LLM calls), so the escalation budget, not Jev, is the number to design around.

Open re-implementations that prove replaceability exists in principle: simple-jev (any open model behind a Jev-compatible endpoint via next-token logits), Verdict-open-jev (ModernBERT), openjev. UNVERIFIED beyond the repositories existing.

### 5.2 Gmail API and Google policy

Sources: https://developers.google.com/workspace/gmail/api/guides/sync, .../reference/rest/v1/users.history/list, .../guides/push, .../reference/quota, .../guides/batch, .../auth/scopes, https://developers.google.com/identity/protocols/oauth2/native-app, https://support.google.com/cloud/answer/7454865, https://support.google.com/cloud/answer/15549945, https://developers.google.com/workspace/workspace-api-user-data-developer-policy, https://knowledge.workspace.google.com/admin/apps/control-which-apps-access-google-workspace-data.

**Sync.** A historyId "is typically valid for at least a week, but in some rare circumstances may be valid for only a few hours". An expired id returns 404 and the client "should perform a full sync". History is ordered by id with gaps; the generic `messages[]` field may duplicate entries, so consumers should read the typed `messagesAdded`, `messagesDeleted`, `labelsAdded`, `labelsRemoved` arrays. Whether draft saves surface in history is not documented (UNVERIFIED). Implication: ARCHITECTURE §53 needs a full-resync path and reconciliation, and "what happened while I was away" is exactly the case that will hit it.

**Push.** Push notifications use Cloud Pub/Sub, require a Cloud project topic, and `watch` must be renewed at least every 7 days. A desktop app without a server cannot safely hold the subscriber credentials. Riddle must poll. `history.list` costs 2 units against 6,000 units per user per minute, so polling every 30 to 60 seconds is cheap.

**Quota** (verified on the quota page):

| Limit | Value |
|---|---|
| Per project | 1,200,000 units/min |
| Per user per project | 6,000 units/min |
| Daily threshold | 80,000,000 units/day/project; overage billing announced for 2026 with 90 days' notice |
| `messages.get` | 20 units |
| `threads.get` | 40 units |
| `messages.list` | 5 units |
| `history.list` | 2 units |
| `messages.modify` | 5 units |
| `messages.batchModify` | 50 units, max 1,000 ids |
| `drafts.create` | 10 units |
| `messages.send` | 100 units |
| Batch HTTP | max 100 calls per batch; batching does not reduce units |

Implication: at 20 units per `messages.get`, one account can fetch about 300 messages per minute, so a 100k-message initial sync takes over 5 hours of wall time regardless of local speed. Every install shares one Cloud project's daily threshold. Labels are capped at 5,000 per mailbox.

**OAuth and verification.** `gmail.modify`, `gmail.readonly`, `gmail.compose`, `gmail.metadata`, `gmail.insert`, `gmail.settings.*` and `mail.google.com` are restricted scopes; `gmail.send` is sensitive; `gmail.labels` is non-sensitive. Restricted scopes require app verification; an unverified app is capped at 100 users. In Testing status, "authorizations by a test user will expire seven days from the time of consent". A security assessment (CASA, assurance levels AL1/AL2, revalidated annually) is required when the app stores restricted-scope data on servers "(or transmit)". Whether sending message content to a cloud LLM counts as "transmit" is the question Riddle must answer before choosing a cloud path; assessor pricing is UNVERIFIED (third parties quote roughly $500 to $3,000 per year). Desktop apps cannot keep a client secret; loopback redirect with PKCE is the supported flow. Each Google account allows 100 refresh tokens per client ID.

**Limited Use policy** (Workspace API User Data and Developer Policy, updated 2026-09-03). Transfers of data are allowed only "to provide or improve your appropriate use case or user-facing features that are visible and prominent in the requesting application's user interface and only with the user's consent". Prohibited: "using user data to create, train, or improve a machine learning or artificial intelligence model beyond that specific user's personalized model". The secure-environment requirements include "protecting against prompt injection techniques by either using Google Cloud Platform's Model Armor or other prompt injection protection" and encrypting tokens at rest. A Limited Use disclosure is mandatory in the app. Implication: third-party LLM inference is permissible for a prominent, consented feature with a vendor that does not train on the data, and prompt-injection defence is now a policy requirement, not only a security best practice.

**Workspace administrators** can mark apps Trusted, Limited, or Blocked and can block all unconfigured third-party apps. The user then sees the OAuth error `admin_policy_enforced`. Implication: "Work Gmail" Digs may be impossible for many employees unless an admin whitelists Riddle's client ID. The onboarding flow must handle this explicitly.

**Not available through the API.** Snooze (no method, no label; snoozed mail simply loses INBOX), schedule send, unsubscribe (headers are readable; the client must act on them itself). Available as labels: `IMPORTANT`, `SPAM`, `CATEGORY_PERSONAL`, `CATEGORY_SOCIAL`, `CATEGORY_PROMOTIONS`, `CATEGORY_UPDATES`, `CATEGORY_FORUMS`, all readable and applicable. Implication: Gmail's own categorisation and importance marker are free deterministic facts Riddle currently ignores, and snooze must be Riddle-only state (which §14 should list as an example).

**Content.** `format=full` returns parsed bodies and all stored headers, including `Authentication-Results` in practice (UNVERIFIED in official text). Attachments are fetched separately at 20 units each. Send size limit is 35 MiB via the API.

### 5.3 LLM calibration and prompt injection

Sources are linked inline; all checked 2026-09-21.

**Calibration.** Base-model token probabilities on multiple-choice tasks are reasonably calibrated and improve with scale (Kadavath et al. 2022, https://arxiv.org/abs/2207.05221), but post-training degrades this: the GPT-4 report shows the pretrained model well calibrated on MMLU and the RLHF model not (https://arxiv.org/abs/2303.08774). On RLHF models verbalised confidence beats token probabilities by about half the calibration error but remains overconfident (Tian et al. 2023, https://arxiv.org/abs/2305.14975; Xiong et al. 2024, https://arxiv.org/abs/2306.13063). The Claude Messages API exposes no log-probabilities (https://platform.claude.com/docs/en/api/messages/create), so an LLM-backed fallback for Jev would be verbalised numbers or sampling frequencies. Practical consequence for §2's `> .80`: it is an operating point on a ranking score, not an 80% chance, until a calibration map says otherwise.

**Making thresholds trustworthy.** Platt or temperature scaling needs a few hundred labels per Signal; isotonic regression needs about a thousand or it overfits (Niculescu-Mizil and Caruana 2005, https://www.cs.cornell.edu/~alexn/papers/calibration.icml05.crc.rev3.pdf). Split conformal prediction gives distribution-free coverage on any score with about a thousand calibration points (Angelopoulos and Bates, https://arxiv.org/abs/2107.07511); its output sets {YES}, {NO}, {YES, NO} map directly onto act, act, escalate, which fits §39 well. The guarantee is per (score × model × data distribution) and breaks silently on any model or prompt change. User corrections are a biased label source; each Signal needs a random audited sample.

**Drift and determinism.** GPT-4's accuracy on a fixed task moved from 84% to 51% between the March and June 2023 snapshots (Chen, Zaharia and Zou, https://arxiv.org/abs/2307.09009). OpenAI's `system_fingerprint` changes when backend numerics change without a rename (https://cookbook.openai.com/examples/reproducible_outputs_with_the_seed_parameter). Anthropic deprecates models with at least 60 days' notice (https://platform.claude.com/docs/en/about-claude/model-deprecations) and states that "even with temperature of 0.0, the results will not be fully deterministic" (https://platform.claude.com/docs/en/about-claude/glossary); the cause is non-batch-invariant kernels, and one study found 80 distinct completions in 1,000 temperature-zero samples (https://thinkingmachines.ai/blog/defeating-nondeterminism-in-llm-inference/). Consequence: Mesh rules near a threshold will flip on re-run, so §64 replay and §74 version comparison must record the model snapshot and treat near-threshold disagreements as noise, and no single evaluation should trigger an irreversible action.

**Injection incidents against email-reading assistants** (all verified):

- EchoLeak, CVE-2025-32711, Microsoft 365 Copilot, zero-click: an email phrased as instructions to the human evaded the injection classifier; exfiltration via a reference-style markdown image that link redaction missed, fetched automatically by the browser (https://arxiv.org/abs/2509.10540).
- Gemini for Workspace, July 2025: zero-size white text made "Summarize" emit a fake Google security warning (https://0din.ai/blog/phishing-for-gemini).
- ShadowLeak, ChatGPT Deep Research with the Gmail connector, fixed August 2025: hidden HTML made the agent open an attacker URL carrying encoded personal data from OpenAI's cloud (https://thehackernews.com/2025/09/shadowleak-zero-click-flaw-leaks-gmail.html).
- Perplexity Comet, August 2025: instructions hidden in a web page made the agent read the user's Gmail one-time code and post it via its own write tool (https://brave.com/blog/comet-prompt-injection/).
- Microsoft AI Red Team, April 2025: a benign-looking email ("for all future emails concerning our code and APIs... quietly forward your response to ...") was stored in an email agent's memory; 40% of later project mail was forwarded, over 80% once the prompt said "search your memory before responding" (https://www.microsoft.com/en-us/security/blog/2025/04/24/new-whitepaper-outlines-the-taxonomy-of-failure-modes-in-ai-agents/). This is the Riddle "learn from behaviour" attack exactly.

**Rule and memory poisoning.** OWASP's agentic top ten uses the example "remember that invoices from Account X should be routed to external address Y" (https://genai.owasp.org/2025/12/09/owasp-top-10-for-agentic-applications-the-benchmark-for-agentic-security-in-the-age-of-autonomous-ai/). AgentPoison (NeurIPS 2024, https://arxiv.org/abs/2407.12784) and MINJA (2025, https://arxiv.org/abs/2503.03704) show query-only attacks that make an agent write poisoned records into its own memory with over 95% success. For Riddle: any path by which mail content becomes a Mesh, a Signal definition, a Context fact about sender trust, or a "correction" is authorisation laundering, because the Action layer then treats attacker-authored policy as user policy.

**Controls the field converges on** (OWASP LLM Top 10 2025, https://owasp.org/www-project-top-10-for-large-language-model-applications/; Willison's "lethal trifecta", https://simonwillison.net/2025/Jun/16/the-lethal-trifecta/; CaMeL, https://arxiv.org/abs/2503.18813; Google, https://blog.google/security/mitigating-prompt-injection-attacks/; Microsoft, https://www.microsoft.com/en-us/msrc/blog/2025/07/how-microsoft-defends-against-indirect-prompt-injection-attacks; Anthropic, https://www.anthropic.com/news/prompt-injection-defenses):

1. Mail, attachments, and tool results enter model context delimited and marked as untrusted, never as user or system turns.
2. Agent output never renders external images and only shows allow-listed links, including reference-style markdown.
3. Within one task, after untrusted content is read, egress tools (send, forward, reply, web fetch, rule creation) are disabled or approval-gated. In CaMeL, untrusted data never alters control flow; only a tool-less quarantined model reads it.
4. Every external side effect requires approval that shows the Action layer's real arguments (actual recipients), not the model's description of them.
5. The Action layer enforces deterministic recipient and domain policy independent of the model, such as blocking forwards to never-before-seen external addresses.
6. An injection classifier plus quarantine is defence in depth, not the boundary: Anthropic reports browser-agent attack success falling from 23.6% to about 1% with layered defences, still non-zero.
7. Durable state (Meshes, Signals, trust facts) is created only from explicit user actions; proposals show the inducing evidence and are rejected when that evidence traces to a single sender or to quarantined mail.
8. Expeditions declare tools and mailbox scope up front, with egress logging and rate limits on outbound actions.

### 5.4 Desktop, local-first and Tauri

Sources are linked inline; all checked 2026-09-21.

**Nothing runs while the machine sleeps.** Apple: during sleep "the CPU gets suspended... stops running any code, including app code" and apps cannot opt out (https://developer.apple.com/forums/thread/795461). Windows Modern Standby "pauses all desktop applications" (https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/integrating-apps-with-modern-standby). While awake but backgrounded, macOS App Nap throttles timers and I/O unless the process holds an activity assertion via `NSProcessInfo beginActivityWithOptions:` (https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/AppNap.html). Implication: the morning summary is a catch-up that starts on wake, with latency proportional to the overnight backlog and capped by Gmail's fetch quota. The README should say so rather than imply continuous processing.

**Tauri process lifetime.** Tauri 2 exits when the last window closes; keeping the process alive means handling `RunEvent::ExitRequested` with `prevent_exit`, or hiding on close, both documented as workarounds (https://github.com/tauri-apps/tauri/issues/13511). A tray icon and a macOS accessory activation policy are supported (https://v2.tauri.app/learn/system-tray/); launch at login uses `tauri-plugin-autostart` (https://v2.tauri.app/plugin/autostart/). None of this is in ARCHITECTURE §4 or §7.

**Push needs a server.** Google's guidance for installed apps is to poll `history.list`. Mimestream, the closest shipping analogue (native macOS, Gmail API), built a relay server for push (https://mimestream.com/trust/security-and-privacy). Its founder has said the Gmail API "has definite efficiency issues compared to IMAP" for a thick client and needed a "lengthy review process" (https://twitter.com/neil_jhaveri/status/1355787069155631105).

**IPC and storage.** Tauri events are JSON strings "not designed for low latency or high throughput"; Channels are the ordered, fast path (https://v2.tauri.app/develop/calling-frontend/). The SQL plugin drives sqlx from the webview over IPC; a heavy store should use rusqlite in Rust behind paginated commands. This matches §7's "Tauri is an adapter" principle but the document should say the webview never sees raw SQL and lists are paged.

**HTML rendering.** Tauri's CSP guidance is to avoid remote content (https://v2.tauri.app/security/csp/). The realistic threat is Mailspring's CVE-2023-47479, where mutation XSS in an HTML email reached remote code execution (https://www.sonarsource.com/blog/reply-to-calc-the-attack-chain-to-compromise-mailspring/). The standard defence is a Rust-side sanitiser (ammonia), a sandboxed `srcdoc` iframe with scripts and same-origin disabled, remote loads blocked by the frame's CSP, and `cid:` images served through a custom protocol; note the open Linux bug for custom protocols in iframes (https://github.com/tauri-apps/tauri/issues/12767). Thunderbird blocks remote content by default; Apple Mail proxies or blocks; Mimestream blocks tracking pixels from 60+ services.

**Token storage and encryption.** OS keychain via the `keyring` crate is the recommended home for refresh tokens (https://docs.rs/keyring/latest/keyring/); Mimestream does the same. SQLite has no encryption by default; SQLCipher through rusqlite's `bundled-sqlcipher` feature adds AES-256 at "5 to 15% overhead" with the key held in the keychain, and any loadable extension such as sqlite-vec must be built against it. OS-level disk encryption protects lost devices only, not other processes on a logged-in machine.

**Scale.** The Enron corpus is about 2.7 KB per message as plain text; modern HTML bodies run tens of KB, so 500k messages is roughly 1.5 to 35 GB before attachments. FTS5 handles that; `detail=column` or `detail=none` cut one email index from 743 to 134 MiB (https://sqlite.org/fts5.html). A WAL reader that never closes starves checkpoints. Per-message AI traces at about 5 KB each add about 2.5 GB per 500k messages, which argues for compression, a separate attached database, and age-based retention. sqlite-vec brute-force search is about 75 ms at 100k by 1024 dimensions and fails the 100 ms budget at 1M; its ANN indexes are alpha as of 2026 (https://github.com/asg017/sqlite-vec/releases). Local embeddings with all-MiniLM-L6 on Apple Silicon run in the thousands of texts per second, so embedding a full mailbox is minutes to an hour (https://github.com/jerrythomas/rust-embedding-bench).

**Multi-device.** Options are: a small sync server; a CRDT layer (cr-sqlite is column-level LWW inside SQLite with 2.5x slower inserts and no FTS or virtual tables; Automerge and Loro suit rules and preferences, not 500k rows); or using Gmail labels as the shared medium, which costs quota on every write, is capped at 5,000 labels, and can carry only categorical state (https://github.com/vlcn-io/cr-sqlite, https://www.inkandswitch.com/essay/local-first/). Push, multi-device, CASA scope, key custody, and crash reporting all hinge on whether a server exists, so the decision belongs in the architecture now.

**Crash reporting.** Sentry minidumps capture process memory, including "in-memory representations of input fields" (https://docs.sentry.io/platforms/native/guides/minidumps/). For a mail client that means message bodies leave the device unless minidumps are disabled or scrubbed.

**Auto-update.** Tauri's updater requires a signing key pair; losing the private key strands installed apps (https://v2.tauri.app/plugin/updater/).

### 5.5 Prior art

Sources are linked inline. All product claims were read from vendor help pages, interviews, or reviews on 2026-09-21.

- **Hidden versus visible decides trust.** Microsoft's Clutter silently moved mail to a folder and was retired in 2020 for Focused Inbox, which shows the same split as a visible tab. Superhuman keeps an enumerable "Auto Archived" view. SaneBox sends a daily digest at a chosen time specifically so misfiles are caught. (https://www.howtogeek.com/407202/, https://help.superhuman.com/hc/en-us/articles/40127432866323-Auto-Labels, https://geekflare.com/software/sanebox-review/)
- **Deterministic never-hide rules beat confidence.** Superhuman never auto-archives senders or domains the user has emailed and has explicit Never/Always Archive lists. Copilot in Outlook excludes encrypted, out-of-office, and calendar mail from its prioritiser. (https://new.superhuman.com/auto-archive-exclusions-inclusions-321320, https://practical365.com/prioritize-my-inbox/)
- **Google Priority Inbox ran asymmetric errors on purpose.** Aberdeen, Pacovsky and Slater (2010) defined importance as the probability the user acts within a window, combined a global model with a per-user model, and tuned per-user thresholds so false negatives ran at 3 to 4 times false positives. Users spent 6% less time on mail overall and 13% less on unimportant mail. (https://static.googleusercontent.com/media/research.google.com/en//pubs/archive/36955.pdf)
- **Base rate.** Grbovic et al. (Yahoo, CIKM 2014) measured machine-generated mail at 90% of a 500-billion-message corpus. (https://arxiv.org/html/1606.09296) The widely quoted "87% automated" figure is UNVERIFIED.
- **Automations fight over one mailbox.** SaneBox documents that Superhuman's reminders moving mail back to the inbox retrained SaneBox to treat those senders as important. Any learner that reads every mailbox move as user intent will poison itself with the moves made by Gmail filters, other clients, and its own rules. (https://www.sanebox.com/help/388-superhuman-sanebox-important-things-to-know)
- **Cost is existential for "classify everything".** Shortwave's CEO says a full agent run costs about 100x a summary, that the company would be "bankrupt" without prompt caching, and that the assistant deliberately cannot send, delete, or archive. Fyxer meters by volume. Notion Mail and Superhuman gate AI behind paid tiers. (https://www.aisummer.org/p/andrew-lee-on-running-an-ai-email)
- **Injection is a shipped exploit.** In July 2025 hidden CSS text in a mail body hijacked Gemini's Gmail summaries into phishing instructions. (https://www.bleepingcomputer.com/news/security/google-gemini-flaw-hijacks-email-summaries-for-phishing/)
- **Natural-language rules must be editable in place.** The most repeated Notion Mail complaint is that a label's prompt cannot be edited without recreating it, at roughly 60 to 70% observed accuracy. (https://www.eesel.ai/blog/notion-mail-reviews)
- **First-sender screening scales badly.** HEY's Screener works, but mailing lists create "constant triage", approved senders pollute contacts with no-reply and one-time-code addresses, and HEY needed a bypass code and a 90-day retention rule for screened-out mail. (https://help.hey.com/article/722-the-screener, https://mgmarlow.com/words/2025-04-18-how-i-use-hey/)
- **Privacy incidents define the category.** Superhuman's default-on read receipts (2019), Unroll.me selling receipt data (2017), Spark storing credentials server-side, and the November 2025 Gmail "smart features" training panic all became the product story. Local-first and no-training are selling points worth stating prominently.
- **Filters degrade the user's own judgment.** An IJHCI 2024 study found that belief in a reliable filter reduces users' own vigilance, which raises the cost of every false negative. (https://www.tandfonline.com/doi/full/10.1080/10447318.2024.2429759)
- **Mimestream is the existence proof for Riddle's topology.** It is a native macOS client that talks to the Gmail API directly, keeps mail in local SQLite, stores refresh tokens in the macOS Keychain, has completed Google's restricted-scope verification, and states that it "only stores your data and tokens on your device" with "direct connections to the Gmail API, without going through any intermediary sync service". Four lessons for Riddle. First, even this "no server" client needed a server: Private Push relays "only opaque history IDs" through `push.mimestream.com` because installed apps cannot receive Gmail push. Second, snooze is implemented as a client feature ("Snooze Labs") because Gmail exposes none, confirming that snooze is Riddle-only state. Third, it manages Gmail's server-side filters and exposes Gmail's own Inbox Categories as first-class features; Riddle's docs mention neither, yet both are free deterministic signal and a way to make deterministic Meshes apply on every device. Fourth, its privacy posture is stated in one sentence and backed by concrete claims (trackers from 75+ services blocked, logs kept on device and shared only by explicit opt-in with message content excluded). Mimestream has no AI features at all, which makes it a clean baseline for the "excellent email client" half of Riddle's purpose that the current docs barely describe. The founder has said the Gmail API "has definite efficiency issues compared to IMAP" for a thick client and that verification was a "lengthy review process". (https://mimestream.com/, https://mimestream.com/trust/security-and-privacy, https://twitter.com/neil_jhaveri/status/1355787069155631105)
- **Table stakes** from current client reviews: unified inbox, threading, fast search including attachments, offline, keyboard shortcuts, snooze, undo send, scheduled send, follow-up reminders, calendar invite handling, notification control, templates, signatures, send-as aliases, contacts autocomplete, rich compose with attachments. (https://zapier.com/blog/best-email-app/)

## 6. Recommended edits, in priority order

Each item names the document and the section to change.

1. **ARCHITECTURE: add a "Semantic Judgment Engine" section before §20.** Name Jev, link the vendor docs, define the trait, record the state budget (32k), the pinned model version, the primitives allowed (Choice and Noul only), the documented weaknesses as constraints, and the replacement path. Update invariant 18 to reference the trait. *(Applied 2026-09-22.)*
2. **ARCHITECTURE: add a "Security Threat Model" section and invariants.** Untrusted-content provenance on every tool result, Contamination excluded from Director context, no autonomous external side effect in a task that ingested untrusted content, rule and Mesh proposals never sourced from mail content, deterministic never-hide and never-Find gates, HTML rendering sandbox, remote images blocked by default, token storage in the OS keychain, encrypted local store. Google's policy now requires prompt-injection protection (§5.2), so this is also a compliance item. *(Applied 2026-09-22.)*
3. **ARCHITECTURE §22, §26, §27, §38: calibration and version pinning.** Every Signal version carries a calibration artifact (labelled set, method, model version, date). Meshes reference `signal@version`. Promotion of a Signal version triggers re-simulation of dependent Meshes. Cache keys include engine and model identity and a Context-slice hash for Context-dependent Signals. *(Applied 2026-09-22.)*
4. **ARCHITECTURE §17: define the LLM escalation contract.** The LLM refines Signals and the Mesh re-runs, or the document states explicitly that the escalated tail is outside Mesh policy. Add the terminal "Unresolved" disposition and its UI surface. Update the README morning summary to include the count. *(Applied 2026-09-22.)*
5. **ARCHITECTURE §2 and §41: define the classification model and Find lifecycle.** Disposition enum, Strata as independent labels, Mesh output vocabulary, Find states, auto-resolution rules, aggregation rules, cross-Dig identity resolution. *(Applied 2026-09-22.)*
6. **ARCHITECTURE: add "Provider Projection and Reconciliation" after §14.** Which Riddle states are written to Gmail as labels or archive, the outbox, conflict rules, attribution of provider-side changes (human, filter, other client, Riddle) before they reach the learning layer, and the full-resync path when the history cursor expires. *(Applied 2026-09-22.)*
7. **ARCHITECTURE: add "Execution Model" after §4.** Menu-bar or tray process, launch at login, sleep and wake catch-up, polling cadence (no push without a server), initial sync pacing under Gmail quota (about 300 messages per minute per account), and an explicit statement that nothing is processed while the machine is off. State single-device for v1 and the consequence. Consider compiling the purely deterministic subset of Meshes into Gmail server-side filters (the Gmail `settings.filters` API, which Mimestream already manages in-client) so that subset applies on every device while Riddle is closed, and import the user's existing Gmail filters as seed Meshes so the two rule systems do not fight. *(Applied 2026-09-22.)*
8. **ARCHITECTURE §51: change `create_mesh` and `update_mesh` to SUGGEST by default.** Autonomy per Mesh unlocks after a shadow period with an observed agreement threshold. Align the README's "Riddle updated Mesh" example with §46. *(Applied 2026-09-22.)*
9. **ARCHITECTURE: add "Temporal Sift".** A scheduler that re-evaluates threads and Finds on time and on thread-state change, owning Follow Up, Attend, Pay expiry, snooze, and Respond auto-resolution. Add sent mail to Sift scope and a thread-state object. *(Applied 2026-09-22.)*
10. **README: add "Cold Start" and "What Riddle Never Hides".** Shadow period length, the backfill window, conservative defaults, and the deterministic guardrails (people you have written to, contacts, same organisation, security and financial senders, calendar mail). *(Applied 2026-09-22.)*
11. **README: add a v1 scope section** listing table-stakes client features in and out, and stating the Gmail-only, single-device, desktop-only constraints plainly. *(Applied 2026-09-22.)*
12. **ARCHITECTURE §57: promote per-Dig cloud exclusion from "future" to v1**, driven by the Work Dig case and by Workspace admin controls. Add the Limited Use disclosure, the no-training vendor requirement, and the verification and CASA plan as launch requirements. *(Applied 2026-09-22.)*
13. **ARCHITECTURE §31, §65: fix the evaluation methodology.** Add sampled audits of Spoil and Filed mail for recall, define the Attention Zero targets numerically, and note that corrections alone cannot measure missed Finds. *(Applied 2026-09-22.)*
14. **ARCHITECTURE §54, §93: add a backfill and budget policy** for new Signals over history and for LLM escalation, per Dig, with hard caps. *(Applied 2026-09-22.)*
15. **ARCHITECTURE §36: specify Mesh resolution.** Rule kinds (route, guard), same-scope ordering, Dig-in-multiple-groups rule, and what "refine" means. *(Applied 2026-09-22.)*
16. **README, Activity & Provenance examples: fix actors.** Sift or Mesh, not Director, for automated classification; proposal rather than immediate rewrite after one correction; either specify or remove "matched previous examples". *(Applied 2026-09-22.)*
17. **ARCHITECTURE: add "Jobs and Scheduling" after §54.** Priority classes (interactive, incoming Sift, catch-up, backfill, experiments), per-resource rate limiters for Gmail, Jev, the LLM provider, and the SQLite writer, fairness across Digs, back-pressure, checkpointing, cancellation, dependencies, and pause on sleep or outage. Add a user-facing Task Centre (status indicator, task list with progress and cancel, completion notifications) to the README, and make the morning summary state whether processing is complete. See 2.21. *(Applied 2026-09-22.)*
18. **ARCHITECTURE §68 to §90: mark Developer Mode as phased.** It is the most detailed part of the document and is fine as written, but it should not be read as v1 scope ahead of items 1 to 10. *(Applied 2026-09-22.)*

## 7. Playbook conformance

Audited on 2026-09-22 against `APPLICATION_FEATURES.md` (the local copy of `~/Projects/application_features.md`), the playbook distilled from CORA, Pontifex, Sleipnir, Warpcore, Grey Eminence, vpb, NommoBar and Relevant Reviews. File paths below are relative to those projects under `~/Projects` and point at the implementation to lift.

The short version: Riddle's documents are the strongest of any project on the playbook's audit, undo, provenance and agent-native sections, and they design a Developer Mode nobody has built yet. They say nothing about the operational half of the playbook: distribution, updates, logging, credentials mechanics, settings, OS integration, data lifecycle, testing, and process. Three of the nine Day 0 checklist items are met.

### 7.1 Scorecard

| Playbook section | Riddle | Notes |
|---|---|---|
| 1 Principles | Strong | Nine of eleven present. Missing: "bookkeeping never fails the work" and "honest UI, disabled controls say why". |
| 2 Architecture shape | Partial | Core-and-adapter and the Action layer are better specified than in any shipped app. Missing: instrumented invoke, typed errors, CLI twin, MCP, per-tenant store, generated bindings. |
| 3 Distribution and updates | Missing | No CI, signing, cask, updater, version discipline, dev/prod split, changelog. |
| 4 Developer mode | Strong on AI, missing the basics | Twenty-four sections of inspectors, but no runtime, storage, log viewer, in-flight calls or copy-diagnostics pane, and no out-of-app probes. |
| 5 Logging | Missing | One sentence (Activity is not logging). No framework, categories, redaction, rotation, retention. |
| 6 Audit, safety, undo | Strong | Provenance, undo, risk metadata, simulation. Missing: concrete confirmation tiers, backend re-check, soft delete for policy objects, persistence gate. |
| 7 Agent-native and AI | Strong | Missing: model catalog source, prompt caching, tool budgets, circuit breaker, usage-ledger rules, MCP exposure. |
| 8 Security | Partial | The principle is stated (§57). No keyring configuration, capabilities, CSP, file modes, signing for TCC, HTML sandbox. |
| 9 Background and OS integration | Partial | Sync and jobs are named. No poll-loop design, sleep and wake, tray, notifications, login item, health badge (see 2.21). |
| 10 Settings and onboarding | Missing | No settings model, ownership split, first run, tours, help. |
| 11 UX conventions | Missing | The README describes the experience; nothing on theming, shortcuts, errors, empty states, status. |
| 12 Data and migrations | Partial | SQLite chosen. No per-Dig store, migrations, backups, disk watermarks, export. |
| 13 Testing | Partial | Signal and Mesh evaluation is well designed. No software test strategy, fixtures, CI, or test-host guard. |
| 14 Docs and process | Partial | Two excellent documents. No status line, roadmap with "not planned", CONTRIBUTING, CLAUDE.md, troubleshooting. |
| 15 Stack defaults | Partial | The crate list names domains, not dependencies. |
| 16 Day 0 checklist | 3 of 9 | Core-and-adapter split, action allowlist, developer toggle (partially). |
| 17 Recurring regrets | 10 apply | See 7.4. |

### 7.2 Patterns to adopt

**A. Protect the developer from the real mailbox.** This is the group Riddle needs more than any other project, because a bug in a dev build can archive ten thousand real messages.

- Dev and prod split keyed on the build: separate data directory, keychain service suffix, and log file, plus a gold DEV badge because "magenta is reserved for production danger" (Sleipnir `src-tauri/src/paths.rs:18-32`, `src/App.css:250-263`). Riddle should add one rule on top: dev builds start in shadow mode (invariant 21) and must be explicitly armed before any provider mutation, ideally against a separate OAuth client and a test account.
- Sign dev builds with a stable Developer ID so the keychain ACL on OAuth refresh tokens survives rebuilds; otherwise every rebuild re-prompts or loses the token (Pontifex `scripts/cargo-signed.sh`; Grey Eminence `project.yml:48-52`). Enable `keyring`'s `apple-native` and `windows-native` features; the default is an in-memory mock "which cost us every refresh token" (Sleipnir `Cargo.toml:28-31`).
- A test-host guard so tests can never open the real store or call Gmail (Grey Eminence `App/TestEnvironment.swift`), with an in-memory store for unit tests (CORA `Store::open_in_memory`).
- A persistence gate: every store write is logged with its call site, and consecutive failures flip a fault flag that pauses Sift and shows in the status bar rather than silently dropping classifications (Grey Eminence `Services/Persistence/PersistenceGate.swift`).
- A panic inside a command leaves the IPC call pending forever (Pontifex `src-tauri/tests/infer_hang.rs`). Actions catch at the boundary and the TypeScript side times out.

**B. Architecture mechanics.**

- One instrumented invoke wrapper that logs names and timings, keeps an in-flight registry, and never logs argument values (Pontifex `src/lib/ipc.ts:112-150`). For Riddle the rule becomes: log message IDs, never subjects, bodies or addresses.
- Typed errors `{kind, message}` so the UI can offer the right affordance (Pontifex `src-tauri/src/error.rs`). Riddle needs an error taxonomy up front: `provider_auth`, `provider_admin_blocked`, `provider_rate_limited`, `provider_history_expired`, `judge_unavailable`, `llm_unavailable`, `budget_exhausted`, `policy_denied`, `needs_confirmation`. Each maps to a specific UI response; several map to "park the job and wake on sign-in".
- One SQLite database per Dig, as CORA keeps one per GitHub org with the rule "Data never crosses org boundaries" (`src-tauri/src/orgs.rs`). For Riddle this makes the per-Dig privacy policy (§57), Dig removal (purge one file), per-Dig backup, and per-Dig cloud exclusion trivial. Cross-Dig views attach databases or use a small index store. Keep the pre-split database as a permanent `legacy-backup` before any restructuring (`orgs.rs:152-162`).
- Generate TypeScript types from Rust with ts-rs (CORA `npm run bindings`) so Find types, dispositions and Mesh outputs cannot drift between the two sides.
- A CLI twin sharing the core: replay, simulate, export traces, run an Expedition headless, with NDJSON `--progress` that emits the plan before the first side effect and `--yes` implied by `--progress` because "a program driving the CLI confirms on its own side" (Warpcore `src/warpcore/cli/run_progress.py`, `cli/main.py:493`; Sleipnir `src-tauri/src/cli.rs`).
- An MCP server exposing Actions read-only first, write Actions gated by the same risk metadata (Warpcore `src/warpcore/mcp_server.py`). Riddle's Director is internal, but an external agent driving Riddle through MCP is the natural extension of "agent-native".
- Named progress events per subsystem (`sift://progress`, `sync://progress`, `expedition://progress`), delivered over Channels rather than events for volume (Pontifex event naming; review §5.4).

**C. Distribution, updates and versioning.**

- Tag-driven CI with a tag-equals-version check, tests, sign and notarize, a hard gate that refuses unnotarized output, an idempotent release, and a Homebrew cask bump (Sleipnir `.github/workflows/release.yml`). A stable-name asset uploaded with `--clobber` (Relevant Reviews `.github/workflows/release.yml`).
- `tauri-plugin-updater` with a silent check 5 s after launch and every 6 h, and a banner state machine from checking to Relaunch (Relevant Reviews `app/src/components/UpdateBanner.tsx`, `App.tsx:116`). Write down where the signing key lives.
- A version-bump hook that denies mismatched tags and unbumped commits (Grey Eminence `.claude/hooks/check-version-bump.sh`), and a build stamp injected at build time (Warpcore `visual-builder/src-tauri/build.rs`). Riddle should record app version and build sha in every decision trace, because §64 replay comparisons need to know which build produced each result.
- An upgrade-time simulation gate. Grey Eminence ships a What's New sheet with "Try it" links (`Features/WhatsNew/`). Riddle needs the stronger form: an update that ships new system Signal versions or changed default Meshes runs the §38 simulation on first launch and shows "this update would reclassify N messages: Apply, or keep previous behaviour" before anything changes. A silent behaviour change in an update is the one thing an email automation must never do.
- A bundled CHANGELOG rendered in-app (Grey Eminence `Features/Help/ChangelogView.swift`).

**D. Logging.**

- `tauri-plugin-log` to the log directory and stdout, a size cap, `[rfc3339][LEVEL][category]`, noisy crates pinned to warn (Pontifex `src-tauri/src/lib.rs:143-183`). Fixed categories with macros and a `Timed` guard that warns above a threshold (`src-tauri/src/logging.rs`). Suggested vocabulary: sync, sift, judge, llm, mesh, director, expedition, store, jobs, ui.
- Structured payloads appended as ` ||PAYLOAD||<json>` and expanded in the viewer (Sleipnir `src-tauri/src/applog.rs`, `src/components/DeveloperTab.tsx`).
- Webview errors forwarded to the backend log through a `ui_log` command and per-route error boundaries (Pontifex `src/app/error-boundary.tsx`).
- The redaction rule as a hard requirement: logs carry identifiers only. Developer Mode resolves identifiers from the store when displaying. Document what logs may still contain (Sleipnir `docs-src/troubleshooting.md:174-177`).
- Rotation from day one; Grey Eminence added it after `system.log` reached 735 MB (`Services/Logging/LogManager.swift:55-62`).

**E. Safety tiers mapped to autonomy levels.** Riddle's §51 defines `confirmation_policy` abstractly. The playbook supplies the concrete tiers:

- Confirm in place, never a modal, for reversible bulk actions, with Undo All (Sleipnir `src/components/ProjectPanel.tsx:30`).
- Two-step confirm-danger for send and reply, showing the Action layer's real recipients rather than the model's description (CORA `src/windows/MainApp.tsx:751-775`; review §5.3).
- Type the exact count or name for permanent delete and for emptying Contamination (Pontifex `src-tauri/src/commands/schemas.rs:515-540`).
- Press and hold with coaching text for the one combination that changes everything: disabling all Meshes, or raising a Dig to TRUSTED autonomy (Sleipnir `src/components/HoldButton.tsx`).
- All of it re-validated in Rust, not only in the dialog (Pontifex `confirm_name`, `confirm_protected`).
- Soft delete with Recently Deleted and a restore that refuses a name clash for Meshes, Signals and Strata; purge is the only lossy path (Sleipnir `src-tauri/src/config/schema.rs:92-131`).
- Label the irreversible where it happens, as CORA writes "Merges are permanent" (`HistoryDrawer.tsx:9`): "Sent mail cannot be unsent".
- Provenance on anything Riddle writes into Gmail: a managed-by marker on filters and label descriptions, as Pontifex stamps exported profiles (`src-tauri/src/aws/config_writer.rs:11-29`), so the user can tell Riddle's filters from their own.

**F. AI operations.**

- Model catalog from a shared settings file, never hard-coded, with a rule for the fast model (Pontifex `src-tauri/src/commands/bedrock.rs:409-420`).
- Model tiers with the evidence recorded in code, as CORA does for effort levels ("on four PRs run at both levels (2026-09-06) the findings held while the run took 30-50% less time", `src-tauri/src/models.rs:421-437`). §56's selection criteria should carry the same evidence.
- Prompt caching with a rolling checkpoint and a cache-free retry (CORA `src-tauri/src/analysis/engine.rs:445-470`) for the Director and Expeditions.
- Bounded tools: character caps, batch limits, a shared rate gate, and an exploration budget after which "the research tools are withdrawn" (CORA `engine.rs:22-27`). This is the concrete form of the Expedition budgets in §3 item 14.
- A per-provider circuit breaker in application state with a probe slot that cannot wedge (CORA `src-tauri/src/health.rs`), for Gmail, Jev and the LLM provider separately.
- Usage ledger rules: "anything we can't price is reported as unpriced rather than quietly counted as free", and a usage row "is bookkeeping, never a reason to fail the work" (CORA `src-tauri/src/usage.rs:1-9, 41`); task-local attribution so every token is charged to Sift, Director, Expedition or Simulation (Grey Eminence `Services/AI/AIUsage.swift`). The §84 Cost Observatory needs both.
- Separate principals for the model provider and the mailbox (Pontifex `src-tauri/src/state.rs:81-94`).
- The Director's system prompt as a versioned, runtime-editable file with placeholders (Grey Eminence `Services/AI/PromptStore.swift`). Signals are already versioned; the Director prompt is not.

**G. Background work and OS integration.**

- Poll loop as `tokio::select!` on a sleep versus a `Notify` trigger, exponential backoff, slowdown under rate limit (CORA `src-tauri/src/github/poller.rs:27-115`); park on auth failure and wake through credential-change subscribers (Pontifex `src-tauri/src/aws/clients.rs:49-55`). For Riddle: the Gmail history poller parks on `admin_policy_enforced` or token expiry and wakes on re-authentication.
- Sleep detection from a wall-clock gap on a 30 s tick, and a refresh pass on `visibilitychange` and `focus`, because "the click IS the reconnect" (Sleipnir `src-tauri/src/lib.rs:59-103`, `src/lib/useAppState.ts`).
- A daily budget with in-flight dedupe (CORA `src-tauri/src/store.rs:572`) for Jev and LLM spend per Dig.
- Tray, close-to-hide while armed, Dock badge with the Find count, and off-screen window rescue after monitor changes (Pontifex `src-tauri/src/watch/runtime.rs:292`; CORA `src-tauri/src/lib.rs:44-97`).
- Notifications through a Swift `UNUserNotificationCenter` helper, because on macOS 26 the plugin's API "reports success and shows nothing" (Pontifex `src-tauri/notifier/main.swift`, `src-tauri/src/watch/notify.rs`); a click claim via the next activation (CORA `src-tauri/src/notify.rs:7-18`); one reserved interruptive alert, everything else waits in the panel (vpb `macos/VpbApp/CriticalAlert.swift`), which for Riddle means only Finds above a priority threshold interrupt. A Test Notification button in settings.
- Launch at login through `SMAppService` with `.requiresApproval` and a read-back so the toggle "reports what the system now says rather than what was asked for" (vpb `macos/VpbApp/SettingsModel.swift:88-118`).
- A health badge that separates intent from delivery, ENGAGED versus STALE (Sleipnir `src-tauri/src/commands.rs:452-527`). Per Dig: Synced, Sifting N remaining, Projection pending N, Degraded.

**H. Settings, onboarding, help and UX.**

- Settings split by ownership: declarative, shareable config (Digs, groups, per-Dig privacy and autonomy, Mesh references) versus runtime state (cursors, jobs) versus high-churn data (Activity) in separate files, so a form save cannot clobber live state (Sleipnir `src-tauri/src/state.rs:1-9`; Pontifex `src-tauri/src/watch/store.rs:1-12`). Backend-owned, atomic writes, `#[serde(default)]` on every field, and a test that a file written before a feature still loads (Pontifex `src-tauri/src/settings.rs:503-518, 645-659`).
- Meshes and Signals exportable as files. Sleipnir's `config.toml` is "shareable" and hand-editable. For Riddle this gives version control of policy and a cheap way to carry Meshes between machines, which solves the policy half of the multi-device problem without a server.
- First-run bootstrap from what already exists: Gmail filters, labels and categories, with ranked guesses and the rule that "a bad guess is not a neutral default" (Pontifex `src-tauri/src/lib.rs:38-80`).
- Data-driven tours anchored to `data-tour` attributes with non-fatal missing anchors (Sleipnir `src/tour/`), bundled docs in a Help window (Grey Eminence `Features/Help/HelpDocsView.swift`), settings cross-links as navigation objects (`Features/Settings/SettingsNavigation.swift`), and a shortcut table that doubles as the help sheet "so a shortcut can't exist without being documented" (CORA `src/windows/MainApp.tsx:2128`). Riddle's triage loop should be keyboard-first.
- Token-only theming with a test that fails on a missing token (Pontifex `src/theme/themes.test.ts`); inline errors with a sign-in affordance, no toasts; disabled actions carry a clickable reason (Warpcore `visual-builder/src/utils/deployRunner.ts:14`), for example "Send disabled: the Director read untrusted content in this task".
- The empty state is the product. The Attention Zero screen must show its evidence: last sync time, messages still being sifted, what went to Spoil, and that Undo is available. "NOTHING ENGAGED" and "RUN THE ORG SCAN FIRST" (Sleipnir) are the model: an empty state is an instruction or a proof, never a blank.
- Modals only for tasks with progress (Warpcore `DESIGN_GUIDANCE.md §7`).

**I. Data lifecycle.**

- Versioned schema migrations with the previous database kept as a permanent backup before any structural change (Grey Eminence `Services/Persistence/SchemaVersions.swift`; CORA `orgs.rs:152-162`).
- Daily rolling backups with retention and a reset path (Grey Eminence `Services/Persistence/StoreBackupService.swift`). For Riddle, back up Riddle state (Meshes, Signals, Finds, Activity, traces), not the mail cache, which is re-syncable.
- Disk-space watermarks for the mail cache (Grey Eminence `DiskSpace`, 10 GB warning), a TTL cache for expensive lookups (Pontifex `src-tauri/src/ttl_cache.rs`), and a throttled idempotent maintenance job (Grey Eminence `MaintenanceService`).

**J. Testing and process.**

- Live `#[ignore]` tests against Gmail and Jev whose header states the claim they prove (Pontifex `src-tauri/tests/*.rs`); parity tests between UI lists and Rust enums for Find types and Mesh outputs (Pontifex `tests/format_picker_parity.rs`); a structure-preserving synthetic mailbox fixture derived from real mail (Sleipnir `src-tauri/src/discovery_fixture.txt`); vitest for pure reducers; PR CI, not only tag CI.
- A README status line, a roadmap with an explicit "Not planned" list (Pontifex `docs/watch-roadmap.md`), CONTRIBUTING house rules (Grey Eminence `Resources/Docs/CONTRIBUTING.md`), a CLAUDE.md, module `//!` headers as ADRs (CORA `src-tauri/src/health.rs`), and a troubleshooting section that names the log location.

### 7.3 Combinations only Riddle needs

These come from crossing a playbook pattern with something already in Riddle's design.

1. **Upgrade simulation gate**: What's New crossed with Mesh simulation. Updates that change system Signals or default Meshes show their historical impact and ask before applying.
2. **Dev shadow mode**: the dev/prod split crossed with invariant 21. Dev builds observe and record but never mutate the provider until armed.
3. **Per-Dig store crossed with per-Dig privacy**: cloud exclusion, removal and backup become file operations.
4. **Health badge crossed with provider projection**: the per-Dig status shows whether Riddle's decisions have reached Gmail yet.
5. **Blocked reason crossed with the trifecta rule**: when the Director has read untrusted content, egress actions are disabled with the reason shown, not silently missing.
6. **Empty state crossed with Attention Zero**: the promise is only credible if the empty state shows the evidence behind it.

### 7.4 Regrets that apply directly

From the playbook's §17, the ones Riddle would repeat without a decision now: `keyring` left on its in-memory mock; unsigned dev builds prompting for the keychain on every rebuild; `csp: null`; logs without rotation and without redaction, which for a mail client means message content on disk in plain text; the version duplicated across `tauri.conf.json`, `package.json` and `Cargo.toml`; fake progress during catch-up; the notification API that shows nothing on macOS 26; tests that run only on tag pushes; a panic in a command hanging the UI; and two automations fighting over one mailbox, here Gmail filters against Meshes.

### 7.5 Additional recommended edits

These extend §6. Items P1 to P4 belong in the first architecture revision; the rest can follow.

- **P1. ARCHITECTURE: add "Developer Safety".** Dev/prod data split, dev shadow mode, signed dev builds, `keyring` native features, test-host guard, persistence gate, command-boundary panic handling (7.2 A). *(Applied 2026-09-22.)*
- **P2. ARCHITECTURE: add "Logging".** Plugin configuration, category vocabulary, `Timed` guard, structured payloads, webview forwarding, the identifiers-only redaction rule, rotation and retention (7.2 D). *(Applied 2026-09-22.)*
- **P3. ARCHITECTURE §8: add the error taxonomy and the instrumented invoke wrapper** (7.2 B). *(Applied 2026-09-22.)*
- **P4. ARCHITECTURE §11: one SQLite store per Dig**, with a legacy backup before restructuring and versioned migrations (7.2 B, I). *(Applied 2026-09-22.)*
- **P5. ARCHITECTURE: add "Distribution and Updates"** with the upgrade simulation gate, version hook, build stamp recorded in traces, updater and key custody (7.2 C). *(Applied 2026-09-22.)*
- **P6. ARCHITECTURE §51: replace the abstract `confirmation_policy` with the concrete tiers**, backend re-validation, soft delete for policy objects, and managed-by markers on anything written to Gmail (7.2 E). *(Applied 2026-09-22.)*
- **P7. ARCHITECTURE §56, §84: AI operations.** Model catalog source, tiers with recorded evidence, prompt caching, tool budgets, per-provider circuit breaker, usage-ledger rules, separate principals, versioned Director prompt (7.2 F). *(Applied 2026-09-22.)*
- **P8. ARCHITECTURE §53, §54: OS integration.** Poll loop with park and wake, sleep detection, daily budgets, tray and Dock badge, notifier helper, login item with read-back, per-Dig health badge (7.2 G). *(Applied 2026-09-22.)*
- **P9. ARCHITECTURE: add "Settings and Configuration"** with the ownership split, atomic writes and compatibility tests, exportable Meshes and Signals, and first-run bootstrap from Gmail filters and labels (7.2 H). *(Applied 2026-09-22.)*
- **P10. README: add "Keyboard, Help and Empty States".** Keyboard-first triage with the shortcut table as help, tours, bundled docs, the evidence-bearing Attention Zero screen, blocked reasons (7.2 H). *(Applied 2026-09-22.)*
- **P11. ARCHITECTURE: add "Data Lifecycle".** State backups with retention, disk watermarks for the cache, TTL caches, maintenance job (7.2 I). *(Applied 2026-09-22.)*
- **P12. ARCHITECTURE: add "Testing and CI".** In-memory store tests, live tests with stated claims, parity tests, synthetic mailbox fixture, PR CI (7.2 J). *(Applied 2026-09-22.)*
- **P13. ARCHITECTURE §67: put the operations pane first in Developer Mode.** Runtime, storage with reveal and clear, log viewer, in-flight calls, copy diagnostics, and out-of-app probes for Gmail and Jev, ahead of the AI inspectors. *(Applied 2026-09-22.)*
- **P14. ARCHITECTURE §4: add a CLI twin and an MCP server** sharing the Action layer, with NDJSON progress (7.2 B). *(Applied 2026-09-22.)*
- **P15. ARCHITECTURE §95: add two invariants.** "Bookkeeping, logging and telemetry never fail the work they describe" and "Progress and status are derived from real events; a disabled control states its reason." *(Applied 2026-09-22.)*
- **P16. Repository: README status line, roadmap with "Not planned", CONTRIBUTING, CLAUDE.md, troubleshooting** (7.2 J). *(Applied 2026-09-22.)*
