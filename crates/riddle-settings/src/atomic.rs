//! Whole-or-nothing file writes: a private temp file beside the target, then
//! a rename. A crash between the two leaves the previous file intact, which
//! is the incident this exists for (a truncated settings file on launch
//! resets every preference). Files are created 0600 because settings and
//! state may name profiles, parameters and accounts (§58).

use riddle_errors::{Result, RiddleError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Stage one: write the bytes to a private temp file next to `path`.
/// Exposed so the crash test can stop here.
pub fn write_temp(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| RiddleError::internal(format!("create {}: {e}", parent.display())))?;
    }
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!(".{name}.{}.{seq}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&tmp)
        .map_err(|e| RiddleError::internal(format!("open {}: {e}", tmp.display())))?;
    use std::io::Write;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            RiddleError::internal(format!("write {}: {e}", tmp.display()))
        })?;
    Ok(tmp)
}

/// Stage two: move the temp file into place.
pub fn commit(tmp: &Path, path: &Path) -> Result<()> {
    std::fs::rename(tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(tmp);
        RiddleError::internal(format!("rename into {}: {e}", path.display()))
    })
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = write_temp(path, bytes)?;
    commit(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_replaces_the_file_whole_and_keeps_it_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nested").join("f.toml");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let leftovers = std::fs::read_dir(p.parent().unwrap()).unwrap().count();
        assert_eq!(leftovers, 1, "no temp files remain");
    }
}
