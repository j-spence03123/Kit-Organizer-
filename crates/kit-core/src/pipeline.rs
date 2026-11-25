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

#[derive(Debug, Clone)]
pub struct AnalyseOptions {
    pub threads: usize,
    /// Decode a few seconds of each audio file for signal analysis.
    pub analyse_audio: bool,
}

/// Probe, hash (and optionally analyse) every file using a bounded thread
/// pool, then classify the batch with sibling context. Files are hashed by
/// streaming and at most a few seconds of audio are decoded, so memory
/// stays flat for large libraries.
pub fn analyse(
    files: Vec<ScannedFile>,
    classifier: &Classifier,
    opts: &AnalyseOptions,
    cancel: &AtomicBool,
    progress: &(dyn Fn(&ScanProgress) + Sync),
) -> CoreResult<Vec<Analysed>> {
    let total = files.len() as u64;
    let done = AtomicU64::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads.max(1))
        .build()
        .map_err(|e| CoreError::Other(format!("cannot start worker pool: {e}")))?;
    type Prepped = (ScannedFile, Option<AudioMeta>, Result<String, String>);
    let prepped: Vec<Option<Prepped>> = pool.install(|| {
        files
            .into_par_iter()
            .map(|file| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let audio = (file.kind == FileKind::Audio).then(|| {
                    let mut m = audio::probe(&file.path);
                    if opts.analyse_audio && m.error.is_none() {
                        m.features = audio::analyse_file(&file.path).ok();
                    }
                    m
                });
                let hash = hash_file(&file.path).map_err(|e| e.to_string());
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
                Some((file, audio, hash))
            })
            .collect()
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(CoreError::Cancelled);
    }
    let mut prepped: Vec<_> = prepped.into_iter().flatten().collect();
    prepped.sort_by(|a, b| a.0.path.cmp(&b.0.path));
    let groups: Vec<String> = prepped.iter().map(|(f, _, _)| f.path.parent().map(|p| p.display().to_string()).unwrap_or_default()).collect();
    let items: Vec<BatchItem> = prepped
        .iter()
        .zip(&groups)
        .map(|((f, a, _), g)| BatchItem { file_name: &f.file_name, folders: &f.folders, kind: f.kind, audio: a.as_ref(), group: g, fixed: None })
        .collect();
    let classes = pool.install(|| classify_batch(&items, classifier));
    Ok(prepped
        .into_iter()
        .zip(classes)
        .map(|((file, audio, hash), classification)| Analysed { file, audio, hash, classification: classification.expect("not fixed") })
        .collect())
}

/// One file in a batch classification.
pub struct BatchItem<'a> {
    pub file_name: &'a str,
    pub folders: &'a [String],
    pub kind: FileKind,
    pub audio: Option<&'a AudioMeta>,
    /// Files with the same group (their folder) give each other context.
    pub group: &'a str,
    /// A category the user already chose. Not reclassified, but counted
    /// as confident context for its siblings.
    pub fixed: Option<(&'a str, &'a str)>, // (category id, path)
}

/// Confidence at which a classification is trusted as sibling context.
pub const SIBLING_CONFIDENT: f32 = 0.7;

/// Classify a batch. Pass 1 classifies each file on its own. Pass 2: in
/// folders of 3+ files where at least two confident files (or manual
/// corrections) agree and make up 60%+ of the confident files, every
/// low-confidence file gets supporting evidence for that category and is
/// re-classified. Returns `None` for fixed items.
pub fn classify_batch(items: &[BatchItem], classifier: &Classifier) -> Vec<Option<Classification>> {
    let input = |it: &BatchItem<'_>, hints: &[crate::classifier::Hint]| -> Classification {
        classifier.classify(&ClassifyInput { file_name: it.file_name, folders: it.folders, kind: it.kind, audio: it.audio, hints })
    };
    let mut out: Vec<Option<Classification>> =
        items.par_iter().map(|it| if it.fixed.is_some() { None } else { Some(input(it, &[])) }).collect();

    let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        if it.kind == FileKind::Audio {
            groups.entry(it.group).or_default().push(i);
        }
    }
    let uncategorised = classifier.config().nodes[classifier.config().uncategorised].id.clone();
    let mut todo: Vec<(usize, crate::classifier::Hint)> = Vec::new();
    for idxs in groups.values().filter(|v| v.len() >= 3) {
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        let mut confident = 0usize;
        for &i in idxs {
            let cat = match (&items[i].fixed, &out[i]) {
                (Some((id, path)), _) => Some((id.to_string(), path.to_string())),
                (None, Some(c)) if c.confidence >= SIBLING_CONFIDENT && c.category_id != uncategorised => {
                    Some((c.category_id.clone(), c.category_path.clone()))
                }
                _ => None,
            };
            if let Some(k) = cat {
                confident += 1;
                *counts.entry(k).or_default() += 1;
            }
        }
        let Some(((id, path), n)) = counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))) else { continue };
        let share = n as f32 / confident.max(1) as f32;
        if n < 2 || share < 0.6 {
            continue;
        }
        let points = classifier.config().source.weights.sibling * share;
        for &i in idxs {
            if let Some(c) = &out[i] {
                if c.confidence < SIBLING_CONFIDENT {
                    let label = format!("{n} of {confident} confidently classified files in this folder are {path}");
                    todo.push((i, crate::classifier::Hint { category_id: id.clone(), points, label }));
                }
            }
        }
    }
    let redone: Vec<(usize, Classification)> =
        todo.par_iter().map(|(i, h)| (*i, input(&items[*i], std::slice::from_ref(h)))).collect();
    for (i, c) in redone {
        out[i] = Some(c);
    }
    out
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
    for f in folders {
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
            key_source: c.key_source,
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

/// Re-run classification for stored rows (after config changes or new
/// learned rules). Manual overrides are kept, and act as sibling context.
pub fn reclassify(rows: &[SampleRow], classifier: &Classifier) -> Vec<SampleRow> {
    let folders: Vec<Vec<String>> =
        rows.iter().map(|r| r.relative_dir.split('/').filter(|s| !s.is_empty()).map(String::from).collect()).collect();
    let items: Vec<BatchItem> = rows
        .iter()
        .zip(&folders)
        .map(|(r, f)| BatchItem {
            file_name: &r.original_filename,
            folders: f,
            kind: r.file_kind,
            audio: r.audio.as_ref(),
            group: &r.original_dir,
            fixed: (r.manual_override || r.status != SampleStatus::Pending).then_some((r.category_id.as_str(), r.category_path.as_str())),
        })
        .collect();
    let classes = classify_batch(&items, classifier);
    rows.iter()
        .zip(classes)
        .filter_map(|(r, c)| {
            let c = c?;
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
            n.key_source = c.key_source;
            n.method = c.method;
            Some(n)
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

#[cfg(test)]
mod sibling_tests {
    use super::*;
    use crate::config::{default_config, CompiledConfig};

    #[test]
    fn siblings_rescue_vague_names() {
        let c = Classifier::new(CompiledConfig::compile(&default_config()).unwrap());
        let folders = vec!["Pack".to_string(), "Stuff".to_string()];
        let names = ["Kick_01.wav", "Kick_02.wav", "Kick_Hard.wav", "XLNT_07.wav"];
        let items: Vec<BatchItem> = names
            .iter()
            .map(|n| BatchItem { file_name: n, folders: &folders, kind: FileKind::Audio, audio: None, group: "g", fixed: None })
            .collect();
        let out = classify_batch(&items, &c);
        let vague = out[3].as_ref().unwrap();
        assert_eq!(vague.category_path, "Kicks", "{:#?}", vague.evidence);
        assert!(vague.method.contains("siblings"));
        // Without siblings it would have been uncategorised.
        let alone = classify_batch(&items[3..], &c);
        assert_eq!(alone[0].as_ref().unwrap().category_path, "Uncategorised");
    }

    #[test]
    fn mixed_folders_give_no_hint() {
        let c = Classifier::new(CompiledConfig::compile(&default_config()).unwrap());
        let folders = vec!["Pack".to_string(), "Stuff".to_string()];
        let names = ["Kick_01.wav", "Snare_01.wav", "Clap_01.wav", "XLNT_07.wav"];
        let items: Vec<BatchItem> = names
            .iter()
            .map(|n| BatchItem { file_name: n, folders: &folders, kind: FileKind::Audio, audio: None, group: "g", fixed: None })
            .collect();
        let out = classify_batch(&items, &c);
        assert_eq!(out[3].as_ref().unwrap().category_path, "Uncategorised");
    }

    #[test]
    fn manual_corrections_count_as_context() {
        let c = Classifier::new(CompiledConfig::compile(&default_config()).unwrap());
        let folders = vec!["Pack".to_string(), "Shots".to_string()];
        let names = ["XLNT_01.wav", "XLNT_02.wav", "XLNT_03.wav"];
        let items: Vec<BatchItem> = names
            .iter()
            .enumerate()
            .map(|(i, n)| BatchItem {
                file_name: n,
                folders: &folders,
                kind: FileKind::Audio,
                audio: None,
                group: "g",
                fixed: (i < 2).then_some(("bass/bass-stabs", "Bass/Bass Stabs")),
            })
            .collect();
        let out = classify_batch(&items, &c);
        assert_eq!(out[2].as_ref().unwrap().category_path, "Bass/Bass Stabs");
    }
}
