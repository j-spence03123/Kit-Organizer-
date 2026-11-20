//! Low-level filesystem primitives used by the organiser.

use crate::error::{CoreError, CoreResult, IoContext};
use crate::hashing::hash_file;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

/// `Kick 01.wav` → `Kick 01 (n).wav`.
pub fn numbered(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_string();
    }
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{} ({n}){}", &name[..i], &name[i..]),
        _ => format!("{name} ({n})"),
    }
}

/// Case-insensitive key so `kick.wav` and `Kick.wav` collide (they would on
/// macOS and Windows).
pub fn path_key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// First free name in `dir` not on disk and not already claimed.
pub fn unique_path(dir: &Path, name: &str, claimed: &HashSet<String>) -> (PathBuf, bool) {
    for n in 1..100_000 {
        let candidate = dir.join(numbered(name, n));
        if !claimed.contains(&path_key(&candidate)) && !exists_no_follow(&candidate) {
            return (candidate, n > 1);
        }
    }
    (dir.join(format!("{}-{}", uuid::Uuid::new_v4(), name)), true)
}

pub fn exists_no_follow(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

/// Create `dir` and any missing parents, returning the directories that
/// were actually created (deepest last) so undo can remove them if empty.
pub fn create_dirs(dir: &Path) -> CoreResult<Vec<PathBuf>> {
    let mut missing = Vec::new();
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if d.exists() {
            break;
        }
        missing.push(d.to_path_buf());
        cur = d.parent();
    }
    fs::create_dir_all(dir).at(dir)?;
    missing.reverse();
    Ok(missing)
}

/// Give `temp` the name `desired` without ever replacing an existing file.
/// Returns the final path, which differs from `desired` if it was taken.
pub fn claim_name(temp: &Path, desired: &Path) -> CoreResult<PathBuf> {
    let dir = desired.parent().ok_or_else(|| CoreError::InvalidPath(desired.display().to_string()))?;
    let name = desired.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let empty = HashSet::new();
    let mut target = desired.to_path_buf();
    for _ in 0..1000 {
        match fs::hard_link(temp, &target) {
            Ok(()) => {
                fs::remove_file(temp).at(temp)?;
                return Ok(target);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                target = unique_path(dir, &name, &empty).0;
            }
            Err(_) => {
                // Filesystem without hard links (FAT/exFAT): check, then rename.
                if exists_no_follow(&target) {
                    target = unique_path(dir, &name, &empty).0;
                    continue;
                }
                fs::rename(temp, &target).at(&target)?;
                return Ok(target);
            }
        }
    }
    Err(CoreError::Other(format!("could not find a free name for {}", desired.display())))
}

pub fn temp_path_for(desired: &Path) -> PathBuf {
    let dir = desired.parent().unwrap_or(Path::new("."));
    let short = &uuid::Uuid::new_v4().simple().to_string()[..12];
    dir.join(format!(".kitorg-{short}.part"))
}

/// Copy `src` to a temp file next to `desired`, fsync, verify the hash,
/// then claim the final name. Returns the final path.
pub fn copy_verified(src: &Path, desired: &Path, expected_hash: Option<&str>, temp: &Path) -> CoreResult<PathBuf> {
    let result = (|| {
        {
            let mut input = File::open(src).at(src)?;
            let mut out = File::create_new(temp).at(temp)?;
            io::copy(&mut input, &mut out).at(temp)?;
            out.sync_all().at(temp)?;
        }
        if let Ok(m) = fs::metadata(src) {
            if let Ok(perm) = m.modified() {
                let _ = File::options().write(true).open(temp).and_then(|f| f.set_modified(perm));
            }
        }
        let written = hash_file(temp)?;
        if let Some(exp) = expected_hash {
            if written != exp {
                return Err(CoreError::Other(format!(
                    "verification failed for {}: the source changed since it was scanned or the copy is corrupt",
                    src.display()
                )));
            }
        }
        claim_name(temp, desired)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

/// Rename `src` to `desired` without replacing anything (same volume).
pub fn move_no_replace(src: &Path, desired: &Path) -> CoreResult<PathBuf> {
    let dir = desired.parent().ok_or_else(|| CoreError::InvalidPath(desired.display().to_string()))?;
    let name = desired.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let empty = HashSet::new();
    let mut target = desired.to_path_buf();
    for _ in 0..1000 {
        match fs::hard_link(src, &target) {
            Ok(()) => {
                // Both names now point at the data; dropping the old one
                // completes the move. A crash here leaves both (no loss).
                fs::remove_file(src).at(src)?;
                return Ok(target);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => target = unique_path(dir, &name, &empty).0,
            Err(_) => {
                if exists_no_follow(&target) {
                    target = unique_path(dir, &name, &empty).0;
                    continue;
                }
                fs::rename(src, &target).at(&target)?;
                return Ok(target);
            }
        }
    }
    Err(CoreError::Other(format!("could not find a free name for {}", desired.display())))
}

/// Best-effort check whether two paths are on the same volume.
pub fn same_volume(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let ma = fs::metadata(a);
        let mb = existing_ancestor(b).and_then(|p| fs::metadata(p).ok());
        match (ma, mb) {
            (Ok(x), Some(y)) => x.dev() == y.dev(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let root = |p: &Path| p.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
        root(a) == root(b)
    }
}

pub fn existing_ancestor(p: &Path) -> Option<PathBuf> {
    let mut cur = Some(p);
    while let Some(c) = cur {
        if c.exists() {
            return Some(c.to_path_buf());
        }
        cur = c.parent();
    }
    None
}

pub fn available_space(p: &Path) -> Option<u64> {
    existing_ancestor(p).and_then(|a| fs4::available_space(a).ok())
}

/// Remove directories (deepest first) if they are empty and inside `root`.
pub fn remove_empty_dirs(dirs: &[PathBuf], root: &Path) {
    let mut sorted: Vec<&PathBuf> = dirs.iter().filter(|d| d.starts_with(root) && d.as_path() != root).collect();
    sorted.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    sorted.dedup();
    for d in sorted {
        let _ = fs::remove_dir(d); // fails harmlessly when not empty
    }
}
