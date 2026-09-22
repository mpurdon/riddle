//! The Signal Registry service over `repo::signals` (§23, §27, §30).
//!
//! Seeding is idempotent so every start of the app can call it; a version is
//! never edited in place because Meshes pin `name@vN` and historical
//! evaluations retain their version. Promotion requires a test report or an
//! explicit override, which is invariant 15 as code: the incident was a
//! Signal rewritten by hand that quietly flipped a Mesh for a week.

use crate::testing::SignalTestReport;
use riddle_core::{CreatedBy, Signal, SignalId, SignalStatus};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use riddle_store::{repo, Store};
use std::sync::Arc;

pub struct Registry {
    index: Arc<Store>,
}

#[derive(Debug, Clone, Default)]
pub struct PromoteOptions<'a> {
    /// The test report that justifies promotion (§31).
    pub report: Option<&'a SignalTestReport>,
    /// The user explicitly chose to promote untested (invariant 15).
    pub override_untested: bool,
}

impl Registry {
    pub fn new(index: Arc<Store>) -> Self {
        Self { index }
    }

    pub fn store(&self) -> &Store {
        &self.index
    }

    /// Seed the system vocabulary as ACTIVE v1 where absent. Returns how many
    /// were newly written; existing rows (any status) are left alone so a
    /// deprecation survives restarts.
    pub fn seed_system(&self) -> Result<usize> {
        let mut seeded = 0;
        for s in crate::vocabulary::system_signals() {
            if repo::signals::get(&self.index, &s.name, s.version)?.is_none() {
                repo::signals::upsert(&self.index, &s)?;
                seeded += 1;
            }
        }
        if seeded > 0 {
            rinfo!(Category::Sift, "seeded {seeded} system signals");
        }
        Ok(seeded)
    }

    /// Propose a new version of a Signal: validate, assign the next version,
    /// status PROPOSED. The proposer's `version` and `status` are ignored.
    pub fn propose(&self, mut signal: Signal, by: CreatedBy) -> Result<Signal> {
        if let Err(problems) = signal.validate() {
            return Err(
                RiddleError::invalid("signal definition breaks the authoring rules")
                    .with_detail(serde_json::json!({ "problems": problems })),
            );
        }
        let existing = repo::signals::versions(&self.index, &signal.name)?;
        signal.version = repo::signals::next_version(&self.index, &signal.name)?;
        signal.id = existing
            .first()
            .map(|s| s.id.clone())
            .unwrap_or_else(|| SignalId(format!("sig_{}", signal.name)));
        signal.status = SignalStatus::Proposed;
        signal.created_by = by;
        signal.created_at = chrono::Utc::now();
        signal.calibration = None;
        repo::signals::upsert(&self.index, &signal)?;
        rinfo!(
            Category::Sift,
            "proposed signal {}@v{}",
            signal.name,
            signal.version
        );
        Ok(signal)
    }

    /// Promote a version to ACTIVE and deprecate the previously ACTIVE one.
    pub fn promote(&self, name: &str, version: u32, opts: PromoteOptions<'_>) -> Result<Signal> {
        let mut target = self.resolve(name, version)?;
        if target.status == SignalStatus::Active {
            return Ok(target);
        }
        if target.status == SignalStatus::Rejected {
            return Err(RiddleError::invalid(format!(
                "{name}@v{version} was rejected; propose a new version"
            )));
        }
        match opts.report {
            Some(r) if r.name == name && r.version == version && r.sample_size > 0 => {}
            Some(_) => {
                return Err(RiddleError::invalid(
                    "the test report is for a different Signal version",
                ))
            }
            None if opts.override_untested => {}
            None => {
                return Err(RiddleError::policy_denied(format!(
                    "{name}@v{version} has no test report; run test_signal or override explicitly"
                )))
            }
        }
        for mut prior in repo::signals::versions(&self.index, name)?
            .into_iter()
            .filter(|s| s.status == SignalStatus::Active && s.version != version)
        {
            prior.status = SignalStatus::Deprecated;
            repo::signals::upsert(&self.index, &prior)?;
        }
        target.status = SignalStatus::Active;
        if let Some(r) = opts.report {
            target.calibration = r.calibration.clone().or(target.calibration);
        }
        repo::signals::upsert(&self.index, &target)?;
        rinfo!(Category::Sift, "promoted signal {name}@v{version}");
        Ok(target)
    }

    pub fn set_status(&self, name: &str, version: u32, status: SignalStatus) -> Result<Signal> {
        let mut s = self.resolve(name, version)?;
        s.status = status;
        repo::signals::upsert(&self.index, &s)?;
        Ok(s)
    }

    pub fn resolve(&self, name: &str, version: u32) -> Result<Signal> {
        repo::signals::get(&self.index, name, version)?
            .ok_or_else(|| RiddleError::not_found(format!("signal {name}@v{version}")))
    }

    /// The ACTIVE version of a Signal, for the Baseline set and Sift.
    pub fn resolve_active(&self, name: &str) -> Result<Signal> {
        repo::signals::latest(&self.index, name, &[SignalStatus::Active])?
            .ok_or_else(|| RiddleError::not_found(format!("no active version of signal {name}")))
    }

    pub fn list(&self) -> Result<Vec<Signal>> {
        repo::signals::list(&self.index)
    }

    /// The latest version of every Signal name.
    pub fn list_latest(&self) -> Result<Vec<Signal>> {
        let mut latest: std::collections::BTreeMap<String, Signal> =
            std::collections::BTreeMap::new();
        for s in self.list()? {
            let replace = latest
                .get(&s.name)
                .map(|cur| s.version > cur.version)
                .unwrap_or(true);
            if replace {
                latest.insert(s.name.clone(), s);
            }
        }
        Ok(latest.into_values().collect())
    }

    pub fn versions(&self, name: &str) -> Result<Vec<Signal>> {
        repo::signals::versions(&self.index, name)
    }

    pub fn active_signals(&self) -> Result<Vec<Signal>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|s| s.status == SignalStatus::Active)
            .collect())
    }

    /// Store a calibration artifact on a version (§27).
    pub fn set_calibration(
        &self,
        name: &str,
        version: u32,
        calibration: riddle_core::Calibration,
    ) -> Result<Signal> {
        let mut s = self.resolve(name, version)?;
        s.calibration = Some(calibration);
        repo::signals::upsert(&self.index, &s)?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_store::{Store, StoreKind};

    fn registry() -> Registry {
        Registry::new(Arc::new(Store::in_memory(StoreKind::Index).unwrap()))
    }

    #[test]
    fn seeding_twice_writes_the_vocabulary_once() {
        let r = registry();
        let first = r.seed_system().unwrap();
        assert!(first >= 27);
        assert_eq!(r.seed_system().unwrap(), 0);
        assert_eq!(r.resolve_active("review_requested").unwrap().version, 1);
    }

    #[test]
    fn a_proposal_gets_the_next_version_and_promotion_needs_a_report_or_an_override() {
        let r = registry();
        r.seed_system().unwrap();
        let mut candidate = Signal::noul(
            "review_requested",
            "The recipient is named as a reviewer of a change.",
        );
        candidate.version = 99;
        let proposed = r.propose(candidate, CreatedBy::Director).unwrap();
        assert_eq!(proposed.version, 2);
        assert_eq!(proposed.status, SignalStatus::Proposed);
        assert_eq!(proposed.id, r.resolve("review_requested", 1).unwrap().id);

        let err = r
            .promote("review_requested", 2, PromoteOptions::default())
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::PolicyDenied);

        let promoted = r
            .promote(
                "review_requested",
                2,
                PromoteOptions {
                    report: None,
                    override_untested: true,
                },
            )
            .unwrap();
        assert_eq!(promoted.status, SignalStatus::Active);
        assert_eq!(
            r.resolve("review_requested", 1).unwrap().status,
            SignalStatus::Deprecated
        );
        assert_eq!(r.resolve_active("review_requested").unwrap().version, 2);
    }

    #[test]
    fn a_definition_with_a_negation_is_refused_at_proposal() {
        let r = registry();
        let bad = Signal::noul("not_spam", "The message is not spam.");
        let err = r.propose(bad, CreatedBy::User).unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        assert!(!err.detail.unwrap()["problems"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
