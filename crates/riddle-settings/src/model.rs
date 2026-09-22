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
use riddle_core::{Autonomy, DigId, PrivacyMode, ProjectionMode};
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

/// The config file (`settings.toml`). See the module header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    pub schema_version: u32,
    pub developer_mode: bool,
    pub log_level: LogLevel,
    /// Finds at or above this priority (0–100) interrupt; the rest wait.
    pub notify_priority_threshold: u32,
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
            log_level: LogLevel::Info,
            notify_priority_threshold: 70,
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
        }
    }

    /// Dev builds start in OBSERVE (§59) and log at debug.
    pub fn dev_defaults() -> Self {
        Self {
            developer_mode: true,
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

    /// The Action layer's policy (§51). `dev` and `armed` are runtime facts
    /// the integrator sets: `dev` from the build profile, `armed` preserved
    /// from `Runtime::is_armed()` across settings reloads.
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
            armed: false,
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
        assert!(s.developer_mode);
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
