//! Builds the complete, reviewable organisation plan. Planning never
//! modifies the filesystem.

use super::fsops::{path_key, unique_path};
use crate::error::{CoreError, CoreResult, IoContext};
use crate::models::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlannedAction {
    Copy,
    Move,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedOp {
    pub sample_id: String,
    pub src: String,
    pub dest_dir: String,
    pub dest_name: String,
    pub dest_path: String,
    pub action: PlannedAction,
    /// Category the file will actually be placed in (may differ from the
    /// prediction when low-confidence files are rerouted).
    pub category_id: String,
    pub category_path: String,
    pub hash: Option<String>,
    pub size: u64,
    /// Why the file is skipped or rerouted.
    pub reason: Option<String>,
    /// Original name when a collision forced a rename.
    pub renamed_from: Option<String>,
    pub duplicate: bool,
    /// Status to record for skipped files; `None` keeps them pending so
    /// they can be organised in a later run.
    pub skip_status: Option<SampleStatus>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub transfers: u64,
    pub bytes: u64,
    pub skipped_duplicates: u64,
    pub undecided_duplicates: u64,
    pub kept_duplicates: u64,
    pub low_confidence_rerouted: u64,
    pub low_confidence_skipped: u64,
    pub renamed: u64,
    pub collisions_skipped: u64,
    pub missing: u64,
    pub folders: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub library_root: String,
    pub mode: OpMode,
    pub ops: Vec<PlannedOp>,
    pub summary: PlanSummary,
}

pub struct PlanOptions<'a> {
    pub library_root: &'a Path,
    pub mode: OpMode,
    pub collision: CollisionHandling,
    pub low_confidence: LowConfidenceAction,
    pub threshold: f32,
    pub uncategorised: (&'a str, &'a str), // (id, path)
}

/// Validate the output directory and return the library root inside it.
/// Creates the output directory itself only if its parent exists, and
/// verifies it is writable with a probe file.
pub fn validate_output(output_dir: &Path, root_folder: &str, sources: &[PathBuf]) -> CoreResult<PathBuf> {
    if !output_dir.is_absolute() {
        return Err(CoreError::InvalidPath("the output folder must be an absolute path".into()));
    }
    crate::config::validate_name(root_folder)?;
    if !output_dir.exists() {
        match output_dir.parent() {
            Some(p) if p.exists() => std::fs::create_dir(output_dir).at(output_dir)?,
            _ => return Err(CoreError::InvalidPath(format!("{} does not exist", output_dir.display()))),
        }
    }
    if !output_dir.is_dir() {
        return Err(CoreError::InvalidPath(format!("{} is not a folder", output_dir.display())));
    }
    let probe = output_dir.join(format!(".kitorg-write-test-{}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&probe, b"").map_err(|e| {
        CoreError::InvalidPath(format!("cannot write to {}: {e}", output_dir.display()))
    })?;
    let _ = std::fs::remove_file(&probe);
    let library = output_dir.join(root_folder);
    let canon_lib = std::fs::canonicalize(output_dir).map(|p| p.join(root_folder)).unwrap_or(library.clone());
    for s in sources {
        let cs = std::fs::canonicalize(s).unwrap_or(s.clone());
        if cs.starts_with(&canon_lib) {
            return Err(CoreError::InvalidPath(format!(
                "source {} is inside the library folder; choose a different output folder",
                s.display()
            )));
        }
    }
    Ok(library)
}

/// Make one folder name safe on every OS (no separators, no `..`, no
/// reserved characters or trailing dots/spaces).
pub fn safe_segment(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    let t = cleaned.trim().trim_end_matches('.').trim().to_string();
    if t == "." || t == ".." {
        String::new()
    } else {
        t
    }
}

pub fn build_plan(samples: &[SampleRow], opts: &PlanOptions) -> CoreResult<Plan> {
    let mut ops = Vec::with_capacity(samples.len());
    let mut s = PlanSummary::default();
    let mut claimed: HashSet<String> = HashSet::new();
    let mut folders: HashSet<String> = HashSet::new();
    let transfer = match opts.mode {
        OpMode::Copy => PlannedAction::Copy,
        OpMode::Move => PlannedAction::Move,
    };

    for r in samples.iter().filter(|r| r.status == SampleStatus::Pending) {
        let src = PathBuf::from(&r.original_path);
        let mut op = PlannedOp {
            sample_id: r.id.clone(),
            src: r.original_path.clone(),
            dest_dir: String::new(),
            dest_name: r.original_filename.clone(),
            dest_path: String::new(),
            action: transfer,
            category_id: r.category_id.clone(),
            category_path: r.category_path.clone(),
            hash: r.hash.clone(),
            size: r.size,
            reason: None,
            renamed_from: None,
            duplicate: r.duplicate_of.is_some(),
            skip_status: None,
        };
        let skip = |op: &mut PlannedOp, why: String, status: Option<SampleStatus>| {
            op.action = PlannedAction::Skip;
            op.reason = Some(why);
            op.skip_status = status;
        };

        if !src.is_file() {
            s.missing += 1;
            skip(&mut op, "source file is missing".into(), Some(SampleStatus::Failed));
            ops.push(op);
            continue;
        }
        if src.starts_with(opts.library_root) {
            skip(&mut op, "file is already inside the library".into(), Some(SampleStatus::Skipped));
            ops.push(op);
            continue;
        }
        if r.duplicate_of.is_some() {
            let where_ = r.duplicate_path.clone().unwrap_or_default();
            match r.dup_decision {
                Some(DupDecision::Skip) => {
                    s.skipped_duplicates += 1;
                    skip(&mut op, format!("identical content to {where_}"), Some(SampleStatus::Skipped));
                    ops.push(op);
                    continue;
                }
                None => {
                    s.undecided_duplicates += 1;
                    skip(&mut op, format!("duplicate of {where_} awaiting your decision"), None);
                    ops.push(op);
                    continue;
                }
                Some(DupDecision::Keep) => s.kept_duplicates += 1,
            }
        }
        let low = r.confidence < opts.threshold && !r.reviewed && !r.manual_override && r.category_id != opts.uncategorised.0;
        if low {
            match opts.low_confidence {
                LowConfidenceAction::Uncategorised => {
                    s.low_confidence_rerouted += 1;
                    op.category_id = opts.uncategorised.0.to_string();
                    op.category_path = opts.uncategorised.1.to_string();
                    op.reason = Some(format!(
                        "low confidence ({:.0}%) — predicted {}, placed in {} until reviewed",
                        r.confidence * 100.0,
                        r.category_path,
                        opts.uncategorised.1
                    ));
                }
                LowConfidenceAction::Skip => {
                    s.low_confidence_skipped += 1;
                    skip(&mut op, format!("low confidence ({:.0}%), not yet reviewed", r.confidence * 100.0), None);
                    ops.push(op);
                    continue;
                }
                LowConfidenceAction::Predicted => {}
            }
        }

        let mut dir = opts.library_root.to_path_buf();
        for seg in op.category_path.split('/').filter(|x| !x.is_empty()) {
            dir.push(seg);
        }
        // A construction kit's own folder (only when the file stays in the
        // category that set it, not when rerouted to Uncategorised).
        if let Some(sub) = r.subfolder.as_deref().filter(|_| op.category_id == r.category_id) {
            for seg in sub.split('/').map(safe_segment).filter(|x| !x.is_empty()) {
                dir.push(seg);
            }
        }
        let (dest, renamed) = unique_path(&dir, &r.original_filename, &claimed);
        if renamed {
            if opts.collision == CollisionHandling::Skip {
                s.collisions_skipped += 1;
                skip(&mut op, format!("a file named {} already exists", r.original_filename), Some(SampleStatus::Skipped));
                ops.push(op);
                continue;
            }
            s.renamed += 1;
            op.renamed_from = Some(r.original_filename.clone());
        }
        claimed.insert(path_key(&dest));
        folders.insert(path_key(&dir));
        op.dest_dir = dir.display().to_string();
        op.dest_name = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        op.dest_path = dest.display().to_string();
        s.transfers += 1;
        s.bytes += r.size;
        ops.push(op);
    }
    s.folders = folders.len() as u64;
    Ok(Plan { library_root: opts.library_root.display().to_string(), mode: opts.mode, ops, summary: s })
}
