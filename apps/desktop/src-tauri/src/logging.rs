//! tauri-plugin-log configured per ARCHITECTURE.md §48: log directory and
//! stdout in every build, rotation from day one, `[rfc3339][LEVEL][category]`
//! form, noisy crates pinned to warn.
//!
//! The dispatch accepts every level and `log::set_max_level` does the
//! filtering, so the log level saved in Settings can change while the app
//! runs (`system`). `RIDDLE_LOG` pins the level for the whole process.

use crate::paths::{AppDirs, Profile};
use tauri_plugin_log::{Builder, RotationStrategy, Target, TargetKind, TimezoneStrategy};

const MAX_FILE_BYTES: u128 = 10 * 1024 * 1024;

pub fn plugin<R: tauri::Runtime>(
    dirs: &AppDirs,
    profile: Profile,
) -> tauri::plugin::TauriPlugin<R> {
    let level = env_level().unwrap_or(log::LevelFilter::Trace);
    let file_name = match profile {
        Profile::Dev => "riddle-dev",
        Profile::Prod => "riddle",
    };
    Builder::new()
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::Folder {
                path: dirs.logs.clone(),
                file_name: Some(file_name.into()),
            }),
        ])
        .level(level)
        .level_for("hyper", log::LevelFilter::Warn)
        .level_for("hyper_util", log::LevelFilter::Warn)
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("rustls", log::LevelFilter::Warn)
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .level_for("aws_config", log::LevelFilter::Warn)
        .level_for("aws_smithy_runtime", log::LevelFilter::Warn)
        .level_for("aws_runtime", log::LevelFilter::Warn)
        .level_for("tracing", log::LevelFilter::Warn)
        .level_for("h2", log::LevelFilter::Warn)
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(RotationStrategy::KeepAll)
        .timezone_strategy(TimezoneStrategy::UseUtc)
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}][{}] {}",
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                record.level(),
                record.target(),
                message
            ))
        })
        .build()
}

fn env_level() -> Option<log::LevelFilter> {
    std::env::var("RIDDLE_LOG")
        .ok()
        .and_then(|l| l.parse::<log::LevelFilter>().ok())
}

/// The level until settings load: `RIDDLE_LOG`, else the profile default.
pub fn initial_level(profile: Profile) -> log::LevelFilter {
    env_level().unwrap_or(if profile.is_dev() {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    })
}
