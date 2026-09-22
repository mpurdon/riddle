//! The Mesh service over `repo::meshes` (§36–§38, §51). Versions are never
//! edited in place; a changed Mesh becomes a new version in Shadow because
//! "new and changed Meshes go through the same period" (README). Deletion is
//! soft and restore refuses a name clash (§51).

use crate::parse::parse_full;
use crate::validate::{validate, RegistryResolver};
use riddle_core::{CreatedBy, DigId, Mesh, MeshId, MeshScope, MeshStatus};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use riddle_store::{repo, Store};
use std::sync::Arc;

pub struct MeshService {
    index: Arc<Store>,
}

impl MeshService {
    pub fn new(index: Arc<Store>) -> Self {
        Self { index }
    }

    pub fn store(&self) -> &Store {
        &self.index
    }

    pub fn resolver(&self) -> RegistryResolver {
        RegistryResolver {
            index: self.index.clone(),
        }
    }

    /// Seed the default Meshes where absent. Existing rows keep their status.
    pub fn seed_defaults(&self) -> Result<usize> {
        let mut n = 0;
        for m in crate::defaults::default_meshes() {
            if repo::meshes::latest(&self.index, &m.id)?.is_none() {
                repo::meshes::upsert(&self.index, &m)?;
                n += 1;
            }
        }
        if n > 0 {
            rinfo!(Category::Mesh, "seeded {n} default meshes");
        }
        Ok(n)
    }

    pub fn latest(&self, id: &MeshId) -> Result<Mesh> {
        repo::meshes::latest(&self.index, id)?
            .ok_or_else(|| RiddleError::not_found(format!("mesh {id}")))
    }

    pub fn get(&self, id: &MeshId, version: u32) -> Result<Mesh> {
        repo::meshes::get(&self.index, id, version)?
            .ok_or_else(|| RiddleError::not_found(format!("mesh {id} v{version}")))
    }

    pub fn list(&self, include_deleted: bool) -> Result<Vec<Mesh>> {
        repo::meshes::list_latest(&self.index, include_deleted)
    }

    pub fn versions(&self, id: &MeshId) -> Result<Vec<Mesh>> {
        repo::meshes::versions(&self.index, id)
    }

    /// Running Meshes (Active or Shadow) whose scope applies to the Dig.
    pub fn in_scope(&self, dig_id: &DigId, group: Option<&str>) -> Result<Vec<Mesh>> {
        Ok(repo::meshes::list_running(&self.index)?
            .into_iter()
            .filter(|m| m.scope.applies_to(dig_id, group))
            .collect())
    }

    fn check_name_free(&self, name: &str, except: Option<&MeshId>) -> Result<()> {
        if let Some(existing) = repo::meshes::find_by_name(&self.index, name)? {
            if except != Some(&existing.id) {
                return Err(
                    RiddleError::invalid(format!("a Mesh named {name:?} already exists"))
                        .with_detail(serde_json::json!({ "meshId": existing.id })),
                );
            }
        }
        Ok(())
    }

    /// Parse, validate and store a new Mesh at version 1.
    pub fn create(
        &self,
        text: &str,
        scope: Option<MeshScope>,
        status: MeshStatus,
        by: CreatedBy,
    ) -> Result<Mesh> {
        let (mut m, _) = parse_full(text)?;
        if let Some(s) = scope {
            m.scope = s;
        }
        validate(&m, &self.resolver())?;
        self.check_name_free(&m.name, None)?;
        m.version = 1;
        m.status = if status == MeshStatus::Deleted {
            MeshStatus::Draft
        } else {
            status
        };
        m.created_by = by;
        m.created_at = chrono::Utc::now();
        if m.status == MeshStatus::Shadow {
            m.shadow_started_at = Some(m.created_at);
        }
        repo::meshes::upsert(&self.index, &m)?;
        rinfo!(Category::Mesh, "created mesh {} v1 ({:?})", m.id, m.status);
        Ok(m)
    }

    /// A new version of an existing Mesh. Active or Shadow Meshes restart
    /// their Shadow Period; Draft and Disabled keep their status.
    pub fn update(
        &self,
        id: &MeshId,
        text: &str,
        change_note: Option<String>,
        by: CreatedBy,
    ) -> Result<Mesh> {
        let prior = self.latest(id)?;
        if prior.status == MeshStatus::Deleted {
            return Err(RiddleError::invalid("restore the Mesh before changing it"));
        }
        let (mut m, header) = parse_full(text)?;
        if !header.scope_given {
            m.scope = prior.scope.clone();
        }
        validate(&m, &self.resolver())?;
        self.check_name_free(&m.name, Some(id))?;
        m.id = id.clone();
        m.version = repo::meshes::next_version(&self.index, id)?;
        m.status = match prior.status {
            MeshStatus::Active | MeshStatus::Shadow => MeshStatus::Shadow,
            other => other,
        };
        m.created_by = by;
        m.created_at = chrono::Utc::now();
        m.change_note = change_note;
        if m.status == MeshStatus::Shadow {
            m.shadow_started_at = Some(m.created_at);
        }
        repo::meshes::upsert(&self.index, &m)?;
        rinfo!(
            Category::Mesh,
            "updated mesh {} to v{} ({:?})",
            m.id,
            m.version,
            m.status
        );
        Ok(m)
    }

    pub fn set_status(&self, id: &MeshId, status: MeshStatus) -> Result<Mesh> {
        let m = self.latest(id)?;
        if status == MeshStatus::Deleted {
            return self.delete(id);
        }
        if m.status == MeshStatus::Deleted {
            return Err(RiddleError::invalid(
                "restore the Mesh before changing its status",
            ));
        }
        repo::meshes::set_status(&self.index, id, m.version, status)?;
        self.latest(id)
    }

    pub fn delete(&self, id: &MeshId) -> Result<Mesh> {
        let m = self.latest(id)?;
        repo::meshes::set_status(&self.index, id, m.version, MeshStatus::Deleted)?;
        self.latest(id)
    }

    /// Restore a soft-deleted Mesh as Disabled; refuses a name clash.
    pub fn restore(&self, id: &MeshId) -> Result<Mesh> {
        let mut m = self.latest(id)?;
        if m.status != MeshStatus::Deleted {
            return Ok(m);
        }
        self.check_name_free(&m.name, Some(id))?;
        m.status = MeshStatus::Disabled;
        m.deleted_at = None;
        repo::meshes::upsert(&self.index, &m)?;
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_store::StoreKind;

    fn service() -> MeshService {
        let index = Arc::new(Store::in_memory(StoreKind::Index).unwrap());
        for s in ["newsletter", "personally_relevant"] {
            repo::signals::upsert(&index, &riddle_core::Signal::noul(s, "x")).unwrap();
        }
        MeshService::new(index)
    }

    #[test]
    fn create_update_delete_and_restore_follow_the_versioning_rules() {
        let svc = service();
        let m = svc
            .create(
                "Mesh: News\nScope:\nWork Digs\nROUTES\nIF newsletter@v1 > .8 → Spoil",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        assert_eq!(m.version, 1);
        let err = svc
            .create(
                "Mesh: News\nROUTES\nIF newsletter@v1 > .8 → Spoil",
                None,
                MeshStatus::Draft,
                CreatedBy::User,
            )
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        let v2 = svc
            .update(
                &m.id,
                "Mesh: News\nROUTES\nIF newsletter@v1 > .9 → Spoil",
                Some("tighter".into()),
                CreatedBy::Director,
            )
            .unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(
            v2.status,
            MeshStatus::Shadow,
            "a changed Mesh re-enters the Shadow Period"
        );
        assert_eq!(
            v2.scope,
            MeshScope::DigGroup {
                group: "Work".into()
            },
            "scope is kept when the text has none"
        );
        let deleted = svc.delete(&m.id).unwrap();
        assert_eq!(deleted.status, MeshStatus::Deleted);
        assert!(svc.list(false).unwrap().is_empty());
        svc.create(
            "Mesh: News\nROUTES\nIF newsletter@v1 > .8 → Spoil",
            None,
            MeshStatus::Draft,
            CreatedBy::User,
        )
        .unwrap();
        let err = svc.restore(&m.id).unwrap_err();
        assert_eq!(
            err.kind,
            riddle_errors::ErrorKind::Invalid,
            "restore refuses a name clash"
        );
    }

    #[test]
    fn an_unregistered_signal_is_refused_and_defaults_seed_once() {
        let svc = service();
        let err = svc
            .create(
                "Mesh: X\nROUTES\nIF nope@v1 > .8 → Spoil",
                None,
                MeshStatus::Draft,
                CreatedBy::User,
            )
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        let index = svc.store();
        for name in [
            "security_alert",
            "account_notification",
            "routine",
            "meeting_related",
            "requires_response",
            "review_requested",
            "ci_failure",
            "personally_relevant",
            "deployment_failed",
            "owned_work_failed",
            "receipt",
            "invoice",
            "requires_action",
            "payment_related",
            "represents_charge",
            "marketing",
            "human_authored",
            "asks_question",
            "social_notification",
            "transactional",
        ] {
            repo::signals::upsert(index, &riddle_core::Signal::noul(name, "x")).unwrap();
        }
        assert!(svc.seed_defaults().unwrap() >= 9);
        assert_eq!(svc.seed_defaults().unwrap(), 0);
        let in_scope = svc.in_scope(&DigId("d".into()), Some("Work")).unwrap();
        assert!(in_scope.iter().any(|m| m.name == "System guards"));
    }
}
