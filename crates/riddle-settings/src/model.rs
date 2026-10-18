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
    fn default() -> Self {
        Self {
            provider: JudgeProvider::Jev,
            model: "jev-1.13.0".into(),
            key_source: KeySource::Ssm,
            ssm_profile: Some("dev".into()),
            ssm_parameter: Some("/riddle/jev_api_key".into()),
            api_key_present: false,
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
    fn default() -> Self {
        Self {
            provider: LlmProvider::Bedrock,
            profile: "dev".into(),
            region: "us-east-1".into(),
            escalation_model: "us.anthropic.claude-haiku-4-5-20251001-v1:0".into(),
            director_model: "us.anthropic.claude-sonnet-4-5-20250929-v1:0".into(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GmailSettings {
    pub client_id: String,
    /// Derived at read time from the keychain; never written to disk.
    pub client_secret_present: bool,
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

/// The roster as of October 2026: the newest of each family the dev
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
            launch_at_login: false,
            close_to_hide: true,
            comparison: ComparisonSettings::default(),
        }
    }

    /// Dev builds start in OBSERVE (§59), log at debug and show Everything.
    /// Developer is not the default even here: the level is the person's
    /// choice, and a file that already says `developerMode = true` loads as
    /// Developer (`parse_settings`), so nobody loses the pages they had.
    pub fn dev_defaults() -> Self {
        Self {
            developer_mode: false,
            ui_level: UiLevel::Everything,
            log_level: LogLevel::Debug,
            autonomy: AutonomySettings {
                default: Autonomy::Observe,
                ..Default::default()
            },
            ..Self::prod_defaults()
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
        }
        // The schema version leads the file; the rest is sorted by the TOML writer.
        if let Some(table) = value.as_table_mut() {
            table.remove("schemaVersion");
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
        assert_eq!(s.judge.key_source, KeySource::Ssm);
        assert_eq!(s.judge.ssm_profile.as_deref(), Some("dev"));
        assert_eq!(
            s.judge.ssm_parameter.as_deref(),
            Some("/riddle/jev_api_key")
        );
        assert_eq!(s.llm.profile, "dev");
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
        let text = s.to_toml_string().unwrap();
        assert!(!text.contains("apiKeyPresent"));
        assert!(!text.contains("clientSecretPresent"));
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
