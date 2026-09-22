//! Compatibility: settings files written before a feature existed must load
//! with defaults for what they lack (ARCHITECTURE.md §12), a save must round
//! trip, and a crash mid-write must leave the previous file intact.

use riddle_core::{Autonomy, RecordingSink};
use riddle_settings::{atomic, LogLevel, NoSecrets, Settings, SettingsStore};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn open(dir: &Path, dev: bool) -> SettingsStore {
    SettingsStore::open(
        &dir.join("config"),
        &dir.join("data"),
        dev,
        Arc::new(NoSecrets),
        Arc::new(RecordingSink::default()),
    )
    .unwrap()
}

fn open_with_fixture(name: &str, dev: bool) -> (tempfile::TempDir, SettingsStore) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("config")).unwrap();
    std::fs::copy(fixture(name), dir.path().join("config/settings.toml")).unwrap();
    let store = open(dir.path(), dev);
    (dir, store)
}

#[test]
fn a_minimal_v1_file_loads_with_defaults_for_every_later_field() {
    let (_dir, store) = open_with_fixture("settings_v1_minimal.toml", false);
    let s = store.settings();
    assert_eq!(s.log_level, LogLevel::Warn);
    assert_eq!(s.shadow_period_days, 10);
    assert_eq!(s.judge.model, "jev-1.12.0", "the file's value wins");
    assert_eq!(
        s.judge.ssm_parameter.as_deref(),
        Some("/riddle/jev_api_key"),
        "a missing nested key takes its default"
    );
    assert_eq!(s.llm.director_model, "us.anthropic.claude-sonnet-4-5-20250929-v1:0");
    assert_eq!(s.budgets.daily_llm_usd, 5.0);
    assert_eq!(s.autonomy.default, Autonomy::Suggest);
    assert!(s.close_to_hide);
    assert!(!s.judge.api_key_present);
}

#[test]
fn the_same_old_file_takes_dev_defaults_in_a_dev_build() {
    let (_dir, store) = open_with_fixture("settings_v1_minimal.toml", true);
    let s = store.settings();
    assert_eq!(s.autonomy.default, Autonomy::Observe);
    assert_eq!(
        s.log_level,
        LogLevel::Warn,
        "the file's explicit level still wins"
    );
}

#[test]
fn a_partial_autonomy_table_and_an_unknown_key_load() {
    let (_dir, store) = open_with_fixture("settings_v1_partial_autonomy.toml", false);
    let s = store.settings();
    assert_eq!(s.autonomy.default, Autonomy::Act);
    assert_eq!(s.autonomy.per_dig["dig-work"], Autonomy::Observe);
    assert!(s.autonomy.per_action.is_empty());
    assert_eq!(s.budgets.daily_judge_usd, 0.5);
    assert_eq!(s.budgets.daily_llm_usd, 5.0);
    assert!(!s.close_to_hide);
}

#[test]
fn a_save_round_trips_every_field_and_stamps_the_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path(), false);
    let mut s = Settings::prod_defaults();
    s.schema_version = 0;
    s.developer_mode = true;
    s.log_level = LogLevel::Trace;
    s.notify_priority_threshold = 42;
    s.judge.ssm_profile = None;
    s.llm.region = "eu-west-1".into();
    s.gmail.client_id = "client-id".into();
    s.autonomy
        .per_action
        .insert("archive_messages".into(), Autonomy::Trusted);
    s.projection_default = riddle_core::ProjectionMode::LabelsOnly;
    s.privacy_default = riddle_core::PrivacyMode::LocalOnly;
    s.launch_at_login = true;
    let saved = store.save(s.clone()).unwrap();
    assert_eq!(
        saved.schema_version,
        riddle_settings::SETTINGS_SCHEMA_VERSION
    );
    let text = std::fs::read_to_string(store.config_path()).unwrap();
    assert!(text.lines().any(|l| l == "schemaVersion = 1"), "{text}");
    assert!(text.starts_with("# Riddle settings"), "{text}");
    let reloaded = open(dir.path(), false).settings();
    assert_eq!(reloaded, saved);
    assert_eq!(reloaded.judge.ssm_profile, None);
    assert_eq!(
        reloaded.autonomy.per_action["archive_messages"],
        Autonomy::Trusted
    );
}

#[test]
fn a_crash_between_temp_write_and_rename_leaves_the_old_file_intact() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path(), false);
    store.update(|s| s.shadow_period_days = 3).unwrap();
    let before = std::fs::read_to_string(store.config_path()).unwrap();

    // Simulate the crash: stage one only.
    let tmp = atomic::write_temp(store.config_path(), b"shadowPeriodDays = 99\n").unwrap();
    assert!(tmp.exists());
    assert_eq!(
        std::fs::read_to_string(store.config_path()).unwrap(),
        before
    );
    let reloaded = open(dir.path(), false);
    assert_eq!(reloaded.settings().shadow_period_days, 3);

    // Completing the write replaces the file whole.
    atomic::commit(&tmp, store.config_path()).unwrap();
    assert_eq!(open(dir.path(), false).settings().shadow_period_days, 99);
}

#[test]
fn a_corrupt_settings_file_is_reported_with_its_path_not_silently_reset() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("config")).unwrap();
    std::fs::write(
        dir.path().join("config/settings.toml"),
        "this is = = not toml",
    )
    .unwrap();
    let err = SettingsStore::open(
        &dir.path().join("config"),
        &dir.path().join("data"),
        false,
        Arc::new(NoSecrets),
        Arc::new(RecordingSink::default()),
    )
    .unwrap_err();
    assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
    assert!(err.message.contains("settings.toml"));
}
