//! Network loss (§55): a probe decides online or offline; offline pauses
//! everything with `Offline` and a faster re-probe clears it. The default
//! probe is a DNS lookup of the provider host with a 3 s timeout: cheap,
//! needs no credential, and fails fast on a captive portal. Provider crates
//! can also report an observed network failure directly.

use crate::queue::JobQueue;
use crate::spec::PauseReason;
use async_trait::async_trait;
use riddle_observability::{rinfo, rwarn, Category};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[async_trait]
pub trait Connectivity: Send + Sync {
    async fn online(&self) -> bool;
}

pub struct DnsProbe {
    pub host: String,
    pub timeout: Duration,
}

impl DnsProbe {
    pub fn gmail() -> Self {
        Self {
            host: "gmail.googleapis.com".into(),
            timeout: Duration::from_secs(3),
        }
    }
}

#[async_trait]
impl Connectivity for DnsProbe {
    async fn online(&self) -> bool {
        let target = format!("{}:443", self.host);
        match tokio::time::timeout(self.timeout, tokio::net::lookup_host(target)).await {
            Ok(Ok(mut addrs)) => addrs.next().is_some(),
            _ => false,
        }
    }
}

/// A scripted answer for tests.
pub struct FixedConnectivity(pub AtomicBool);
impl FixedConnectivity {
    pub fn online() -> Self {
        Self(AtomicBool::new(true))
    }
    pub fn set(&self, online: bool) {
        self.0.store(online, Ordering::SeqCst);
    }
}
#[async_trait]
impl Connectivity for FixedConnectivity {
    async fn online(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub const ONLINE_INTERVAL: Duration = Duration::from_secs(30);
pub const OFFLINE_INTERVAL: Duration = Duration::from_secs(10);

pub struct ConnectivityMonitor {
    queue: Arc<JobQueue>,
    probe: Arc<dyn Connectivity>,
    offline: AtomicBool,
}

impl ConnectivityMonitor {
    pub fn new(queue: Arc<JobQueue>, probe: Arc<dyn Connectivity>) -> Arc<Self> {
        Arc::new(Self {
            queue,
            probe,
            offline: AtomicBool::new(false),
        })
    }

    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::SeqCst)
    }

    /// One probe; applies the transition. Returns the online state.
    pub async fn probe_once(&self) -> bool {
        let online = self.probe.online().await;
        self.apply(online);
        online
    }

    /// A provider crate saw a network failure: go offline now, re-probe later.
    pub fn report_offline(&self) {
        self.apply(false);
    }

    fn apply(&self, online: bool) {
        let was_offline = self.offline.swap(!online, Ordering::SeqCst);
        match (was_offline, online) {
            (false, false) => {
                rwarn!(Category::Jobs, "network unreachable; pausing jobs");
                self.queue.pause_all(PauseReason::Offline);
            }
            (true, true) => {
                rinfo!(Category::Jobs, "network back; resuming jobs");
                self.queue.clear_pause(PauseReason::Offline);
            }
            _ => {}
        }
    }

    pub fn start(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                let online = me.probe_once().await;
                tokio::time::sleep(if online {
                    ONLINE_INTERVAL
                } else {
                    OFFLINE_INTERVAL
                })
                .await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{RecordingSink, SystemClock};
    use riddle_store::{Store, StoreKind};

    #[tokio::test]
    async fn losing_the_network_pauses_with_offline_and_regaining_it_clears_only_that_reason() {
        let queue = JobQueue::open(
            Arc::new(Store::in_memory(StoreKind::Index).unwrap()),
            Arc::new(SystemClock),
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
        let probe = Arc::new(FixedConnectivity::online());
        let monitor = ConnectivityMonitor::new(queue.clone(), probe.clone());
        assert!(monitor.probe_once().await);
        assert!(!queue.is_paused());
        probe.set(false);
        assert!(!monitor.probe_once().await);
        assert_eq!(queue.pause_reasons(), vec![PauseReason::Offline]);
        queue.pause_all(PauseReason::Budget);
        probe.set(true);
        monitor.probe_once().await;
        assert_eq!(
            queue.pause_reasons(),
            vec![PauseReason::Budget],
            "the budget pause stays"
        );
    }
}
