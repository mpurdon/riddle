//! Meshes in the index store, every version kept.

use crate::{from_json, now_ts, to_json, ts, Store};
use riddle_core::{Mesh, MeshId, MeshStatus};
use riddle_errors::Result;
use rusqlite::{params, OptionalExtension};

pub fn upsert(store: &Store, m: &Mesh) -> Result<()> {
    store.write(|c| {
        c.execute(
            "INSERT INTO meshes (id, name, version, status, json, created_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id, version) DO UPDATE SET name = excluded.name, status = excluded.status, json = excluded.json, deleted_at = excluded.deleted_at",
            params![m.id.as_str(), m.name, m.version, status_str(m.status), to_json(m)?, ts(&m.created_at), m.deleted_at.as_ref().map(ts)],
        )?;
        Ok(())
    })
}

fn status_str(s: MeshStatus) -> &'static str {
    match s {
        MeshStatus::Draft => "draft",
        MeshStatus::Shadow => "shadow",
        MeshStatus::Active => "active",
        MeshStatus::Disabled => "disabled",
        MeshStatus::Deleted => "deleted",
    }
}

pub fn get(store: &Store, id: &MeshId, version: u32) -> Result<Option<Mesh>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM meshes WHERE id = ?1 AND version = ?2",
            params![id.as_str(), version],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

/// The newest version of a Mesh regardless of status.
pub fn latest(store: &Store, id: &MeshId) -> Result<Option<Mesh>> {
    store.read(|c| {
        c.query_row(
            "SELECT json FROM meshes WHERE id = ?1 ORDER BY version DESC LIMIT 1",
            [id.as_str()],
            |r| from_json(r.get::<_, String>(0)?),
        )
        .optional()
    })
}

pub fn versions(store: &Store, id: &MeshId) -> Result<Vec<Mesh>> {
    store.read(|c| {
        let mut st = c.prepare("SELECT json FROM meshes WHERE id = ?1 ORDER BY version")?;
        let rows = st
            .query_map([id.as_str()], |r| from_json(r.get::<_, String>(0)?))?
            .collect();
        rows
    })
}

/// The newest version of every Mesh, excluding deleted ones unless asked.
pub fn list_latest(store: &Store, include_deleted: bool) -> Result<Vec<Mesh>> {
    store.read(|c| {
        let mut st = c.prepare(
            "SELECT json FROM meshes m WHERE version = (SELECT MAX(version) FROM meshes WHERE id = m.id) ORDER BY name",
        )?;
        let all: Vec<Mesh> = st.query_map([], |r| from_json(r.get::<_, String>(0)?))?.collect::<rusqlite::Result<_>>()?;
        Ok(all.into_iter().filter(|m| include_deleted || m.status != MeshStatus::Deleted).collect())
    })
}

/// The running Meshes and the versions that decide mail, from one read.
/// Running: per id, the latest version when Active or Shadow. Live: the
/// latest when Active; when the latest is in Shadow, the newest earlier
/// Active version. Changing an active Mesh put its new version in Shadow and
/// the whole Mesh stopped deciding: a Payments fix left a day's notices with
/// no decision (13 Oct 2026). Only the chosen rows' JSON is parsed; both
/// lists are in name order.
pub fn running_and_live(store: &Store) -> Result<(Vec<Mesh>, Vec<Mesh>)> {
    let rows: Vec<(String, String, String)> = store.read(|c| {
        let mut st = c.prepare("SELECT id, status, json FROM meshes ORDER BY id, version DESC")?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect();
        rows
    })?;
    let (mut running, mut live) = (Vec::new(), Vec::new());
    for versions in rows.chunk_by(|a, b| a.0 == b.0) {
        let (_, status, json) = &versions[0];
        match status.as_str() {
            "active" => {
                let m: Mesh = serde_json::from_str(json)?;
                running.push(m.clone());
                live.push(m);
            }
            "shadow" => {
                running.push(serde_json::from_str(json)?);
                if let Some((_, _, older)) = versions[1..].iter().find(|(_, s, _)| s == "active") {
                    live.push(serde_json::from_str(older)?);
                }
            }
            _ => {}
        }
    }
    running.sort_by(|a: &Mesh, b| a.name.cmp(&b.name));
    live.sort_by(|a: &Mesh, b| a.name.cmp(&b.name));
    Ok((running, live))
}

/// The Mesh versions that decide mail (`running_and_live`).
pub fn list_live(store: &Store) -> Result<Vec<Mesh>> {
    Ok(running_and_live(store)?.1)
}

/// The Meshes that run in production: latest version per id with status
/// Active or Shadow.
pub fn list_running(store: &Store) -> Result<Vec<Mesh>> {
    Ok(list_latest(store, false)?
        .into_iter()
        .filter(|m| matches!(m.status, MeshStatus::Active | MeshStatus::Shadow))
        .collect())
}

pub fn find_by_name(store: &Store, name: &str) -> Result<Option<Mesh>> {
    Ok(list_latest(store, false)?
        .into_iter()
        .find(|m| m.name.eq_ignore_ascii_case(name)))
}

pub fn next_version(store: &Store, id: &MeshId) -> Result<u32> {
    store.read(|c| {
        let v: Option<u32> = c.query_row(
            "SELECT MAX(version) FROM meshes WHERE id = ?1",
            [id.as_str()],
            |r| r.get(0),
        )?;
        Ok(v.unwrap_or(0) + 1)
    })
}

pub fn set_status(store: &Store, id: &MeshId, version: u32, status: MeshStatus) -> Result<()> {
    let Some(mut m) = get(store, id, version)? else {
        return Err(riddle_errors::RiddleError::not_found("mesh version"));
    };
    m.status = status;
    // A status the user chose is not a demotion.
    m.demoted = None;
    if status == MeshStatus::Deleted {
        m.deleted_at = Some(chrono::Utc::now());
    }
    if status == MeshStatus::Shadow && m.shadow_started_at.is_none() {
        m.shadow_started_at = Some(chrono::Utc::now());
    }
    upsert(store, &m)
}

pub fn purge(store: &Store, id: &MeshId) -> Result<()> {
    store.write(|c| {
        c.execute(
            "DELETE FROM meshes WHERE id = ?1 AND deleted_at IS NOT NULL AND deleted_at < ?2",
            params![id.as_str(), now_ts()],
        )?;
        Ok(())
    })
}

/// Remove one version row. Used by undo of `update_mesh`, which reverts the
/// version it created; every other removal is a soft delete.
pub fn remove_version(store: &Store, id: &MeshId, version: u32) -> Result<bool> {
    store.write(|c| {
        Ok(c.execute(
            "DELETE FROM meshes WHERE id = ?1 AND version = ?2",
            params![id.as_str(), version],
        )? > 0)
    })
}

/// Step a version back to Shadow on evidence and record why.
pub fn demote(
    store: &Store,
    id: &MeshId,
    version: u32,
    demotion: riddle_core::Demotion,
) -> Result<()> {
    let Some(mut m) = get(store, id, version)? else {
        return Err(riddle_errors::RiddleError::not_found("mesh version"));
    };
    m.status = MeshStatus::Shadow;
    if m.shadow_started_at.is_none() {
        m.shadow_started_at = Some(demotion.at);
    }
    m.demoted = Some(demotion);
    upsert(store, &m)
}
