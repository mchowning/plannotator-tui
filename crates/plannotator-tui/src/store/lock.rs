//! The record's lock and its atomic replacement.
//!
//! Every writer takes an exclusive advisory lock on `annotations.json.lock`, next to the
//! record, for the whole read-modify-write. The review UI and the `thread` CLI run in
//! different processes, so a lock held only around the write would still let one of them
//! overwrite the other's change with a stale copy.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

/// Held for one read-modify-write; the lock is released when this is dropped.
#[derive(Debug)]
pub(super) struct Guard {
    _file: File,
}

fn lock_path(record: &Path) -> PathBuf {
    let mut name = record.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
    name.push(".lock");
    record.with_file_name(name)
}

/// Block until this process holds the record's lock.
pub(super) fn acquire(record: &Path) -> Result<Guard> {
    if let Some(dir) = record.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let path = lock_path(record);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    file.lock().with_context(|| format!("locking {}", path.display()))?;
    Ok(Guard { _file: file })
}

/// Replace `path` with `contents` through a temp file only this write uses, so a reader
/// never sees a partial record and a writer without the lock never shares our temp file.
pub(super) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tmp = path.with_extension(format!("json.{}.{nanos:x}.tmp", std::process::id()));
    std::fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}
