//! Scan → analyse (probe + hash + classify, in parallel) → persist.

use crate::audio::{self, AudioMeta};
use crate::classifier::{Classification, Classifier, ClassifyInput};
use crate::db::{now, Database};
use crate::error::{CoreError, CoreResult};
use crate::filetypes::FileKind;
use crate::hashing::hash_file;
use crate::models::*;
use crate::scanner::{ScanProgress, ScannedFile};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub struct Analysed {
    pub file: ScannedFile,
    pub audio: Option<AudioMeta>,
    pub hash: Result<String, String>,
    pub classification: Classification,
}

/// Probe, hash and classify every file using a bounded thread pool. Audio
/// is never fully decoded here — only headers are read — and files are
/// hashed by streaming, so memory stays flat for large libraries.
pub fn analyse(
    files: Vec<ScannedFile>,
    classifier: &Classifier,
    threads: usize,
    cancel: &AtomicBool,
    progress: &(dyn Fn(&ScanProgress) + Sync),
) -> CoreResult<Vec<Analysed>> {
    let total = files.len() as u64;
    let done = AtomicU64::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
        .map_err(|e| CoreError::Other(format!("cannot start worker pool: {e}")))?;
    let out: Vec<Option<Analysed>> = pool.install(|| {
        files
            .into_par_iter()
            .map(|file| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let audio = (file.kind == FileKind::Audio).then(|| audio::probe(&file.path));
                let hash = hash_file(&file.path).map_err(|e| e.to_string());
                let classification = classifier.classify(&ClassifyInput {
                    file_name: &file.file_name,
                    folders: &file.folders,
                    kind: file.kind,
                    audio: audio.as_ref(),
                });
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_multiple_of(25) || n == total {
                    progress(&ScanProgress {
                        phase: "analysing".into(),
                        discovered: total,
                        supported: total,
                        unsupported: 0,
                        processed: n,
                        total,
                        current: file.file_name.clone(),
                    });
                }
                Some(Analysed { file, audio, hash, classification })
            })
            .collect()
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    let mut v: Vec<Analysed> = out.into_iter().flatten().collect();
    v.sort_by(|a, b| a.file.path.cmp(&b.file.path));
    Ok(v)
}

/// Folder names that are containers of packs rather than packs.
const CONTAINER_NAMES: &[&str] = &[
    "sample packs", "samplepacks", "packs", "downloads", "samples", "splice", "libraries", "library", "sounds", "drum kits",
    "kits", "desktop", "music", "producer", "production", "new folder",
];

/// Guess the source pack from the folder chain. The imported folder is
/// usually the pack, unless it is a container such as "Sample Packs" or
/// "Downloads", in which case the next folder down is used. Users can
/// correct this in the Review screen.
pub fn detect_source_pack(folders: &[String]) -> String {
    let norm = |s: &str| crate::classifier::tokenize::fine_tokens(s).join(" ");
    for f in folders.iter().take(folders.len().saturating_sub(0)) {
        if !CONTAINER_NAMES.contains(&norm(f).as_str()) {
            return f.clone();
        }
    }
    folders.first().cloned().unwrap_or_else(|| "Unknown".into())
}

/// Convert analysed files to database rows, detecting duplicates both
/// within the batch and against samples already organised.
pub fn build_rows(
    import_id: &str,
    analysed: Vec<Analysed>,
    db: &Database,
    dup: DuplicateHandling,
) -> CoreResult<Vec<SampleRow>> {
    let ts = now();
    let hashes: Vec<String> = analysed.iter().filter_map(|a| a.hash.as_ref().ok().cloned()).collect();
    let mut existing: HashMap<String, (String, String)> = HashMap::new();
    for (id, h, p) in db.organised_with_hashes(&hashes)? {
        existing.entry(h).or_insert((id, p));
    }
    let mut first_seen: HashMap<String, (String, String)> = HashMap::new();
    let default_decision = match dup {
        DuplicateHandling::Skip => Some(DupDecision::Skip),
        DuplicateHandling::Keep => Some(DupDecision::Keep),
        DuplicateHandling::Ask => None,
    };
    let mut rows = Vec::with_capacity(analysed.len());
    for a in analysed {
        let id = uuid::Uuid::new_v4().to_string();
        let path = a.file.path.display().to_string();
        let (hash, status, error) = match &a.hash {
            Ok(h) => (Some(h.clone()), SampleStatus::Pending, None),
            Err(e) => (None, SampleStatus::Failed, Some(format!("could not read file: {e}"))),
        };
        let mut duplicate_of = None;
        let mut duplicate_path = None;
        if let Some(h) = &hash {
            if let Some((eid, ep)) = existing.get(h) {
                duplicate_of = Some(eid.clone());
                duplicate_path = Some(ep.clone());
            } else if let Some((fid, fp)) = first_seen.get(h) {
                duplicate_of = Some(fid.clone());
                duplicate_path = Some(fp.clone());
            } else {
                first_seen.insert(h.clone(), (id.clone(), path.clone()));
            }
        }
        let c = a.classification;
        rows.push(SampleRow {
            id,
            import_id: import_id.to_string(),
            original_path: path,
            original_filename: a.file.file_name.clone(),
            original_dir: a.file.path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
            relative_dir: a.file.folders.join("/"),
            source_pack: detect_source_pack(&a.file.folders),
            size: a.file.size,
            hash,
            file_kind: a.file.kind,
            category_id: c.category_id,
            category_path: c.category_path,
            confidence: c.confidence,
            evidence: c.evidence,
            alternatives: c.alternatives,
            sample_type: c.sample_type,
            bpm: c.bpm,
            bpm_source: c.bpm_source,
            key: c.key,
            method: c.method,
            audio: a.audio,
            manual_override: false,
            reviewed: false,
            status,
            dup_decision: duplicate_of.as_ref().and(default_decision),
            duplicate_of,
            duplicate_path,
            destination_path: None,
            error,
            created_at: ts.clone(),
            updated_at: ts.clone(),
        });
    }
    Ok(rows)
}

/// Re-run classification for stored rows (after config changes). Manual
/// overrides are preserved by the database update.
pub fn reclassify(rows: &[SampleRow], classifier: &Classifier) -> Vec<SampleRow> {
    rows.par_iter()
        .filter(|r| !r.manual_override && r.status == SampleStatus::Pending)
        .map(|r| {
            let folders: Vec<String> = r.relative_dir.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
            let c = classifier.classify(&ClassifyInput {
                file_name: &r.original_filename,
                folders: &folders,
                kind: r.file_kind,
                audio: r.audio.as_ref(),
            });
            let mut n = r.clone();
            n.category_id = c.category_id;
            n.category_path = c.category_path;
            n.confidence = c.confidence;
            n.evidence = c.evidence;
            n.alternatives = c.alternatives;
            n.sample_type = c.sample_type;
            n.bpm = c.bpm;
            n.bpm_source = c.bpm_source;
            n.key = c.key;
            n.method = c.method;
            n
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_pack_detection() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(detect_source_pack(&v(&["XLNT Riddim", "Bass"])), "XLNT Riddim");
        assert_eq!(detect_source_pack(&v(&["Sample Packs", "Kompany Kit", "Drums"])), "Kompany Kit");
        assert_eq!(detect_source_pack(&v(&["Downloads", "Splice", "BOS Tearout"])), "BOS Tearout");
        assert_eq!(detect_source_pack(&v(&["Samples"])), "Samples");
    }
}
