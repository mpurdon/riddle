//! Dev and prod are split at build time: separate data directory, keychain
//! service suffix and log file, so a dev build can never touch the real
//! store by accident (ARCHITECTURE.md §59).

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Prod,
}

impl Profile {
    pub fn current() -> Self {
        if std::env::var("RIDDLE_PROFILE").as_deref() == Ok("prod") {
            return Profile::Prod;
        }
        if cfg!(debug_assertions) {
            Profile::Dev
        } else {
            Profile::Prod
        }
    }
    pub fn is_dev(self) -> bool {
        self == Profile::Dev
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Dev => "dev",
            Profile::Prod => "prod",
        }
    }
    /// The keychain service name; the suffix keeps dev tokens apart.
    pub fn keychain_service(self) -> String {
        match self {
            Profile::Dev => "dev.codenaked.riddle.dev".into(),
            Profile::Prod => "dev.codenaked.riddle".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppDirs {
    pub data: PathBuf,
    pub logs: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
}

impl AppDirs {
    pub fn resolve(profile: Profile) -> Self {
        if let Ok(root) = std::env::var("RIDDLE_DATA_DIR") {
            let root = PathBuf::from(root);
            return Self {
                data: root.join("data"),
                logs: root.join("logs"),
                config: root.join("config"),
                cache: root.join("cache"),
            };
        }
        let id = match profile {
            Profile::Dev => "dev.codenaked.riddle.dev",
            Profile::Prod => "dev.codenaked.riddle",
        };
        let support = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(id);
        let logs = dirs::home_dir()
            .map(|h| h.join("Library/Logs").join(id))
            .unwrap_or_else(|| support.join("logs"));
        let cache = dirs::cache_dir()
            .unwrap_or_else(|| support.clone())
            .join(id);
        Self {
            data: support.join("data"),
            logs,
            config: support.join("config"),
            cache,
        }
    }
}
