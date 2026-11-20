//! Recursive, cancellable directory scanning.
//!
//! The scanner only walks directories and stats files; it never reads file
//! contents. It does not follow symlinks (avoiding cycles), skips hidden
//! directories and `__MACOSX`, never descends into the output library, and
//! records unreadable directories as issues instead of failing.

use crate::error::{CoreError, CoreResult};
use crate::filetypes::{FileKind, FileTypeSettings};
use crate::models::FsIssue;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    /// The imported folder this file came from.
    pub root: PathBuf,
    /// Folder names from the imported root (inclusive) to the parent.
    pub folders: Vec<String>,
    pub file_name: String,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub files: Vec<ScannedFile>,
    pub total: u64,
    pub unsupported: u64,
    pub ignored: u64,
    pub issues: Vec<FsIssue>,
    /// A sample of unsupported file paths, for display.
    pub unsupported_examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub discovered: u64,
    pub supported: u64,
    pub unsupported: u64,
    pub processed: u64,
    pub total: u64,
    pub current: String,
}

pub struct ScanOptions<'a> {
    pub roots: &'a [PathBuf],
    /// Directories never to descend into (the output library).
    pub exclude: &'a [PathBuf],
    pub file_types: &'a FileTypeSettings,
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Validate and normalise import roots: must exist, be absolute; nested
/// roots are collapsed into their ancestor so no file is scanned twice.
pub fn normalise_roots(roots: &[PathBuf], exclude: &[PathBuf]) -> CoreResult<Vec<PathBuf>> {
    let excl: Vec<PathBuf> = exclude.iter().map(|p| canonical(p)).collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !r.is_absolute() {
            return Err(CoreError::InvalidPath(format!("{} is not an absolute path", r.display())));
        }
        if !r.exists() {
            return Err(CoreError::InvalidPath(format!("{} does not exist", r.display())));
        }
        let c = canonical(r);
        if let Some(e) = excl.iter().find(|e| c.starts_with(e)) {
            return Err(CoreError::InvalidPath(format!(
                "{} is inside the output library ({}); choose a different output folder",
                r.display(),
                e.display()
            )));
        }
        out.push(c);
    }
    out.sort();
    out.dedup();
    let mut collapsed: Vec<PathBuf> = Vec::new();
    for r in out {
        if !collapsed.iter().any(|c| r.starts_with(c)) {
            collapsed.push(r);
        }
    }
    Ok(collapsed)
}

pub fn scan(opts: &ScanOptions, cancel: &AtomicBool, progress: &dyn Fn(&ScanProgress)) -> CoreResult<ScanReport> {
    let roots = normalise_roots(opts.roots, opts.exclude)?;
    let exclude: Vec<PathBuf> = opts.exclude.iter().map(|p| canonical(p)).collect();
    let mut rep = ScanReport::default();
    let mut last = Instant::now();
    let emit = |rep: &ScanReport, current: &Path, force: bool, last: &mut Instant| {
        if force || last.elapsed() > Duration::from_millis(120) {
            *last = Instant::now();
            progress(&ScanProgress {
                phase: "scanning".into(),
                discovered: rep.total,
                supported: rep.files.len() as u64,
                unsupported: rep.unsupported,
                processed: 0,
                total: 0,
                current: current.display().to_string(),
            });
        }
    };

    for root in &roots {
        if root.is_file() {
            // A single dropped file: its parent acts as the root folder.
            let parent = root.parent().map(Path::to_path_buf).unwrap_or_default();
            visit_file(root, &parent, opts.file_types, &mut rep);
            continue;
        }
        let root_name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let walker = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                if name.starts_with('.') || name == "__MACOSX" || name == "$RECYCLE.BIN" {
                    return false;
                }
                if exclude.iter().any(|x| e.path().starts_with(x)) {
                    return false;
                }
            }
            true
        });
        for entry in walker {
            if cancel.load(Ordering::Relaxed) {
                return Err(CoreError::Cancelled);
            }
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    let path = err.path().map(|p| p.display().to_string()).unwrap_or_default();
                    rep.issues.push(FsIssue { path, message: err.to_string() });
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let _ = &root_name;
            visit_file(entry.path(), root, opts.file_types, &mut rep);
            emit(&rep, entry.path(), false, &mut last);
        }
    }
    emit(&rep, Path::new(""), true, &mut last);
    Ok(rep)
}

fn visit_file(path: &Path, root: &Path, types: &FileTypeSettings, rep: &mut ScanReport) {
    rep.total += 1;
    let kind = types.detect(path);
    match kind {
        FileKind::Ignored => {
            rep.ignored += 1;
            return;
        }
        FileKind::Unsupported => {
            rep.unsupported += 1;
            if rep.unsupported_examples.len() < 50 {
                rep.unsupported_examples.push(path.display().to_string());
            }
            return;
        }
        _ => {}
    }
    let size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            rep.issues.push(FsIssue { path: path.display().to_string(), message: e.to_string() });
            return;
        }
    };
    let mut folders = Vec::new();
    if let Some(name) = root.file_name() {
        folders.push(name.to_string_lossy().to_string());
    }
    if let Some(parent) = path.parent() {
        if let Ok(rel) = parent.strip_prefix(root) {
            folders.extend(rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()));
        }
    }
    rep.files.push(ScannedFile {
        path: path.to_path_buf(),
        root: root.to_path_buf(),
        folders,
        file_name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        kind,
        size,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn scans_recursively_and_counts() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Pack");
        touch(&root.join("Drums/One Shots/Kicks/Kick_01.wav"));
        touch(&root.join("Drums/Snare.aiff"));
        touch(&root.join("a/b/c/d/e/f/g/deep.flac"));
        touch(&root.join("MIDI/chords.mid"));
        touch(&root.join("readme.txt"));
        touch(&root.join("Drums/._Kick_01.wav"));
        touch(&root.join("__MACOSX/Drums/._x.wav"));
        touch(&root.join(".hidden/x.wav"));
        let types = FileTypeSettings::default();
        let rep = scan(
            &ScanOptions { roots: &[root.clone()], exclude: &[], file_types: &types },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
        assert_eq!(rep.files.len(), 4);
        assert_eq!(rep.unsupported, 1);
        assert_eq!(rep.ignored, 1);
        let kick = rep.files.iter().find(|f| f.file_name == "Kick_01.wav").unwrap();
        assert_eq!(kick.folders, ["Pack", "Drums", "One Shots", "Kicks"]);
    }

    #[test]
    fn skips_output_directory() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Pack");
        touch(&root.join("Kick.wav"));
        touch(&root.join("Organised Samples/Kicks/Kick.wav"));
        let types = FileTypeSettings::default();
        let out = root.join("Organised Samples");
        let rep = scan(&ScanOptions { roots: &[root], exclude: &[out], file_types: &types }, &AtomicBool::new(false), &|_| {})
            .unwrap();
        assert_eq!(rep.files.len(), 1);
    }

    #[test]
    fn rejects_root_inside_output() {
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("Organised Samples");
        touch(&out.join("Kicks/k.wav"));
        assert!(normalise_roots(&[out.join("Kicks")], &[out.clone()]).is_err());
    }

    #[test]
    fn nested_roots_are_collapsed() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Pack");
        touch(&root.join("Sub/Kick.wav"));
        let roots = normalise_roots(&[root.join("Sub"), root.clone()], &[]).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn cancellation() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Pack");
        touch(&root.join("Kick.wav"));
        let types = FileTypeSettings::default();
        let r = scan(&ScanOptions { roots: &[root], exclude: &[], file_types: &types }, &AtomicBool::new(true), &|_| {});
        assert!(matches!(r, Err(CoreError::Cancelled)));
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_directory_is_reported_not_fatal() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Pack");
        touch(&root.join("ok/Kick.wav"));
        touch(&root.join("locked/Snare.wav"));
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let types = FileTypeSettings::default();
        let rep = scan(&ScanOptions { roots: &[root.clone()], exclude: &[], file_types: &types }, &AtomicBool::new(false), &|_| {})
            .unwrap();
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        // Running as root bypasses permissions; only assert when it applied.
        if rep.files.len() == 1 {
            assert_eq!(rep.issues.len(), 1);
        }
        assert!(rep.files.iter().any(|f| f.file_name == "Kick.wav"));
    }
}
