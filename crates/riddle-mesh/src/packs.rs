//! Rule packs: named, versioned bundles of Meshes and the named lists they
//! read, so a new person starts from rules written for the kinds of mail
//! they get (GitHub, money, shopping…) instead of one person's mailbox.
//!
//! A pack's Meshes never carry one person's values: they read named lists
//! (`sender_domain IN list:keep_promotions`) and the person fills the lists
//! in when installing, or later. That is what lets a pack be shared, and a
//! newer version replace its Meshes without overwriting anyone's choices.
//!
//! Packs do not number their Meshes. Each Mesh sits in a band (safety,
//! personal, specific, general, fallback) and installing gives it the next
//! free Priority in that band, so the run order is "safety first, specific
//! rules before generic ones, catch-alls last" without anyone choosing
//! numbers, and two Meshes never tie (`MeshService::check_priority_free`).
//! A pack is a TOML file; the built-in ones live in `packs/` and are
//! compiled in.

use crate::service::MeshService;
use riddle_core::{CreatedBy, Mesh, MeshId, MeshStatus, NamedListDef, PackRef};
use riddle_errors::{Result, RiddleError};
use riddle_store::{repo, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Where a pack's Mesh runs. Lower runs first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Band {
    /// Never Hidden guards, government mail, scams.
    Safety,
    /// Mail from people.
    Personal,
    /// Rules for a named kind of sender: GitHub, your bank.
    #[default]
    Specific,
    /// Rules on what mail is about: receipts, payments, promotions.
    General,
    /// Catch-alls for what nothing else decided.
    Fallback,
}

impl Band {
    /// The Priority range a band's Meshes take, inclusive. Your corrections
    /// (-90) sit in the safety band, after the Never Hidden guards.
    pub fn range(self) -> (i32, i32) {
        match self {
            Band::Safety => (-100, -61),
            Band::Personal => (-60, -21),
            Band::Specific => (-20, 19),
            Band::General => (20, 39),
            Band::Fallback => (40, 69),
        }
    }
}

/// How onboarding decides a pack applies to a mailbox: enough mail from
/// these domains (or their subdomains) in the scan window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub struct PackDetect {
    #[serde(default)]
    pub sender_domains: Vec<String>,
    /// Gmail categories that make the pack relevant (`promotions`).
    #[serde(default)]
    pub gmail_categories: Vec<String>,
    #[serde(default)]
    pub min_messages: u32,
    /// Offered to everyone (Core).
    #[serde(default)]
    pub always: bool,
}

/// A list a pack's Meshes read, and what to ask the person about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub struct PackList {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The question onboarding asks: "Which stores' promotions do you still want to see?"
    #[serde(default)]
    pub question: String,
    /// Entries the list starts with: generic ones (`linkedin.com`), never one person's.
    #[serde(default)]
    pub default: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub struct PackMesh {
    /// The Mesh text without `Priority:`; installing assigns one in the band.
    pub text: String,
    /// The band, when it differs from the pack's.
    #[serde(default)]
    pub band: Option<Band>,
    /// A catch-all (`Mesh::fallback`).
    #[serde(default)]
    pub fallback: bool,
    /// Where in its band the Mesh belongs: Priority = band start + rank,
    /// or the next free slot after it. Packs order their Meshes by rank,
    /// not by install order, so installing Shopping before Money still
    /// runs payments before promotions.
    #[serde(default)]
    pub rank: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub struct Pack {
    /// `developer-github`: lowercase letters, digits and `-`.
    pub id: String,
    pub name: String,
    pub version: u32,
    /// What it is for, as a group in the catalogue: "Developer", "Money".
    pub area: String,
    pub summary: String,
    #[serde(default)]
    pub band: Band,
    #[serde(default)]
    pub detect: PackDetect,
    #[serde(default)]
    pub lists: Vec<PackList>,
    pub meshes: Vec<PackMesh>,
}

/// A pack as installed on this store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstalledPack {
    pub id: String,
    pub version: u32,
    /// The Mesh each of the pack's Meshes became, by Mesh name.
    pub meshes: BTreeMap<String, MeshId>,
    pub installed_at: chrono::DateTime<chrono::Utc>,
}

/// One pack in the catalogue, with what it would do here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PackInfo {
    pub pack: Pack,
    pub builtin: bool,
    pub installed: Option<InstalledPack>,
    /// The Mesh names in the pack, and whether a Mesh by that name already
    /// exists here outside the pack (installing replaces it).
    pub mesh_names: Vec<String>,
    pub conflicts: Vec<String>,
    /// Each list's current entries here (empty when it does not exist yet).
    pub list_entries: BTreeMap<String, Vec<String>>,
}

const BUILTIN: &[&str] = &[
    include_str!("../packs/core.toml"),
    include_str!("../packs/government-canada.toml"),
    include_str!("../packs/developer-github.toml"),
    include_str!("../packs/money.toml"),
    include_str!("../packs/shopping.toml"),
    include_str!("../packs/reading-social.toml"),
    include_str!("../packs/documents.toml"),
    include_str!("../packs/accounts.toml"),
    include_str!("../packs/travel.toml"),
    include_str!("../packs/career.toml"),
];

/// Parse and check a pack file: its id, that every Mesh parses, that every
/// list it reads is one it declares (or one the person already has), and
/// that no Mesh numbers itself.
pub fn parse_pack(text: &str) -> Result<Pack> {
    let pack: Pack =
        toml::from_str(text).map_err(|e| RiddleError::invalid(format!("not a pack file: {e}")))?;
    let id_ok = !pack.id.is_empty()
        && pack
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !id_ok {
        return Err(RiddleError::invalid(
            "a pack id is lowercase letters, digits and -, like developer-github",
        ));
    }
    if pack.meshes.is_empty() {
        return Err(RiddleError::invalid(format!(
            "pack {} has no Meshes",
            pack.id
        )));
    }
    for l in &pack.lists {
        if !NamedListDef::valid_name(&l.name) {
            return Err(RiddleError::invalid(format!(
                "pack {}: list {} is not a list name",
                pack.id, l.name
            )));
        }
    }
    for pm in &pack.meshes {
        let (m, header) = crate::parse::parse_full(&pm.text)?;
        if header.priority_given {
            return Err(RiddleError::invalid(format!(
                "pack {}: Mesh {} sets a Priority; packs place Meshes by band",
                pack.id, m.name
            )));
        }
    }
    Ok(pack)
}

/// The packs compiled into Riddle.
pub fn builtin_packs() -> Vec<Pack> {
    BUILTIN
        .iter()
        .map(|t| parse_pack(t).expect("built-in pack parses"))
        .collect()
}

/// The pack file for a pack, as TOML: what export writes and import reads.
pub fn to_toml(pack: &Pack) -> Result<String> {
    toml::to_string_pretty(pack).map_err(|e| RiddleError::internal(format!("pack to TOML: {e}")))
}

const INSTALLED: &str = "pack_installed:";
const IMPORTED: &str = "pack_file:";

pub fn installed(index: &Store) -> Result<Vec<InstalledPack>> {
    Ok(repo::kv::list_prefix(index, INSTALLED)?
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str(&v).ok())
        .collect())
}

/// Packs imported from files, by id.
pub fn imported(index: &Store) -> Result<Vec<Pack>> {
    Ok(repo::kv::list_prefix(index, IMPORTED)?
        .into_iter()
        .filter_map(|(_, v)| parse_pack(&v).ok())
        .collect())
}

/// Store an imported pack file. A built-in id cannot be replaced.
pub fn save_imported(index: &Store, text: &str) -> Result<Pack> {
    let pack = parse_pack(text)?;
    if builtin_packs().iter().any(|b| b.id == pack.id) {
        return Err(RiddleError::invalid(format!(
            "{} is a built-in pack; give the imported one another id",
            pack.id
        )));
    }
    repo::kv::set(index, &format!("{IMPORTED}{}", pack.id), text)?;
    Ok(pack)
}

pub fn remove_imported(index: &Store, id: &str) -> Result<()> {
    index.write(|c| {
        c.execute("DELETE FROM kv WHERE key = ?1", [format!("{IMPORTED}{id}")])?;
        Ok(())
    })
}

/// Every pack Riddle knows: built-in, then imported.
pub fn all_packs(index: &Store) -> Result<Vec<(Pack, bool)>> {
    let mut out: Vec<(Pack, bool)> = builtin_packs().into_iter().map(|p| (p, true)).collect();
    out.extend(imported(index)?.into_iter().map(|p| (p, false)));
    Ok(out)
}

pub fn find_pack(index: &Store, id: &str) -> Result<(Pack, bool)> {
    all_packs(index)?
        .into_iter()
        .find(|(p, _)| p.id == id)
        .ok_or_else(|| RiddleError::not_found(format!("no pack {id}")))
}

fn mesh_name(pm: &PackMesh) -> Result<String> {
    Ok(crate::parse::parse_full(&pm.text)?.0.name)
}

/// The catalogue with what each pack would do on this store.
pub fn catalogue(index: &Store) -> Result<Vec<PackInfo>> {
    let installed = installed(index)?;
    let meshes = repo::meshes::list_latest(index, false)?;
    let lists = repo::lists::all(index)?;
    let mut out = Vec::new();
    for (pack, builtin) in all_packs(index)? {
        let mesh_names = pack
            .meshes
            .iter()
            .map(mesh_name)
            .collect::<Result<Vec<_>>>()?;
        let conflicts = mesh_names
            .iter()
            .filter(|n| {
                meshes.iter().any(|m| {
                    m.name.eq_ignore_ascii_case(n)
                        && m.pack.as_ref().map(|p| p.id.as_str()) != Some(pack.id.as_str())
                })
            })
            .cloned()
            .collect();
        let list_entries = pack
            .lists
            .iter()
            .map(|l| {
                let entries = lists
                    .iter()
                    .find(|x| x.name == l.name)
                    .map(|x| x.entries.clone())
                    .unwrap_or_default();
                (l.name.clone(), entries)
            })
            .collect();
        out.push(PackInfo {
            installed: installed.iter().find(|i| i.id == pack.id).cloned(),
            pack,
            builtin,
            mesh_names,
            conflicts,
            list_entries,
        });
    }
    Ok(out)
}

/// What installing did, for the Action's result and its undo.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    pub created: Vec<MeshId>,
    /// Meshes that existed (by name, or from an older version of the pack)
    /// and were replaced, with their text and Priority before.
    pub replaced: Vec<ReplacedMesh>,
    pub lists_created: Vec<String>,
    pub lists_before: Vec<NamedListDef>,
    pub previous: Option<InstalledPack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacedMesh {
    pub id: MeshId,
    pub prior_text: String,
    pub prior_pack: Option<PackRef>,
    pub prior_fallback: bool,
    pub prior_status: MeshStatus,
}

/// Install a pack, or update it to this version. Lists are created from the
/// answers given (or the pack's defaults); a list that already exists keeps
/// its entries, and answers are added to them. A Mesh by the same name that
/// is not from this pack is refused unless `replace` is set; a Mesh from an
/// older version of the pack is updated in place. New Meshes start in
/// `status` (Shadow unless the person chose Active).
pub fn install(
    index: &std::sync::Arc<Store>,
    pack: &Pack,
    answers: &BTreeMap<String, Vec<String>>,
    status: MeshStatus,
    replace: bool,
    by: CreatedBy,
) -> Result<InstallReport> {
    let svc = MeshService::new(index.clone());
    let previous = installed(index)?.into_iter().find(|i| i.id == pack.id);
    let mut report = InstallReport {
        previous: previous.clone(),
        ..Default::default()
    };
    // Conflicts first, so nothing is half-installed.
    let existing = repo::meshes::list_latest(index, false)?;
    let mut plan: Vec<(&PackMesh, String, Option<Mesh>)> = Vec::new();
    for pm in &pack.meshes {
        let name = mesh_name(pm)?;
        let current = existing
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(&name))
            .cloned();
        if let Some(m) = &current {
            let ours = m.pack.as_ref().is_some_and(|p| p.id == pack.id);
            if !ours && !replace {
                return Err(RiddleError::invalid(format!(
                    "a Mesh named {name:?} already exists; install with replace to let the pack take it over"
                ))
                .with_detail(serde_json::json!({ "meshId": m.id, "name": name })));
            }
        }
        plan.push((pm, name, current));
    }
    // Lists before Meshes: a Mesh may only name lists that exist.
    for l in &pack.lists {
        let answer = answers.get(&l.name);
        match repo::lists::get(index, &l.name)? {
            Some(before) => {
                if let Some(add) = answer {
                    let mut merged = before.entries.clone();
                    merged.extend(add.iter().cloned());
                    let after = NamedListDef {
                        entries: NamedListDef::normalise(&merged),
                        ..before.clone()
                    };
                    if after != before {
                        report.lists_before.push(before);
                        repo::lists::put(index, &after)?;
                    }
                }
            }
            None => {
                let entries = answer.cloned().unwrap_or_else(|| l.default.clone());
                repo::lists::put(
                    index,
                    &NamedListDef {
                        name: l.name.clone(),
                        description: l.description.clone(),
                        entries: NamedListDef::normalise(&entries),
                        pack: Some(pack.id.clone()),
                    },
                )?;
                report.lists_created.push(l.name.clone());
            }
        }
    }
    let pack_ref = PackRef {
        id: pack.id.clone(),
        version: pack.version,
    };
    let mut meshes = BTreeMap::new();
    for (pm, name, current) in plan {
        let band = pm.band.unwrap_or(pack.band);
        let priority = free_priority(index, band, pm.rank, current.as_ref().map(|m| &m.id))?;
        let text = with_priority(&pm.text, priority);
        let mesh = match current {
            Some(before) => {
                report.replaced.push(ReplacedMesh {
                    id: before.id.clone(),
                    prior_text: crate::format::format(&before),
                    prior_pack: before.pack.clone(),
                    prior_fallback: before.fallback,
                    prior_status: before.status,
                });
                svc.update(
                    &before.id,
                    &text,
                    Some(format!("From pack {} v{}", pack.name, pack.version)),
                    by,
                )?
            }
            None => {
                let m = svc.create(&text, None, status, by)?;
                report.created.push(m.id.clone());
                m
            }
        };
        let mut tagged = mesh.clone();
        tagged.pack = Some(pack_ref.clone());
        tagged.fallback = pm.fallback;
        // A Mesh the pack takes over keeps its status when only its place
        // moved; when its rules change it gets the status asked for.
        if let Some(r) = report.replaced.iter().find(|r| r.id == mesh.id) {
            let before = crate::parse::parse(&r.prior_text)?;
            tagged.status = if same_rules(&before, &mesh) {
                r.prior_status
            } else {
                status
            };
            if tagged.status == MeshStatus::Shadow && mesh.shadow_started_at.is_none() {
                tagged.shadow_started_at = Some(chrono::Utc::now());
            }
        }
        repo::meshes::upsert(index, &tagged)?;
        meshes.insert(name, mesh.id);
    }
    repo::kv::set_json(
        index,
        &format!("{INSTALLED}{}", pack.id),
        &InstalledPack {
            id: pack.id.clone(),
            version: pack.version,
            meshes,
            installed_at: chrono::Utc::now(),
        },
    )?;
    Ok(report)
}

/// Put back what `install` changed.
pub fn undo_install(
    index: &std::sync::Arc<Store>,
    pack_id: &str,
    report: &InstallReport,
) -> Result<()> {
    let svc = MeshService::new(index.clone());
    for id in &report.created {
        svc.delete(id)?;
    }
    for r in &report.replaced {
        let m = svc.update(
            &r.id,
            &r.prior_text,
            Some("Pack install undone".into()),
            CreatedBy::User,
        )?;
        let mut m = m;
        m.pack = r.prior_pack.clone();
        m.fallback = r.prior_fallback;
        m.status = r.prior_status;
        repo::meshes::upsert(index, &m)?;
    }
    for name in &report.lists_created {
        repo::lists::delete(index, name)?;
    }
    for before in &report.lists_before {
        repo::lists::put(index, before)?;
    }
    match &report.previous {
        Some(p) => repo::kv::set_json(index, &format!("{INSTALLED}{pack_id}"), p)?,
        None => index.write(|c| {
            c.execute(
                "DELETE FROM kv WHERE key = ?1",
                [format!("{INSTALLED}{pack_id}")],
            )?;
            Ok(())
        })?,
    }
    Ok(())
}

/// Remove an installed pack: its Meshes go to Recently Deleted; its lists
/// stay, since they hold the person's answers.
pub fn remove(index: &std::sync::Arc<Store>, pack_id: &str) -> Result<InstalledPack> {
    let inst = installed(index)?
        .into_iter()
        .find(|i| i.id == pack_id)
        .ok_or_else(|| RiddleError::not_found(format!("pack {pack_id} is not installed")))?;
    let svc = MeshService::new(index.clone());
    for id in inst.meshes.values() {
        if svc
            .latest(id)
            .is_ok_and(|m| m.status != MeshStatus::Deleted)
        {
            svc.delete(id)?;
        }
    }
    index.write(|c| {
        c.execute(
            "DELETE FROM kv WHERE key = ?1",
            [format!("{INSTALLED}{pack_id}")],
        )?;
        Ok(())
    })?;
    Ok(inst)
}

/// Write a pack from Meshes and lists on this store, for sharing. List
/// entries are left out unless asked for: they are the person's senders.
pub fn pack_from(
    index: &Store,
    id: &str,
    name: &str,
    area: &str,
    summary: &str,
    mesh_ids: &[MeshId],
    include_entries: bool,
) -> Result<Pack> {
    let mut meshes = Vec::new();
    let mut list_names: Vec<String> = Vec::new();
    for mid in mesh_ids {
        let m = repo::meshes::latest(index, mid)?
            .ok_or_else(|| RiddleError::not_found(format!("no Mesh {mid}")))?;
        for c in m
            .routes
            .iter()
            .map(|r| &r.condition)
            .chain(m.guards.iter().map(|g| &g.condition))
        {
            crate::service::named_lists_in(c, &mut list_names);
        }
        let mut unnumbered = m.clone();
        unnumbered.priority = 0;
        let band = band_of(m.priority);
        meshes.push(PackMesh {
            text: strip_version(&crate::format::format(&unnumbered)),
            band: Some(band),
            fallback: m.fallback,
            rank: m.priority - band.range().0,
        });
    }
    let mut lists = Vec::new();
    for n in list_names {
        let def = repo::lists::get(index, &n)?;
        lists.push(PackList {
            description: def
                .as_ref()
                .map(|d| d.description.clone())
                .unwrap_or_default(),
            question: String::new(),
            default: if include_entries {
                def.map(|d| d.entries).unwrap_or_default()
            } else {
                vec![]
            },
            name: n,
        });
    }
    let pack = Pack {
        id: id.to_string(),
        name: name.to_string(),
        version: 1,
        area: area.to_string(),
        summary: summary.to_string(),
        band: Band::Specific,
        detect: PackDetect::default(),
        lists,
        meshes,
    };
    parse_pack(&to_toml(&pack)?)
}

/// Whether two Meshes decide the same way: the same conditions, outputs
/// and guards, whatever their names, Priority or formatting.
fn same_rules(a: &Mesh, b: &Mesh) -> bool {
    a.routes.len() == b.routes.len()
        && a.guards.len() == b.guards.len()
        && a.routes
            .iter()
            .zip(&b.routes)
            .all(|(x, y)| x.condition == y.condition && x.output == y.output)
        && a.guards
            .iter()
            .zip(&b.guards)
            .all(|(x, y)| x.condition == y.condition && x.never == y.never)
}

/// The band a Priority falls in.
pub fn band_of(priority: i32) -> Band {
    [
        Band::Safety,
        Band::Personal,
        Band::Specific,
        Band::General,
        Band::Fallback,
    ]
    .into_iter()
    .find(|b| {
        let (lo, hi) = b.range();
        (lo..=hi).contains(&priority)
    })
    .unwrap_or(if priority < -100 {
        Band::Safety
    } else {
        Band::Fallback
    })
}

/// The Priority for a Mesh at `rank` in the band: band start + rank when
/// free, else the next free slot after it (then before it). The Mesh being
/// replaced keeps its own slot when that is the one its rank asks for.
fn free_priority(index: &Store, band: Band, rank: i32, keep: Option<&MeshId>) -> Result<i32> {
    let (lo, hi) = band.range();
    let want = (lo + rank.max(0)).min(hi);
    let running = repo::meshes::list_running(index)?;
    if let Some(k) = keep {
        if running.iter().any(|m| &m.id == k && m.priority == want) {
            return Ok(want);
        }
    }
    let taken: Vec<i32> = running
        .iter()
        .filter(|m| Some(&m.id) != keep && m.scope == riddle_core::MeshScope::AllDigs)
        .map(|m| m.priority)
        .collect();
    (want..=hi)
        .chain((lo..want).rev())
        .find(|p| !taken.contains(p))
        .ok_or_else(|| RiddleError::invalid(format!("no free Priority left in the {band:?} band")))
}

/// The text with `Priority: n` set just under the name line.
fn with_priority(text: &str, priority: i32) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.trim_start().to_ascii_lowercase().starts_with("priority:"))
        .map(str::to_string)
        .collect();
    let at = lines
        .iter()
        .position(|l| l.trim_start().starts_with("Mesh:"))
        .map(|i| i + 1)
        .unwrap_or(0);
    lines.insert(at, format!("Priority: {priority}"));
    lines.join("\n") + "\n"
}

/// `Mesh: Name v4` → `Mesh: Name`: a pack's Meshes are versioned by the pack.
fn strip_version(text: &str) -> String {
    text.lines()
        .map(|l| match l.strip_prefix("Mesh: ") {
            Some(rest) => {
                let name = match rest.rsplit_once(" v") {
                    Some((n, v)) if v.chars().all(|c| c.is_ascii_digit()) && !v.is_empty() => n,
                    _ => rest,
                };
                format!("Mesh: {name}")
            }
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_store::StoreKind;
    use std::sync::Arc;

    fn index() -> Arc<Store> {
        let index = Arc::new(Store::in_memory(StoreKind::Index).unwrap());
        riddle_signals::Registry::new(index.clone())
            .seed_system()
            .unwrap();
        index
    }

    #[test]
    fn every_built_in_pack_parses_names_only_lists_it_declares_and_fits_its_bands() {
        let packs = builtin_packs();
        let mut ids: Vec<&str> = packs.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), packs.len(), "pack ids are unique");
        for p in &packs {
            let declared: Vec<&str> = p.lists.iter().map(|l| l.name.as_str()).collect();
            for pm in &p.meshes {
                let m = crate::parse::parse(&pm.text).unwrap();
                let mut used = Vec::new();
                for c in m
                    .routes
                    .iter()
                    .map(|r| &r.condition)
                    .chain(m.guards.iter().map(|g| &g.condition))
                {
                    crate::service::named_lists_in(c, &mut used);
                }
                for u in used {
                    assert!(
                        declared.contains(&u.as_str()),
                        "{}: {} reads list:{u} it does not declare",
                        p.id,
                        m.name
                    );
                }
            }
        }
    }

    #[test]
    fn installing_places_meshes_by_band_fills_lists_and_undo_puts_everything_back() {
        let index = index();
        let packs = builtin_packs();
        let shopping = packs.iter().find(|p| p.id == "shopping").unwrap();
        let mut answers = BTreeMap::new();
        answers.insert("keep_promotions".to_string(), vec!["Uber.com".to_string()]);
        let report = install(
            &index,
            shopping,
            &answers,
            MeshStatus::Shadow,
            false,
            CreatedBy::User,
        )
        .unwrap();
        assert!(!report.created.is_empty());
        let keep = repo::lists::get(&index, "keep_promotions")
            .unwrap()
            .unwrap();
        assert_eq!(keep.entries, vec!["uber.com"]);
        for id in &report.created {
            let m = repo::meshes::latest(&index, id).unwrap().unwrap();
            assert_eq!(m.pack.as_ref().unwrap().id, "shopping");
            let band = shopping
                .meshes
                .iter()
                .find(|pm| crate::parse::parse(&pm.text).unwrap().name == m.name)
                .unwrap()
                .band
                .unwrap_or(shopping.band);
            let (lo, hi) = band.range();
            assert!(
                (lo..=hi).contains(&m.priority),
                "{} at {} outside {band:?}",
                m.name,
                m.priority
            );
        }
        // Installing again over its own Meshes is an update, not a conflict.
        install(
            &index,
            shopping,
            &BTreeMap::new(),
            MeshStatus::Shadow,
            false,
            CreatedBy::User,
        )
        .unwrap();
        undo_install(&index, "shopping", &report).unwrap();
        assert!(repo::lists::get(&index, "keep_promotions")
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_mesh_of_the_same_name_is_refused_unless_the_pack_may_replace_it() {
        let index = index();
        let svc = MeshService::new(index.clone());
        let own = svc
            .create(
                "Mesh: Promotions\nPriority: 28\nROUTES\nIF mailing_list → Spoil",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        let shopping = builtin_packs()
            .into_iter()
            .find(|p| p.id == "shopping")
            .unwrap();
        let e = install(
            &index,
            &shopping,
            &BTreeMap::new(),
            MeshStatus::Shadow,
            false,
            CreatedBy::User,
        )
        .unwrap_err();
        assert!(e.message.contains("Promotions"), "{}", e.message);
        let report = install(
            &index,
            &shopping,
            &BTreeMap::new(),
            MeshStatus::Shadow,
            true,
            CreatedBy::User,
        )
        .unwrap();
        assert!(report.replaced.iter().any(|r| r.id == own.id));
        undo_install(&index, "shopping", &report).unwrap();
        let back = repo::meshes::latest(&index, &own.id).unwrap().unwrap();
        assert!(back.pack.is_none());
        assert!(back.routes[0].text.contains("mailing_list"));
    }

    #[test]
    fn a_pack_written_from_meshes_round_trips_without_the_persons_entries() {
        let index = index();
        repo::lists::put(
            &index,
            &NamedListDef {
                name: "banks".into(),
                description: "Your banks".into(),
                entries: vec!["rbc.com".into()],
                pack: None,
            },
        )
        .unwrap();
        let svc = MeshService::new(index.clone());
        let m = svc
            .create(
                "Mesh: My bank\nPriority: 3\nROUTES\nIF sender_domain IN list:banks → Find: Know",
                None,
                MeshStatus::Active,
                CreatedBy::User,
            )
            .unwrap();
        let pack = pack_from(
            &index,
            "my-bank",
            "My bank",
            "Money",
            "Bank mail.",
            &[m.id],
            false,
        )
        .unwrap();
        let text = to_toml(&pack).unwrap();
        let back = parse_pack(&text).unwrap();
        assert_eq!(back.lists[0].name, "banks");
        assert!(back.lists[0].default.is_empty(), "entries stay private");
        assert!(!back.meshes[0].text.contains("Priority"));
        assert_eq!(back.meshes[0].band, Some(Band::Specific));
    }

    /// The built-in packs' Meshes as they would run once installed: each at
    /// its band start plus its rank, Active.
    fn running(pack_ids: &[&str]) -> Vec<Mesh> {
        let mut out = Vec::new();
        for p in builtin_packs()
            .into_iter()
            .filter(|p| pack_ids.contains(&p.id.as_str()))
        {
            for pm in &p.meshes {
                let mut m = crate::parse::parse(&pm.text).unwrap();
                m.id = MeshId(format!("mesh_{}", m.name.replace(' ', "_")));
                m.priority = pm.band.unwrap_or(p.band).range().0 + pm.rank;
                m.status = MeshStatus::Active;
                out.push(m);
            }
        }
        out
    }

    struct Lists(Vec<&'static str>);
    impl crate::eval::ListMembership for Lists {
        fn is_member(&self, _list: riddle_core::NamedList) -> bool {
            false
        }
        fn in_named_list(&self, list: &str) -> bool {
            self.0.contains(&list)
        }
    }

    fn ev(name: &str, yes: f64) -> riddle_core::SignalEvaluation {
        let mut d = riddle_core::Distribution::new();
        d.insert("YES".into(), yes);
        d.insert("NO".into(), 1.0 - yes);
        riddle_core::SignalEvaluation {
            message_id: riddle_core::MessageId("m".into()),
            signal_name: name.into(),
            signal_version: 1,
            engine_identity: "t".into(),
            selected: if yes >= 0.5 { "YES" } else { "NO" }.into(),
            distribution: d.clone(),
            calibrated: d,
            confidence: yes.max(1.0 - yes),
            latency_ms: 0,
            input_tokens: 0,
            cached: false,
            deterministic: true,
            context_hash: None,
            request_hash: None,
            evaluated_at: chrono::Utc::now(),
        }
    }

    fn decide(
        packs: &[&str],
        facts: &riddle_core::Facts,
        evals: &[riddle_core::SignalEvaluation],
        lists: &[&'static str],
    ) -> crate::eval::MeshDecision {
        crate::eval::evaluate(&running(packs), facts, evals, &Lists(lists.to_vec()))
    }

    fn bulk(category: &str) -> riddle_core::Facts {
        riddle_core::Facts {
            sender_domain: "example.com".into(),
            gmail_category: Some(category.into()),
            list_unsubscribe: true,
            ..Default::default()
        }
    }

    /// A parking booking was a Pay Find and a hotel stay's reminder was
    /// Filed with the updates; with its dates a booking waits for its day,
    /// and one already over is Filed under Bookings.
    #[test]
    fn a_dated_booking_waits_for_its_day_and_a_past_one_is_filed() {
        let dates = riddle_core::DateSpan {
            start: "2026-10-07".parse().unwrap(),
            end: "2026-10-08".parse().unwrap(),
        };
        let booking = |over: bool| riddle_core::Facts {
            sender_domain: "telpark.com".into(),
            dated: Some(dates),
            dates_passed: over,
            ..Default::default()
        };
        // A parking booking scores as an invoice and as marketing, too.
        let evals = [
            ev("event_ticket", 0.94),
            ev("invoice", 0.87),
            ev("requires_action", 0.70),
            ev("marketing", 0.71),
        ];
        let packs = ["travel", "money", "reading-social", "core"];
        let o = decide(&packs, &booking(false), &evals, &[]).output.unwrap();
        assert_eq!(
            o.disposition,
            Some(riddle_core::Disposition::Find {
                find_type: riddle_core::FindType::Attend
            })
        );
        assert_eq!(o.expires, Some(riddle_core::Expiry::AfterDates));
        assert_eq!(o.importance, Some(riddle_core::Importance::High));
        assert!(o.strata.contains(&"Bookings".to_string()), "{:?}", o.strata);

        let past = decide(&packs, &booking(true), &evals, &[]).output.unwrap();
        assert_eq!(past.disposition, Some(riddle_core::Disposition::Filed));

        // The real Telpark bookings scored .82 and .84 as event tickets and
        // .41 as a reservation; a floor of .85 left both Unresolved.
        let real = [
            ev("event_ticket", 0.82),
            ev("travel_reservation", 0.41),
            ev("marketing", 0.50),
        ];
        let past = decide(&packs, &booking(true), &real, &[]).output.unwrap();
        assert_eq!(past.disposition, Some(riddle_core::Disposition::Filed));
        assert!(
            past.strata.contains(&"Bookings".to_string()),
            "{:?}",
            past.strata
        );

        let undated = riddle_core::Facts {
            dated: None,
            ..booking(false)
        };
        // Without dates the Bookings Mesh passes; what decides it then is
        // another Mesh's business.
        let o = decide(&packs, &undated, &evals, &[]).output;
        assert!(o.is_none_or(|o| o.expires != Some(riddle_core::Expiry::AfterDates)));
    }

    /// Dependabot's review requests were as urgent as a colleague's; with
    /// the pack's own list they are Low, and an emptied list puts them back.
    #[test]
    fn a_review_a_bot_requested_is_low_importance_unless_its_list_is_emptied() {
        let pack = builtin_packs()
            .into_iter()
            .find(|p| p.id == "developer-github")
            .unwrap();
        let bots = pack.lists.iter().find(|l| l.name == "github_bots").unwrap();
        let review = |from: &str, entries: &[String]| {
            let facts = riddle_core::Facts {
                sender: "notifications@github.com".into(),
                sender_name: from.into(),
                sender_domain: "github.com".into(),
                github_event: Some("review_requested_you".into()),
                ..Default::default()
            };
            let named = riddle_core::NamedLists::from_defs([riddle_core::NamedListDef {
                name: "github_bots".into(),
                description: String::new(),
                entries: riddle_core::NamedListDef::normalise(entries),
                pack: None,
            }]);
            let lists = crate::simulate::FactsLists(&facts, &named);
            let d = crate::eval::evaluate(&running(&["developer-github"]), &facts, &[], &lists);
            let o = d.output.unwrap();
            (o.disposition.unwrap().kind(), o.importance, o.strata)
        };
        let find = riddle_core::DispositionKind::Find;
        let eng = vec!["Engineering".to_string()];
        assert_eq!(
            review("dependabot[bot]", &bots.default),
            (find, Some(riddle_core::Importance::Low), eng.clone())
        );
        assert_eq!(
            review("Ada Lovelace", &bots.default),
            (find, None, eng.clone())
        );
        assert_eq!(review("dependabot[bot]", &[]), (find, None, eng));
    }

    #[test]
    fn every_built_in_pack_installs_on_a_fresh_store_with_the_signals_it_names() {
        let index = index();
        for p in builtin_packs() {
            install(
                &index,
                &p,
                &BTreeMap::new(),
                MeshStatus::Shadow,
                false,
                CreatedBy::User,
            )
            .unwrap_or_else(|e| panic!("{}: {}", p.id, e.message));
        }
    }

    /// Security promos and newsletters scored .81-.96 as security alerts,
    /// and the guard kept them out of Spoil though its own route had passed
    /// them over as marketing.
    #[test]
    fn a_security_themed_promotion_can_be_spoiled_and_a_real_alert_cannot() {
        let promo = [
            ev("security_alert", 0.92),
            ev("marketing", 0.93),
            ev("newsletter", 0.2),
        ];
        let d = decide(&["core", "shopping"], &bulk("promotions"), &promo, &[]);
        assert!(!d.vetoed(), "{:?}", d.vetoes);
        assert_eq!(d.disposition(), Some(&riddle_core::Disposition::Spoil));

        let core = running(&["core"]);
        let security = core
            .iter()
            .find(|m| m.name == "Security and account alerts")
            .unwrap();
        let guard = &security.guards[0].condition;
        let lists = Lists(vec![]);
        let truth = |e: &[riddle_core::SignalEvaluation]| {
            crate::eval::truth_of(guard, &bulk("updates"), e, &lists)
        };
        let newsletter = [
            ev("security_alert", 0.9),
            ev("marketing", 0.3),
            ev("newsletter", 0.95),
        ];
        assert_eq!(truth(&newsletter), crate::eval::Truth::False);
        let alert = [
            ev("security_alert", 0.95),
            ev("marketing", 0.05),
            ev("newsletter", 0.05),
        ];
        assert_eq!(truth(&alert), crate::eval::Truth::True);
    }

    #[test]
    fn an_account_to_do_is_a_do_find_and_a_terms_notice_is_filed_without_the_judge() {
        let to_do = [
            ev("account_notification", 0.93),
            ev("requires_action", 0.88),
            ev("routine", 0.6),
            ev("marketing", 0.1),
        ];
        let mut facts = bulk("updates");
        facts.subject = "Verify your developer account".into();
        let d = decide(&["core"], &facts, &to_do, &[]);
        assert_eq!(
            d.disposition(),
            Some(&riddle_core::Disposition::Find {
                find_type: riddle_core::FindType::Do
            })
        );
        assert_eq!(d.mesh_name.as_deref(), Some("Account notices"));
        // A notice that repeats (a lock's low battery, every few days) is
        // one Find: the newest from the sender on that subject replaces the
        // earlier ones, and the sender's other notices stand.
        assert!(
            d.output.as_ref().is_some_and(
                |o| o.replaces && o.series_scope() == riddle_core::SeriesScope::Subject
            ),
            "{:?}",
            d.output
        );

        facts.subject = "Updates to our Terms of Service".into();
        let d = decide(&["core"], &facts, &[], &[]);
        assert_eq!(d.disposition(), Some(&riddle_core::Disposition::Filed));
        assert_eq!(d.mesh_name.as_deref(), Some("Account notices"));
    }

    /// A recruiter "came across your profile": a claim of prior contact
    /// that, from a stranger's free mailbox, used to read as a scam.
    #[test]
    fn a_recruiter_from_a_free_mailbox_is_career_mail_not_contamination() {
        let facts = riddle_core::Facts {
            sender: "recruiter@gmail.com".into(),
            sender_domain: "gmail.com".into(),
            free_mail_sender: true,
            ..Default::default()
        };
        assert!(facts.cold_outreach());
        let evals = [ev("claims_prior_contact", 0.7), ev("recruiting", 0.93)];
        let d = decide(&["core", "career"], &facts, &evals, &[]);
        assert_eq!(d.disposition(), Some(&riddle_core::Disposition::Filed));
        assert_eq!(
            d.output.unwrap().strata,
            vec!["Career".to_string()],
            "the tag and the deciding route agree"
        );
        // The same opener about anything else is still Contamination.
        let scam = [ev("claims_prior_contact", 0.7), ev("recruiting", 0.05)];
        let d = decide(&["core", "career"], &facts, &scam, &[]);
        assert_eq!(
            d.disposition(),
            Some(&riddle_core::Disposition::Contamination)
        );
    }

    #[test]
    fn a_job_alert_is_career_before_it_is_social() {
        let mut facts = bulk("social");
        facts.sender = "jobs-noreply@linkedin.com".into();
        facts.sender_domain = "linkedin.com".into();
        facts.mailing_list = true;
        let d = decide(
            &["core", "reading-social", "career"],
            &facts,
            &[],
            &["job_boards", "social_platforms"],
        );
        assert_eq!(d.mesh_name.as_deref(), Some("Job alerts and recruiters"));
        let out = d.output.unwrap();
        assert_eq!(out.disposition, Some(riddle_core::Disposition::Filed));
        assert_eq!(out.strata, vec!["Career".to_string()]);
    }

    #[test]
    fn tickets_you_hold_deliveries_and_a_hotel_stay_each_have_a_place() {
        let packs = ["core", "money", "shopping", "reading-social", "travel"];
        let know = riddle_core::Disposition::Find {
            find_type: riddle_core::FindType::Know,
        };
        let tickets = [ev("event_ticket", 0.94), ev("marketing", 0.2)];
        let d = decide(&packs, &bulk("promotions"), &tickets, &[]);
        assert_eq!(d.mesh_name.as_deref(), Some("Event tickets"));
        assert_eq!(d.disposition(), Some(&know));

        let parcel = [ev("shipment_update", 0.95), ev("marketing", 0.1)];
        let d = decide(&packs, &bulk("updates"), &parcel, &[]);
        assert_eq!(d.mesh_name.as_deref(), Some("Deliveries"));
        assert_eq!(d.output.unwrap().strata, vec!["Purchases".to_string()]);

        let stay = [ev("travel_reservation", 0.88), ev("marketing", 0.2)];
        let d = decide(&packs, &bulk("updates"), &stay, &[]);
        assert_eq!(d.mesh_name.as_deref(), Some("Travel"));
        assert_eq!(d.disposition(), Some(&know));
    }

    #[test]
    fn a_tax_return_to_sign_is_under_documents_and_finance() {
        let mut facts = bulk("updates");
        facts.sender = "echosign@echosign.com".into();
        facts.sender_domain = "adobesign.com".into();
        facts.subject = "Signature requested on T183".into();
        let d = decide(
            &["documents", "money"],
            &facts,
            &[ev("finance_topic", 0.92)],
            &["signature_services"],
        );
        assert_eq!(d.mesh_name.as_deref(), Some("Documents and signatures"));
        assert_eq!(
            d.output.unwrap().strata,
            vec!["Documents".to_string(), "Finance".to_string()]
        );
    }
}
