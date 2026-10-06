//! Wiring. `App::open` builds stores, services and the runtime; the domain
//! crates are attached in `attach_domains` (filled in during integration).

use crate::{events::Broadcast, App, AppConfig};
use riddle_actions::{RuntimeBuilder, Services};
use riddle_core::EventSink;
use riddle_errors::Result;
use riddle_store::Stores;
use std::sync::Arc;

impl App {
    pub fn open(config: AppConfig) -> Result<Arc<App>> {
        let stores = Arc::new(match &config.data_dir {
            Some(dir) => Stores::open(dir)?,
            None => Stores::in_memory()?,
        });
        let events = Arc::new(Broadcast::new());
        let sink: Arc<dyn EventSink> = events.clone();
        let services = Arc::new(Services::new(stores.clone(), sink, config.build.clone()));
        let mut builder = RuntimeBuilder::new(services.clone());
        builder = builder.registry(riddle_actions::actions::register_all);
        let mut policy = riddle_actions::policy::AutonomyPolicy {
            dev: config.dev,
            ..Default::default()
        };
        policy.default = if config.dev {
            riddle_core::Autonomy::Observe
        } else {
            riddle_core::Autonomy::Suggest
        };
        let runtime = builder.policy(policy).build();
        let app = Arc::new(App {
            config,
            started_at: std::time::Instant::now(),
            stores,
            services,
            runtime,
            events,
        });
        Ok(app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::*;

    #[test]
    fn an_empty_app_reports_an_honest_summary() {
        let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
            "0", None, None, None, "test",
        )))
        .unwrap();
        let s = app
            .summary(chrono::Utc::now() - chrono::Duration::days(1), None)
            .unwrap();
        assert_eq!(s.dig_count, 0);
        assert!(s.caught_up);
        assert_eq!(s.finds_open, 0);
    }
}
