//! Groups the review queue so large libraries can be reviewed a folder at
//! a time instead of a file at a time, and explains *why* each group is
//! uncertain.

use crate::classifier::learned::{pattern_of, Pattern};
use crate::classifier::Classifier;
use crate::config::CompiledConfig;
use crate::db::{Database, SampleQuery};
use crate::error::CoreResult;
use crate::models::{SampleRow, SampleStatus};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewGroup {
    /// Folder chain shared by the files (`relative_dir`).
    pub folder: String,
    pub category_id: String,
    pub category_path: String,
    pub count: usize,
    pub avg_confidence: f32,
    pub examples: Vec<String>,
    /// Most common reason the files in this group are uncertain.
    pub reason: String,
    /// The group covers sub-folders of `folder` too.
    pub includes_subfolders: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOverview {
    pub total: usize,
    pub uncategorised: usize,
    /// Just below the threshold (within 0.15 of it).
    pub near_threshold: usize,
    pub low: usize,
    /// Count per reason, most common first.
    pub reasons: Vec<(String, usize)>,
    pub groups: Vec<ReviewGroup>,
}

/// Why a sample's confidence is low, from its evidence trail.
pub fn reason(r: &SampleRow, uncategorised_id: &str) -> &'static str {
    let has = |needle: &str| r.evidence.iter().any(|e| e.contains(needle));
    if r.method == "daw-project" || has("Audio from a DAW project") {
        return "Audio from a DAW project, not a sample";
    }
    if r.method == "pack-demo" {
        return "Pack demo or preview";
    }
    if has("Only other files in the folder suggest this") {
        return "Only other files in the folder suggest it";
    }
    if r.category_id == uncategorised_id {
        if has("Evidence too weak") {
            return "Only weak hints (e.g. the pack name)";
        }
        return "No recognisable keywords";
    }
    if has("Conflicting evidence") {
        return "Name and folder disagree, or several keywords";
    }
    if has("No subcategory evidence") {
        return "Only a broad category matched";
    }
    if has("Loop/one-shot decision is borderline") {
        return "Unsure if loop or one-shot";
    }
    match r.method.as_str() {
        "folder" => "Only the folder name matched",
        m if m.contains("audio") && !m.contains("filename") && !m.contains("folder") => "Only audio analysis",
        m if m.contains("filename") && r.evidence.iter().any(|e| e.contains("typo")) => "Probable typo in the name",
        _ => "Weak single keyword",
    }
}

/// Group the queue. `up_levels` = 0 groups by each file's folder; 1 groups
/// by the folder above it (merging e.g. every kit folder of a pack).
pub fn overview(rows: &[SampleRow], threshold: f32, uncategorised_id: &str, max_groups: usize) -> ReviewOverview {
    overview_at(rows, threshold, uncategorised_id, max_groups, 0)
}

fn folder_at(dir: &str, up: usize) -> &str {
    let mut end = dir.len();
    for _ in 0..up {
        match dir[..end].rfind('/') {
            Some(i) if i > 0 => end = i,
            _ => break,
        }
    }
    &dir[..end]
}

pub fn overview_at(rows: &[SampleRow], threshold: f32, uncategorised_id: &str, max_groups: usize, up_levels: usize) -> ReviewOverview {
    let mut groups: HashMap<(&str, &str), Vec<&SampleRow>> = HashMap::new();
    let mut reasons: HashMap<&'static str, usize> = HashMap::new();
    let (mut unc, mut near, mut low) = (0, 0, 0);
    for r in rows {
        groups.entry((folder_at(&r.relative_dir, up_levels), r.category_id.as_str())).or_default().push(r);
        *reasons.entry(reason(r, uncategorised_id)).or_default() += 1;
        if r.category_id == uncategorised_id {
            unc += 1;
        } else if r.confidence >= threshold - 0.15 {
            near += 1;
        } else {
            low += 1;
        }
    }
    let mut out: Vec<ReviewGroup> = groups
        .into_iter()
        .map(|((folder, cat), items)| {
            let mut rc: HashMap<&'static str, usize> = HashMap::new();
            for r in &items {
                *rc.entry(reason(r, uncategorised_id)).or_default() += 1;
            }
            let reason = rc.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0))).map(|(k, _)| k).unwrap_or("");
            ReviewGroup {
                includes_subfolders: items.iter().any(|r| r.relative_dir != folder),
                folder: folder.to_string(),
                category_id: cat.to_string(),
                category_path: items[0].category_path.clone(),
                // A representative counts for every file its decision covers.
                count: items.iter().map(|r| r.review_group_size.map(|n| n as usize).filter(|_| !r.reviewed).unwrap_or(1)).sum(),
                avg_confidence: items.iter().map(|r| r.confidence).sum::<f32>() / items.len() as f32,
                examples: items.iter().take(4).map(|r| r.original_filename.clone()).collect(),
                reason: reason.to_string(),
            }
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.folder.cmp(&b.folder)).then(a.category_path.cmp(&b.category_path)));
    out.truncate(max_groups);
    let mut reasons: Vec<(String, usize)> = reasons.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    reasons.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ReviewOverview { total: rows.len(), uncategorised: unc, near_threshold: near, low, reasons, groups: out }
}

/// Lines in benchmark format (`Category | path`) with the current
/// predictions, so a review list can be corrected and reused as a
/// benchmark — or shared for tuning (names only, no audio).
pub fn export_lines(rows: &[SampleRow], uncategorised_id: &str) -> String {
    let mut out = String::from(
        "# Kit Organizer review list. Each line: predicted category | file path.\n\
         # Fix the category on wrong lines, then run:\n\
         #   cargo run -p kit-core --example benchmark -- this-file.txt\n\n",
    );
    for r in rows {
        out.push_str(&format!("{} | {}/{}\n", r.category_path, r.relative_dir, r.original_filename));
        out.push_str(&format!("#   {:.0}% · {}\n", r.confidence * 100.0, reason(r, uncategorised_id)));
    }
    out
}

/// Learnable patterns for confirmed decisions. `category` overrides each
/// row's own category (a reassignment); `None` confirms the predictions.
pub fn patterns_for(cfg: &CompiledConfig, rows: &[SampleRow], category: Option<&str>) -> Vec<(Pattern, String, String)> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter(|r| category.is_some() || r.category_id != cfg.nodes[cfg.uncategorised].id)
        // Kit membership comes from the folder, not the name: nothing to learn.
        .filter(|r| category.unwrap_or(&r.category_id) != crate::classifier::KITS_ID)
        .filter_map(|r| {
            let folders: Vec<String> = r.relative_dir.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
            let p = pattern_of(cfg, &r.original_filename, &folders)?;
            let cat = category.unwrap_or(&r.category_id).to_string();
            seen.insert((p.clone(), cat.clone())).then(|| (p, cat, r.original_filename.clone()))
        })
        .collect()
}

/// Re-run the classifier over an import's pending samples and store the
/// results. Returns how many still need review afterwards.
pub fn reclassify_import(db: &mut Database, classifier: &Classifier, import_id: &str, threshold: f32) -> CoreResult<usize> {
    let rows = db.samples_for_import(import_id)?;
    let updated = crate::pipeline::reclassify(&rows, classifier);
    db.update_classification(&updated)?;
    needs_review_count(db, import_id, threshold)
}

pub fn needs_review_count(db: &Database, import_id: &str, threshold: f32) -> CoreResult<usize> {
    Ok(db
        .query_samples(&SampleQuery {
            import_id: Some(import_id.to_string()),
            needs_review_below: Some(threshold),
            limit: 1,
            ..Default::default()
        })?
        .total as usize)
}

/// Rows of an import that still need review.
pub fn pending_review(db: &Database, import_id: &str, threshold: f32) -> CoreResult<Vec<SampleRow>> {
    Ok(db
        .matching_samples(&SampleQuery { import_id: Some(import_id.to_string()), needs_review_below: Some(threshold), ..Default::default() })?
        .into_iter()
        .filter(|r| r.status == SampleStatus::Pending)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_config;
    use crate::db::now;
    use crate::filetypes::FileKind;
    use crate::models::*;

    fn row(id: usize, dir: &str, name: &str, c: &Classifier) -> SampleRow {
        let folders: Vec<String> = dir.split('/').map(String::from).collect();
        let cl = c.classify(&crate::classifier::ClassifyInput { file_name: name, folders: &folders, kind: FileKind::Audio, audio: None, hints: &[] });
        SampleRow {
            id: format!("s{id}"),
            import_id: "imp".into(),
            original_path: format!("/x/{dir}/{name}"),
            original_filename: name.into(),
            original_dir: format!("/x/{dir}"),
            relative_dir: dir.into(),
            source_pack: folders[0].clone(),
            size: 1,
            hash: Some(format!("h{id}")),
            file_kind: FileKind::Audio,
            category_id: cl.category_id,
            category_path: cl.category_path,
            confidence: cl.confidence,
            evidence: cl.evidence,
            alternatives: cl.alternatives,
            sample_type: cl.sample_type,
            bpm: None,
            bpm_source: None,
            key: None,
            key_source: None,
            method: cl.method,
            subfolder: cl.subfolder,
            auto_resolved: false,
            review_group: None,
            review_group_size: None,
            audio: None,
            manual_override: false,
            reviewed: false,
            status: SampleStatus::Pending,
            duplicate_of: None,
            duplicate_path: None,
            dup_decision: None,
            destination_path: None,
            error: None,
            created_at: now(),
            updated_at: now(),
        }
    }

    #[test]
    fn grouping_one_level_up_merges_kits() {
        assert_eq!(folder_at("Kits/Pack/Kit A", 1), "Kits/Pack");
        assert_eq!(folder_at("Kits", 1), "Kits");
        let cfg = CompiledConfig::compile(&default_config()).unwrap();
        let c = Classifier::new(cfg);
        let rows: Vec<SampleRow> = (0..6).map(|i| row(i, &format!("Pack/Kit {}", i % 3), &format!("Pack_Thump_{i}.wav"), &c)).collect();
        assert_eq!(overview_at(&rows, 0.6, "uncategorised", 50, 0).groups.len(), 3);
        let up = overview_at(&rows, 0.6, "uncategorised", 50, 1);
        assert_eq!(up.groups.len(), 1);
        assert!(up.groups[0].includes_subfolders);
    }

    #[test]
    fn prefix_query_matches_subfolders_only() {
        let db = Database::open_in_memory().unwrap();
        db.create_import(&ImportRecord {
            id: "imp".into(), created_at: now(), completed_at: None, sources: vec![], output_dir: String::new(), library_root: String::new(),
            mode: OpMode::Copy, status: ImportStatus::Review, stats: ImportStats::default(), messages: vec![],
        }).unwrap();
        let c = Classifier::new(CompiledConfig::compile(&default_config()).unwrap());
        let mut db = db;
        db.insert_samples(&[row(1, "Pack/Kit_A", "a.wav", &c), row(2, "Pack/Kit_A/Sub", "b.wav", &c), row(3, "Pack/Kit_AB", "c.wav", &c)]).unwrap();
        let n = db.matching_samples(&SampleQuery { relative_dir_prefix: Some("Pack/Kit_A".into()), ..Default::default() }).unwrap().len();
        assert_eq!(n, 2, "Kit_AB must not match the Kit_A prefix");
    }

    #[test]
    fn one_group_decision_shrinks_the_queue() {
        let cfg = CompiledConfig::compile(&default_config()).unwrap();
        let classifier = Classifier::new(cfg.clone());
        let mut db = Database::open_in_memory().unwrap();
        db.create_import(&ImportRecord {
            id: "imp".into(),
            created_at: now(),
            completed_at: None,
            sources: vec![],
            output_dir: String::new(),
            library_root: String::new(),
            mode: OpMode::Copy,
            status: ImportStatus::Review,
            stats: ImportStats::default(),
            messages: vec![],
        })
        .unwrap();
        // 40 files named "Thump" across two folders: nothing recognisable.
        let mut rows = Vec::new();
        for i in 0..20 {
            rows.push(row(i, "Pack/Batch A", &format!("Pack_Thump_{i:02}.wav"), &classifier));
            rows.push(row(100 + i, "Pack/Batch B", &format!("Pack_Thump_{i:02}.wav"), &classifier));
        }
        db.insert_samples(&rows).unwrap();
        assert_eq!(needs_review_count(&db, "imp", 0.6).unwrap(), 40);

        let ov = overview(&pending_review(&db, "imp", 0.6).unwrap(), 0.6, "uncategorised", 50);
        assert_eq!(ov.groups.len(), 2);
        assert_eq!(ov.uncategorised, 40);
        assert_eq!(ov.reasons[0].0, "No recognisable keywords");

        // The user moves one group (Batch A) to Kicks.
        let group: Vec<SampleRow> = rows.iter().filter(|r| r.relative_dir == "Pack/Batch A").cloned().collect();
        let ids: Vec<String> = group.iter().map(|r| r.id.clone()).collect();
        db.reassign(&ids, "kicks", "Kicks").unwrap();
        db.upsert_learned_many(&patterns_for(&cfg, &group, Some("kicks"))).unwrap();
        let learned = Classifier::new(cfg.clone()).with_learned(db.learned_rules().unwrap());
        let left = reclassify_import(&mut db, &learned, "imp", 0.6).unwrap();
        // Batch B follows automatically.
        assert_eq!(left, 0);
        let b = db.matching_samples(&SampleQuery { relative_dir: Some("Pack/Batch B".into()), ..Default::default() }).unwrap();
        assert!(b.iter().all(|r| r.category_path == "Kicks" && r.confidence >= 0.6), "{:?}", b[0].evidence);
    }
}

// ---------- review budget: automatic resolution ----------

/// One row's new state after automatic resolution.
#[derive(Debug, Clone)]
pub struct AutoUpdate {
    pub id: String,
    pub category_id: String,
    pub category_path: String,
    pub subfolder: Option<String>,
    pub reviewed: bool,
    pub auto_resolved: bool,
    pub review_group: Option<String>,
    pub review_group_size: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoResult {
    /// Best guesses accepted automatically.
    pub accepted: usize,
    /// Kept in Uncategorised (original folders preserved).
    pub uncategorised: usize,
    /// Representatives left for a person to decide.
    pub representatives: usize,
    /// Files those representatives decide for (including themselves).
    pub covered_by_representatives: usize,
}

/// Uncategorised files keep their original folder structure (minus the
/// imported root) so they stay browsable by pack.
pub fn uncategorised_subfolder(relative_dir: &str) -> Option<String> {
    let parts: Vec<&str> = relative_dir.split('/').filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => None,
        1 => Some(parts[0].to_string()),
        _ => Some(parts[1..].join("/")),
    }
}

/// Keep at most `budget` files in the review queue.
///
/// Candidates are pending, non-manual files that are either newly
/// uncertain (below `threshold`, unreviewed) or were resolved
/// automatically before. They are grouped by folder and prediction.
/// Existing representatives keep their place; free slots go to the new
/// groups with the highest impact (`files × (1 − confidence)`), or — with
/// `promote` — to previously auto-resolved groups as well ("Review more").
/// Every other candidate is resolved: a guess at or above `floor` is
/// accepted; anything weaker goes to Uncategorised with its folders kept.
pub fn auto_resolve(
    rows: &[SampleRow],
    cfg: &CompiledConfig,
    threshold: f32,
    floor: f32,
    budget: usize,
    promote: bool,
) -> (Vec<AutoUpdate>, AutoResult) {
    let unc = &cfg.nodes[cfg.uncategorised];
    let candidate = |r: &&SampleRow| {
        r.status == SampleStatus::Pending
            && !r.manual_override
            && (r.auto_resolved || (!r.reviewed && r.confidence < threshold))
    };
    let cands: Vec<&SampleRow> = rows.iter().filter(candidate).collect();
    let is_rep = |r: &SampleRow| !r.auto_resolved && !r.reviewed && r.review_group.is_some() && r.confidence < threshold;
    let key_of = |r: &SampleRow| format!("{}|{}", r.relative_dir, r.category_id);

    // Groups among candidates.
    let mut groups: HashMap<String, Vec<&SampleRow>> = HashMap::new();
    for r in &cands {
        groups.entry(r.review_group.clone().filter(|_| is_rep(r)).unwrap_or_else(|| key_of(r))).or_default().push(r);
    }
    // Existing representatives keep their slots.
    let mut reps: HashMap<String, String> = HashMap::new(); // group -> rep id
    for r in cands.iter().filter(|r| is_rep(r)) {
        if reps.len() < budget {
            reps.insert(r.review_group.clone().unwrap(), r.id.clone());
        }
    }
    // Fill free slots by impact.
    let mut ranked: Vec<(&String, f32)> = groups
        .iter()
        .filter(|(k, v)| !reps.contains_key(*k) && (promote || v.iter().any(|r| !r.auto_resolved)))
        .map(|(k, v)| {
            let conf = v.iter().map(|r| r.confidence).sum::<f32>() / v.len() as f32;
            (k, v.len() as f32 * (1.0 - conf))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(b.0)));
    for (k, _) in ranked {
        if reps.len() >= budget {
            break;
        }
        let mut members = groups[k].clone();
        members.sort_by(|a, b| a.original_filename.cmp(&b.original_filename));
        reps.insert(k.clone(), members[0].id.clone());
    }

    let mut out = Vec::with_capacity(cands.len());
    let mut res = AutoResult { representatives: reps.len(), ..Default::default() };
    for (k, members) in &groups {
        let rep = reps.get(k);
        if rep.is_some() {
            res.covered_by_representatives += members.len();
        }
        for r in members {
            if Some(&r.id) == rep {
                out.push(AutoUpdate {
                    id: r.id.clone(),
                    category_id: r.category_id.clone(),
                    category_path: r.category_path.clone(),
                    subfolder: r.subfolder.clone(),
                    reviewed: false,
                    auto_resolved: false,
                    review_group: Some(k.clone()),
                    review_group_size: Some(members.len() as u32),
                });
                continue;
            }
            let group = rep.map(|_| k.clone());
            if r.confidence >= threshold {
                // Became confident after re-classifying: no longer automatic.
                out.push(AutoUpdate {
                    id: r.id.clone(),
                    category_id: r.category_id.clone(),
                    category_path: r.category_path.clone(),
                    subfolder: r.subfolder.clone(),
                    reviewed: r.reviewed,
                    auto_resolved: false,
                    review_group: None,
                    review_group_size: None,
                });
            } else if r.category_id != unc.id && r.confidence >= floor {
                res.accepted += 1;
                out.push(AutoUpdate {
                    id: r.id.clone(),
                    category_id: r.category_id.clone(),
                    category_path: r.category_path.clone(),
                    subfolder: r.subfolder.clone(),
                    reviewed: true,
                    auto_resolved: true,
                    review_group: group,
                    review_group_size: None,
                });
            } else {
                res.uncategorised += 1;
                out.push(AutoUpdate {
                    id: r.id.clone(),
                    category_id: unc.id.clone(),
                    category_path: unc.path.clone(),
                    subfolder: uncategorised_subfolder(&r.relative_dir),
                    reviewed: true,
                    auto_resolved: true,
                    review_group: group,
                    review_group_size: None,
                });
            }
        }
    }
    (out, res)
}

/// Run automatic resolution for an import and store the result.
pub fn auto_resolve_import(
    db: &mut Database,
    cfg: &CompiledConfig,
    import_id: &str,
    threshold: f32,
    floor: f32,
    budget: usize,
    promote: bool,
) -> CoreResult<AutoResult> {
    if budget == 0 {
        return Ok(AutoResult::default());
    }
    let rows = db.samples_for_import(import_id)?;
    let (updates, res) = auto_resolve(&rows, cfg, threshold, floor, budget, promote);
    db.apply_auto(&updates)?;
    Ok(res)
}

#[cfg(test)]
mod auto_tests {
    use super::*;
    use crate::config::default_config;
    use crate::db::now;
    use crate::models::*;

    fn r(id: usize, dir: &str, cat: &str, conf: f32) -> SampleRow {
        SampleRow {
            id: format!("s{id}"), import_id: "imp".into(), original_path: format!("/x/{dir}/f{id}.wav"), original_filename: format!("f{id:04}.wav"),
            original_dir: format!("/x/{dir}"), relative_dir: dir.into(), source_pack: "p".into(), size: 1, hash: None,
            file_kind: crate::filetypes::FileKind::Audio, category_id: cat.into(), category_path: cat.into(), confidence: conf,
            evidence: vec![], alternatives: vec![], sample_type: crate::classifier::SampleType::OneShot, bpm: None, bpm_source: None,
            key: None, key_source: None, method: "x".into(), subfolder: None, auto_resolved: false, review_group: None, review_group_size: None,
            audio: None, manual_override: false, reviewed: false, status: SampleStatus::Pending, duplicate_of: None, duplicate_path: None,
            dup_decision: None, destination_path: None, error: None, created_at: now(), updated_at: now(),
        }
    }

    #[test]
    fn budget_caps_the_queue_and_resolves_the_rest_safely() {
        let cfg = CompiledConfig::compile(&default_config()).unwrap();
        let mut rows = Vec::new();
        for g in 0..120 {
            for i in 0..5 {
                let (cat, conf) = if g % 2 == 0 { ("fx/other-fx", 0.4) } else { ("uncategorised", 0.0) };
                rows.push(r(g * 10 + i, &format!("Kits/Pack/Folder {g}"), cat, conf));
            }
        }
        rows.push(r(9999, "Kits/Pack/Sure", "kicks", 0.9));
        let (ups, res) = auto_resolve(&rows, &cfg, 0.6, 0.35, 50, false);
        assert_eq!(res.representatives, 50);
        let left: Vec<_> = ups.iter().filter(|u| !u.reviewed).collect();
        assert_eq!(left.len(), 50, "queue capped at the budget");
        // Highest impact first: uncategorised groups (confidence 0) win.
        assert!(left.iter().all(|u| u.category_id == "uncategorised"));
        assert!(left.iter().all(|u| u.review_group_size == Some(5)));
        // Guesses >= floor are accepted; weaker ones go to Uncategorised with folders kept.
        let acc = ups.iter().find(|u| u.reviewed && u.category_id == "fx/other-fx").unwrap();
        assert!(acc.auto_resolved);
        let unc = ups.iter().find(|u| u.reviewed && u.category_id == "uncategorised").unwrap();
        assert_eq!(unc.subfolder.as_deref().map(|s| s.starts_with("Pack/Folder")), Some(true));
        // Confident files are untouched.
        assert!(!ups.iter().any(|u| u.id == "s9999"));
    }

    #[test]
    fn existing_representatives_keep_their_place_and_more_can_be_promoted() {
        let cfg = CompiledConfig::compile(&default_config()).unwrap();
        let mut rows: Vec<SampleRow> = (0..30).map(|g| r(g, &format!("Kits/P/F{g}"), "uncategorised", 0.0)).collect();
        let (ups, _) = auto_resolve(&rows, &cfg, 0.6, 0.35, 10, false);
        for u in &ups {
            let row = rows.iter_mut().find(|x| x.id == u.id).unwrap();
            row.reviewed = u.reviewed;
            row.auto_resolved = u.auto_resolved;
            row.review_group = u.review_group.clone();
            row.category_id = u.category_id.clone();
        }
        let reps_before: Vec<String> = ups.iter().filter(|u| !u.reviewed).map(|u| u.id.clone()).collect();
        // Running again changes nothing (idempotent) ...
        let (again, res) = auto_resolve(&rows, &cfg, 0.6, 0.35, 10, false);
        let reps_again: Vec<String> = again.iter().filter(|u| !u.reviewed).map(|u| u.id.clone()).collect();
        assert_eq!(res.representatives, 10);
        assert_eq!({ let mut a = reps_before.clone(); a.sort(); a }, { let mut b = reps_again; b.sort(); b });
        // ... and once the user decides them, "Review more" promotes new ones.
        for id in &reps_before {
            rows.iter_mut().find(|x| &x.id == id).unwrap().manual_override = true;
        }
        let (_, quiet) = auto_resolve(&rows, &cfg, 0.6, 0.35, 10, false);
        assert_eq!(quiet.representatives, 0, "no new work appears on its own");
        let (_, more) = auto_resolve(&rows, &cfg, 0.6, 0.35, 10, true);
        assert_eq!(more.representatives, 10);
    }

    #[test]
    fn uncategorised_keeps_original_folders() {
        assert_eq!(uncategorised_subfolder("Kits/vengeance/VEH1 Cutted Sounds").as_deref(), Some("vengeance/VEH1 Cutted Sounds"));
        assert_eq!(uncategorised_subfolder("Pack").as_deref(), Some("Pack"));
    }
}
