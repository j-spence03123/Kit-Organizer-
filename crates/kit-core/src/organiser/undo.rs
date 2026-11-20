//! Safe undo of a completed organisation run.

use super::fsops::*;
use crate::db::{Database, OperationRow};
use crate::error::{CoreError, CoreResult};
use crate::hashing::hash_file;
use crate::models::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    pub reverted: u64,
    /// Files that could not be safely reverted, with the reason.
    pub skipped: Vec<FsIssue>,
}

fn lock(db: &Mutex<Database>) -> std::sync::MutexGuard<'_, Database> {
    db.lock().unwrap_or_else(|p| p.into_inner())
}

/// Revert an import:
/// * copies are deleted only if the file still has the content we wrote;
/// * moves are put back only if the original location is free;
/// * nothing is ever overwritten; anything unsafe is reported instead.
pub fn undo_import(db: &Mutex<Database>, import_id: &str) -> CoreResult<UndoReport> {
    let mut imp = lock(db).get_import(import_id)?.ok_or_else(|| CoreError::Other("import not found".into()))?;
    if imp.status == ImportStatus::Organising {
        return Err(CoreError::Other("this import is still being organised".into()));
    }
    let ops = lock(db).operations_for_import(import_id)?;
    let mut rep = UndoReport::default();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let root = PathBuf::from(&imp.library_root);
    for mut op in ops.into_iter().filter(|o| o.state == "done") {
        match undo_one(&op) {
            Ok(()) => {
                op.state = "reverted".into();
                let d = lock(db);
                d.update_operation(&op)?;
                d.set_sample_result(&op.sample_id, SampleStatus::Undone, None, None)?;
                rep.reverted += 1;
                dirs.extend(op.created_dirs.iter().map(PathBuf::from));
                if let Some(p) = Path::new(&op.dest).parent() {
                    dirs.push(p.to_path_buf());
                }
            }
            Err(reason) => {
                op.error = Some(reason.clone());
                lock(db).update_operation(&op)?;
                rep.skipped.push(FsIssue { path: op.dest.clone(), message: reason });
            }
        }
    }
    remove_empty_dirs(&dirs, &root);
    if root.exists() && std::fs::read_dir(&root).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(&root);
    }
    imp.stats.reverted += rep.reverted;
    imp.status = if rep.skipped.is_empty() { ImportStatus::Undone } else { ImportStatus::PartiallyUndone };
    imp.messages.push(format!(
        "Undo: {} file(s) reverted, {} could not be safely reverted.",
        rep.reverted,
        rep.skipped.len()
    ));
    lock(db).update_import(&imp)?;
    Ok(rep)
}

fn undo_one(op: &OperationRow) -> Result<(), String> {
    let dest = Path::new(&op.dest);
    if !dest.is_file() {
        return Err("the organised file no longer exists".into());
    }
    if let Some(h) = &op.hash {
        match hash_file(dest) {
            Ok(got) if &got == h => {}
            Ok(_) => return Err("the file was modified after organising, so it was left in place".into()),
            Err(e) => return Err(format!("could not verify the file: {e}")),
        }
    }
    if op.op == "copy" {
        let src = Path::new(&op.src);
        if !src.exists() {
            // The copy is now the only copy; deleting it would lose data.
            return Err("the original file is gone, so the copy was kept to avoid data loss".into());
        }
        std::fs::remove_file(dest).map_err(|e| format!("could not delete: {e}"))?;
        return Ok(());
    }
    // Move: restore to the original path.
    let src = PathBuf::from(&op.src);
    if exists_no_follow(&src) {
        return Err(format!("{} already exists, so the file was not moved back", src.display()));
    }
    if let Some(parent) = src.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot recreate {}: {e}", parent.display()))?;
    }
    let restored = if same_volume(dest, src.parent().unwrap_or(Path::new("/"))) {
        move_no_replace(dest, &src).map_err(|e| e.to_string())?
    } else {
        let temp = temp_path_for(&src);
        let p = copy_verified(dest, &src, op.hash.as_deref(), &temp).map_err(|e| e.to_string())?;
        std::fs::remove_file(dest).map_err(|e| format!("restored a copy but could not remove the organised file: {e}"))?;
        p
    };
    // `restored` only differs from `src` if something claimed the name in
    // the instant after our check; it was still not overwritten.
    let _ = restored;
    Ok(())
}
