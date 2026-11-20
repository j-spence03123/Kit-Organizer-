//! Executes a plan with journaling, verification, progress and
//! cancellation; reconciles interrupted runs.

use super::fsops::*;
use super::plan::{Plan, PlannedAction};
use crate::db::{now, Database, OperationRow};
use crate::error::{CoreError, CoreResult};
use crate::hashing::hash_file;
use crate::models::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecProgress {
    pub processed: u64,
    pub total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecReport {
    pub copied: u64,
    pub moved: u64,
    pub skipped: u64,
    pub failed: u64,
    pub renamed: u64,
    pub cancelled: bool,
    pub errors: Vec<FsIssue>,
    pub warnings: Vec<String>,
    pub library_root: String,
}

const SPACE_MARGIN: u64 = 64 * 1024 * 1024;

fn lock(db: &Mutex<Database>) -> std::sync::MutexGuard<'_, Database> {
    db.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn execute(
    db: &Mutex<Database>,
    import_id: &str,
    plan: &Plan,
    cancel: &AtomicBool,
    progress: &dyn Fn(&ExecProgress),
) -> CoreResult<ExecReport> {
    if plan.summary.undecided_duplicates > 0 {
        return Err(CoreError::Other(format!(
            "{} duplicate file(s) need a keep/skip decision before organising",
            plan.summary.undecided_duplicates
        )));
    }
    let root = PathBuf::from(&plan.library_root);
    let transfers: Vec<_> = plan.ops.iter().filter(|o| o.action != PlannedAction::Skip).collect();

    // Disk space: copies need the full size; moves only across volumes.
    let needed: u64 = transfers
        .iter()
        .filter(|o| o.action == PlannedAction::Copy || !same_volume(Path::new(&o.src), &root))
        .map(|o| o.size)
        .sum();
    if needed > 0 {
        if let Some(avail) = available_space(&root) {
            if needed + SPACE_MARGIN > avail {
                return Err(CoreError::InsufficientSpace { needed: needed + SPACE_MARGIN, available: avail });
            }
        }
    }

    let mut import = lock(db).get_import(import_id)?.ok_or_else(|| CoreError::Other("import not found".into()))?;
    import.status = ImportStatus::Organising;
    import.mode = plan.mode;
    import.library_root = plan.library_root.clone();
    lock(db).update_import(&import)?;

    // Journal every transfer before touching the filesystem.
    let rows: Vec<OperationRow> = transfers
        .iter()
        .map(|o| OperationRow {
            id: 0,
            import_id: import_id.to_string(),
            sample_id: o.sample_id.clone(),
            op: if o.action == PlannedAction::Move { "move".into() } else { "copy".into() },
            src: o.src.clone(),
            dest: o.dest_path.clone(),
            temp: None,
            hash: o.hash.clone(),
            created_dirs: vec![],
            state: "planned".into(),
            error: None,
        })
        .collect();
    let ids = lock(db).insert_operations(&rows)?;

    let mut rep = ExecReport { library_root: plan.library_root.clone(), ..Default::default() };
    // Skips first: they need no filesystem work.
    for o in plan.ops.iter().filter(|o| o.action == PlannedAction::Skip) {
        if let Some(st) = o.skip_status {
            lock(db).set_sample_result(&o.sample_id, st, None, o.reason.as_deref())?;
            if st == SampleStatus::Skipped {
                rep.skipped += 1;
            } else {
                rep.failed += 1;
                rep.errors.push(FsIssue { path: o.src.clone(), message: o.reason.clone().unwrap_or_default() });
            }
        }
    }

    let total = transfers.len() as u64;
    let bytes_total: u64 = transfers.iter().map(|o| o.size).sum();
    let mut bytes_done = 0u64;
    let mut last = Instant::now() - Duration::from_secs(1);
    for (i, (o, mut row)) in transfers.iter().zip(rows).enumerate() {
        row.id = ids[i];
        if cancel.load(Ordering::Relaxed) {
            rep.cancelled = true;
            row.state = "cancelled".into();
            lock(db).update_operation(&row)?;
            continue;
        }
        if last.elapsed() > Duration::from_millis(100) {
            last = Instant::now();
            progress(&ExecProgress { processed: i as u64, total, bytes_done, bytes_total, current: o.dest_name.clone() });
        }
        match run_one(db, o, &mut row) {
            Ok(final_path) => {
                if final_path != Path::new(&o.dest_path) || o.renamed_from.is_some() {
                    rep.renamed += 1;
                }
                if row.op == "move" {
                    rep.moved += 1;
                } else {
                    rep.copied += 1;
                }
                if let Some(w) = &row.error {
                    rep.warnings.push(w.clone());
                }
            }
            Err(e) => {
                rep.failed += 1;
                rep.errors.push(FsIssue { path: o.src.clone(), message: e.to_string() });
                row.state = "failed".into();
                row.error = Some(e.to_string());
                let d = lock(db);
                d.update_operation(&row)?;
                d.set_sample_result(&o.sample_id, SampleStatus::Failed, None, Some(&e.to_string()))?;
            }
        }
        bytes_done += o.size;
    }
    progress(&ExecProgress { processed: total, total, bytes_done, bytes_total, current: String::new() });

    import.status = if rep.cancelled { ImportStatus::Cancelled } else { ImportStatus::Completed };
    import.completed_at = Some(now());
    let st = &mut import.stats;
    st.processed += rep.copied + rep.moved + rep.skipped + rep.failed;
    st.copied += rep.copied;
    st.moved += rep.moved;
    st.skipped += rep.skipped;
    st.errors += rep.failed;
    st.renamed += rep.renamed;
    import.messages.extend(rep.warnings.iter().cloned());
    lock(db).update_import(&import)?;
    Ok(rep)
}

fn run_one(db: &Mutex<Database>, o: &super::plan::PlannedOp, row: &mut OperationRow) -> CoreResult<PathBuf> {
    let src = Path::new(&o.src);
    let desired = PathBuf::from(&o.dest_path);
    let dir = desired.parent().ok_or_else(|| CoreError::InvalidPath(o.dest_path.clone()))?;
    if !src.is_file() {
        return Err(CoreError::Other("source file is missing".into()));
    }
    row.created_dirs = create_dirs(dir)?.iter().map(|p| p.display().to_string()).collect();
    let temp = temp_path_for(&desired);
    row.temp = Some(temp.display().to_string());
    row.state = "in_progress".into();
    lock(db).update_operation(row)?;

    let final_path = if row.op == "move" && same_volume(src, dir) {
        let p = move_no_replace(src, &desired)?;
        let ok = std::fs::metadata(&p).map(|m| m.len() == o.size).unwrap_or(false);
        if !ok {
            return Err(CoreError::Other(format!("moved file at {} failed size verification", p.display())));
        }
        p
    } else {
        let p = copy_verified(src, &desired, o.hash.as_deref(), &temp)?;
        if row.op == "move" {
            // Destination verified; only now remove the source.
            if let Err(e) = std::fs::remove_file(src) {
                row.error = Some(format!("copied {} but could not remove the original: {e}", src.display()));
            }
        }
        p
    };
    row.dest = final_path.display().to_string();
    row.temp = None;
    row.state = "done".into();
    let d = lock(db);
    d.update_operation(row)?;
    d.set_sample_result(&o.sample_id, SampleStatus::Organised, Some(&row.dest), None)?;
    Ok(final_path)
}

/// Reconcile runs interrupted by a crash or power loss. Leftover temp files
/// are deleted (they are ours and incomplete); operations whose destination
/// exists with the expected content are marked done; everything else is
/// returned to pending. Returns human-readable messages.
pub fn recover(db: &Mutex<Database>) -> CoreResult<Vec<String>> {
    let mut messages = Vec::new();
    let interrupted = lock(db).imports_with_status(ImportStatus::Organising)?;
    for mut imp in interrupted {
        let ops = lock(db).operations_for_import(&imp.id)?;
        let (mut done, mut reset) = (0u64, 0u64);
        for mut op in ops.into_iter().filter(|o| o.state == "planned" || o.state == "in_progress") {
            if let Some(t) = &op.temp {
                let _ = std::fs::remove_file(t);
            }
            let dest = Path::new(&op.dest);
            let verified = op.state == "in_progress"
                && dest.is_file()
                && match (&op.hash, hash_file(dest)) {
                    (Some(h), Ok(got)) => *h == got,
                    _ => false,
                };
            let d = lock(db);
            if verified {
                op.state = "done".into();
                if op.op == "move" && Path::new(&op.src).exists() {
                    op.error = Some("recovered after interruption; the original was kept".into());
                }
                op.temp = None;
                d.update_operation(&op)?;
                d.set_sample_result(&op.sample_id, SampleStatus::Organised, Some(&op.dest), None)?;
                done += 1;
                if op.op == "move" {
                    imp.stats.moved += 1;
                } else {
                    imp.stats.copied += 1;
                }
            } else {
                op.state = "cancelled".into();
                op.temp = None;
                d.update_operation(&op)?;
                reset += 1;
            }
        }
        imp.status = ImportStatus::Interrupted;
        let msg = format!(
            "Organising was interrupted. {done} file(s) were verified as complete; {reset} file(s) were not processed and remain pending."
        );
        imp.messages.push(msg.clone());
        lock(db).update_import(&imp)?;
        messages.push(msg);
    }
    Ok(messages)
}
