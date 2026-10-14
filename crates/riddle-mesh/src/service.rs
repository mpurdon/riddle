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
            .map(read)
            .ok_or_else(|| RiddleError::not_found(format!("mesh {id}")))
    }

    pub fn get(&self, id: &MeshId, version: u32) -> Result<Mesh> {
        repo::meshes::get(&self.index, id, version)?
            .map(read)
            .ok_or_else(|| RiddleError::not_found(format!("mesh {id} v{version}")))
    }

    pub fn list(&self, include_deleted: bool) -> Result<Vec<Mesh>> {
        Ok(repo::meshes::list_latest(&self.index, include_deleted)?
            .into_iter()
            .map(read)
            .collect())
    }

    pub fn versions(&self, id: &MeshId) -> Result<Vec<Mesh>> {
        Ok(repo::meshes::versions(&self.index, id)?
            .into_iter()
            .map(read)
            .collect())
    }

    /// Running Meshes (Active or Shadow) whose scope applies to the Dig.
    /// The running Meshes for a Dig, Shadow versions included: what Signals
    /// are selected for, and what a Shadow proposal is measured with.
    pub fn in_scope(&self, dig_id: &DigId, group: Option<&str>) -> Result<Vec<Mesh>> {
        Ok(repo::meshes::list_running(&self.index)?
            .into_iter()
            .filter(|m| m.scope.applies_to(dig_id, group))
            .collect())
    }

    /// The Mesh versions that decide a Dig's mail (`repo::meshes::list_live`):
    /// a Mesh in Shadow decides with its last Active version, or not at all.
    pub fn live_in_scope(&self, dig_id: &DigId, group: Option<&str>) -> Result<Vec<Mesh>> {
        Ok(repo::meshes::list_live(&self.index)?
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

    /// Meshes in one scope run in Priority order, and a tie was broken by
    /// name without a word: Cold outreach and GitHub Notifications both sat
    /// at 10, and which saw GitHub mail first depended on the alphabet. A
    /// running Mesh may not share its scope and Priority with another.
    fn check_priority_free(&self, m: &Mesh) -> Result<()> {
        if !matches!(m.status, MeshStatus::Active | MeshStatus::Shadow) {
            return Ok(());
        }
        let taken: Vec<Mesh> = repo::meshes::list_running(&self.index)?
            .into_iter()
            .filter(|o| o.id != m.id && o.scope == m.scope)
            .collect();
        let Some(other) = taken.iter().find(|o| o.priority == m.priority) else {
            return Ok(());
        };
        let free = |step: i32| {
            (1..)
                .map(|k| m.priority + step * k)
                .find(|p| !taken.iter().any(|o| o.priority == *p))
                .unwrap_or(m.priority)
        };
        let (before, after) = (free(-1), free(1));
        Err(RiddleError::invalid(format!(
            "Priority {} is already Mesh {:?}'s in this scope; Meshes run in Priority order and a tie would be decided by name. Use {before} to run before it or {after} to run after it.",
            m.priority, other.name
        ))
        .with_detail(serde_json::json!({ "meshId": other.id, "before": before, "after": after })))
    }

    /// A Mesh may only name lists that exist: an unknown list would match
    /// nothing, silently, which is worse than refusing the rule.
    fn check_lists_exist(&self, m: &Mesh) -> Result<()> {
        let mut names = Vec::new();
        for c in m
            .routes
            .iter()
            .map(|r| &r.condition)
            .chain(m.guards.iter().map(|g| &g.condition))
        {
            named_lists_in(c, &mut names);
        }
        let missing: Vec<String> = names
            .into_iter()
            .filter(|n| repo::lists::get(&self.index, n).ok().flatten().is_none())
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        Err(RiddleError::invalid(format!(
            "{} does not exist yet; create it first (save_list) so the rule has something to match",
            missing
                .iter()
                .map(|n| format!("list:{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    /// The first priority from 0 up that no running Mesh in the scope holds.
    fn first_free_priority(&self, scope: &MeshScope, except: Option<&MeshId>) -> Result<i32> {
        let taken: Vec<i32> = repo::meshes::list_running(&self.index)?
            .into_iter()
            .filter(|o| Some(&o.id) != except && &o.scope == scope)
            .map(|o| o.priority)
            .collect();
        Ok((0..).find(|p| !taken.contains(p)).unwrap_or(0))
    }

    /// Parse, validate and store a new Mesh at version 1.
    pub fn create(
        &self,
        text: &str,
        scope: Option<MeshScope>,
        status: MeshStatus,
        by: CreatedBy,
    ) -> Result<Mesh> {
        let (mut m, header) = parse_full(text)?;
        if let Some(s) = scope {
            m.scope = s;
        }
        if !header.priority_given {
            m.priority = self.first_free_priority(&m.scope, None)?;
        }
        validate(&m, &self.resolver())?;
        self.check_lists_exist(&m)?;
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
        self.check_priority_free(&m)?;
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
        if !header.autonomy_given {
            m.autonomy = prior.autonomy;
        }
        if !header.priority_given {
            m.priority = prior.priority;
        }
        m.fallback = prior.fallback;
        m.pack = prior.pack.clone();
        validate(&m, &self.resolver())?;
        self.check_lists_exist(&m)?;
        self.check_name_free(&m.name, Some(id))?;
        m.id = id.clone();
        m.version = repo::meshes::next_version(&self.index, id)?;
        // A changed Mesh proposes until it earns Active again, except the
        // user's own sender rules ("Your corrections"): guards that veto,
        // and routes that name one sender or domain. The user stated each
        // exactly, so there is nothing to earn, and a rule must work the
        // moment it is added.
        m.status = match prior.status {
            MeshStatus::Active if sender_rules_only(&m) => MeshStatus::Active,
            MeshStatus::Active | MeshStatus::Shadow => MeshStatus::Shadow,
            other => other,
        };
        m.created_by = by;
        m.created_at = chrono::Utc::now();
        m.change_note = change_note;
        if m.status == MeshStatus::Shadow {
            m.shadow_started_at = Some(m.created_at);
        }
        self.check_priority_free(&m)?;
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
        self.check_priority_free(&Mesh {
            status,
            ..m.clone()
        })?;
        repo::meshes::set_status(&self.index, id, m.version, status)?;
        self.latest(id)
    }

    /// Mark the current version a fallback or not, in place: it changes no
    /// decision, so it neither makes a version nor restarts a Shadow Period.
    pub fn set_fallback(&self, id: &MeshId, fallback: bool) -> Result<Mesh> {
        let mut m = self.latest(id)?;
        m.fallback = fallback;
        repo::meshes::upsert(&self.index, &m)?;
        Ok(m)
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

/// The names of the named lists an expression reads.
pub fn named_lists_in(e: &riddle_core::Expr, out: &mut Vec<String>) {
    use riddle_core::Expr;
    match e {
        Expr::InNamedList { list, .. } => {
            if !out.contains(list) {
                out.push(list.clone());
            }
        }
        Expr::And { items } | Expr::Or { items } => {
            items.iter().for_each(|i| named_lists_in(i, out))
        }
        Expr::Not { item } => named_lists_in(item, out),
        _ => {}
    }
}

/// Every route names exactly one sender or domain (`sender = "a@b"`,
/// `sender_domain = "b"`), and there is at least one rule: a Mesh of the
/// user's own stated rules, not a policy that has to earn trust.
pub fn sender_rules_only(m: &Mesh) -> bool {
    use riddle_core::{CmpOp, Expr, FactValue};
    (!m.routes.is_empty() || !m.guards.is_empty())
        && m.routes.iter().all(|r| {
            matches!(
                &r.condition,
                Expr::Fact { fact, op: CmpOp::Eq, value: FactValue::Text(_) }
                    if fact == "sender" || fact == "sender_domain"
            )
        })
}

/// A Mesh stored before routes carried their reading gets one on the way out.
fn read(mut m: Mesh) -> Mesh {
    crate::humanize::fill(&mut m);
    m
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
    fn two_running_meshes_in_one_scope_never_share_a_priority() {
        let svc = service();
        let first = svc
            .create(
                "Mesh: First\nPriority: 10\nROUTES\nIF mailing_list → Filed",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        let e = svc
            .create(
                "Mesh: Second\nPriority: 10\nROUTES\nIF mailing_list → Spoil",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap_err();
        assert!(
            e.message.contains("already Mesh \"First\"'s"),
            "{}",
            e.message
        );
        assert!(
            e.message.contains("Use 9") && e.message.contains("or 11"),
            "{}",
            e.message
        );

        // A draft may share it; activating it may not.
        let draft = svc
            .create(
                "Mesh: Second\nPriority: 10\nROUTES\nIF mailing_list → Spoil",
                None,
                MeshStatus::Draft,
                CreatedBy::User,
            )
            .unwrap();
        assert!(svc.set_status(&draft.id, MeshStatus::Active).is_err());

        // Without a Priority line: a new Mesh takes a free one, an update keeps its own.
        let third = svc
            .create(
                "Mesh: Third\nROUTES\nIF mailing_list → Filed",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        assert_eq!(third.priority, 0);
        let fourth = svc
            .create(
                "Mesh: Fourth\nROUTES\nIF mailing_list → Filed",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        assert_eq!(fourth.priority, 1);
        let updated = svc
            .update(
                &first.id,
                "Mesh: First\nROUTES\nIF mailing_list → Spoil",
                None,
                CreatedBy::User,
            )
            .unwrap();
        assert_eq!(
            updated.priority, 10,
            "an update without Priority keeps its place"
        );
    }

    #[test]
    fn a_fallback_mark_changes_no_version_and_survives_an_update() {
        let svc = service();
        let m = svc
            .create(
                "Mesh: Updates\nPriority: 45\nROUTES\nIF mailing_list → Filed",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        let marked = svc.set_fallback(&m.id, true).unwrap();
        assert!(marked.fallback);
        assert_eq!((marked.version, marked.status), (1, MeshStatus::Active));
        let next = svc
            .update(
                &m.id,
                "Mesh: Updates\nROUTES\nIF mailing_list → Spoil",
                None,
                CreatedBy::User,
            )
            .unwrap();
        assert!(next.fallback, "a new version is still a fallback");
    }

    #[test]
    fn a_guard_only_mesh_keeps_active_when_a_line_is_added_and_a_cap_survives_an_update() {
        let svc = service();
        let rules = svc
            .create(
                "Mesh: Your corrections v1\nScope: All Digs\n\nGUARDS\nNEVER Spoil IF sender = \"a@x.example\"",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        let v2 = svc
            .update(
                &rules.id,
                "Mesh: Your corrections v1\nScope: All Digs\n\nGUARDS\nNEVER Spoil IF sender = \"a@x.example\"\nNEVER Spoil IF sender_domain = \"y.example\"",
                Some("Added a correction".into()),
                CreatedBy::User,
            )
            .unwrap();
        assert_eq!(
            v2.status,
            MeshStatus::Active,
            "guards veto and never hide mail; nothing to earn"
        );
        assert_eq!(v2.guards.len(), 2);

        let capped = svc
            .create(
                "Mesh: Promotions\nAutonomy: Suggest\nROUTES\nIF newsletter@v1 > .8 → Spoil",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        assert_eq!(capped.autonomy, Some(riddle_core::Autonomy::Suggest));
        let next = svc
            .update(
                &capped.id,
                "Mesh: Promotions\nROUTES\nIF newsletter@v1 > .9 → Spoil",
                None,
                CreatedBy::User,
            )
            .unwrap();
        assert_eq!(
            next.autonomy,
            Some(riddle_core::Autonomy::Suggest),
            "an update without the line keeps the cap"
        );
        assert_eq!(
            next.status,
            MeshStatus::Shadow,
            "a routed change still re-enters Shadow"
        );
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
