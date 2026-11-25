//! IPC commands. Long-running work (scanning, organising) runs on
//! background threads and reports through events, so the UI never blocks.
//!
//! Events:
//! * `scan-progress`     — `{ jobId, phase, discovered, supported, unsupported, processed, total, current }`
//! * `scan-finished`     — `{ jobId, ok, error, cancelled }`
//! * `organise-progress` — `{ jobId, processed, total, bytesDone, bytesTotal, current }`
//! * `organise-finished` — `{ jobId, ok, error, report }`

use crate::state::AppState;
use kit_core::audio::AudioMeta;
use kit_core::classifier::{learned, Classification, ClassifyInput};
use kit_core::config::{default_config, CategoryConfig, CompiledConfig};
use kit_core::db::{now, DashboardStats, ImportSummary, OperationRow, SamplePage, SampleQuery};
use kit_core::error::CoreError;
use kit_core::models::*;
use kit_core::organiser::{self, ExecReport, Plan, PlanOptions, PlanSummary, PlannedAction, PlannedOp, UndoReport};
use kit_core::scanner::{scan, ScanOptions, ScanProgress};
use kit_core::{pipeline, review, CoreResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------------- settings & categories ----------------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> CmdResult<Settings> {
    state.db().settings().map_err(err)
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: Settings) -> CmdResult<()> {
    state.db().save_settings(&settings).map_err(err)
}

#[tauri::command]
pub fn get_category_config(state: State<AppState>) -> CmdResult<CategoryConfig> {
    state.db().category_config().map_err(err)
}

fn apply_config(state: &AppState, cfg: &CategoryConfig) -> CoreResult<()> {
    let compiled = CompiledConfig::compile(cfg)?;
    let paths: Vec<(String, String)> = compiled.nodes.iter().map(|n| (n.id.clone(), n.path.clone())).collect();
    {
        let mut db = state.db();
        db.save_category_config(cfg)?;
        // Keep stored paths of pending samples in sync with renames.
        db.refresh_category_paths(&paths)?;
    }
    state.rebuild_classifier()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remap {
    from_id: String,
    to_id: String,
}

/// Save a new category configuration. `remaps` moves pending samples off
/// categories that were deleted or merged.
#[tauri::command]
pub fn save_category_config(state: State<AppState>, config: CategoryConfig, remaps: Option<Vec<Remap>>) -> CmdResult<()> {
    let compiled = CompiledConfig::compile(&config).map_err(err)?;
    for r in remaps.unwrap_or_default() {
        let to = compiled.by_id.get(&r.to_id).ok_or_else(|| format!("unknown category {}", r.to_id))?;
        state.db().remap_category(&r.from_id, &r.to_id, compiled.path_of(*to)).map_err(err)?;
    }
    apply_config(&state, &config).map_err(err)
}

#[tauri::command]
pub fn reset_category_config(state: State<AppState>) -> CmdResult<CategoryConfig> {
    let cfg = default_config();
    // Samples pointing at custom categories fall back to Uncategorised.
    let compiled = CompiledConfig::compile(&cfg).map_err(err)?;
    let usage = state.db().category_usage().map_err(err)?;
    for (id, _) in usage {
        if !compiled.by_id.contains_key(&id) {
            state.db().remap_category(&id, &cfg.uncategorised_category, "Uncategorised").map_err(err)?;
        }
    }
    apply_config(&state, &cfg).map_err(err)?;
    Ok(cfg)
}

#[tauri::command]
pub fn category_usage(state: State<AppState>) -> CmdResult<Vec<(String, u64)>> {
    state.db().category_usage().map_err(err)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedConfig {
    pub app: String,
    pub format_version: u32,
    pub categories: CategoryConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    /// Rules learned from your corrections and review decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learned: Option<Vec<learned::LearnedRule>>,
}

fn check_json_path(path: &str) -> CmdResult<PathBuf> {
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err("path must be absolute".into());
    }
    match p.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("json") => Ok(p),
        _ => Err("configuration files must have a .json extension".into()),
    }
}

#[tauri::command]
pub fn export_config(state: State<AppState>, path: String, include_settings: bool) -> CmdResult<()> {
    let p = check_json_path(&path)?;
    let db = state.db();
    let out = ExportedConfig {
        app: "kit-organizer".into(),
        format_version: 1,
        categories: db.category_config().map_err(err)?,
        settings: if include_settings { Some(db.settings().map_err(err)?) } else { None },
        learned: if include_settings { Some(db.learned_rules().map_err(err)?) } else { None },
    };
    let json = serde_json::to_string_pretty(&out).map_err(err)?;
    std::fs::write(&p, json).map_err(|e| format!("could not write {}: {e}", p.display()))
}

/// Merge learned rules from an imported configuration.
#[tauri::command]
pub fn import_learned_rules(state: State<AppState>, rules: Vec<learned::LearnedRule>) -> CmdResult<usize> {
    let items: Vec<(learned::Pattern, String, String)> = rules
        .iter()
        .map(|r| (learned::Pattern { kind: r.kind, tokens: r.tokens.clone(), scope: r.scope.clone() }, r.category_id.clone(), r.example.clone()))
        .collect();
    state.db().upsert_learned_many(&items).map_err(err)?;
    state.rebuild_classifier().map_err(err)?;
    Ok(items.len())
}

/// Read and validate a configuration file without applying it.
#[tauri::command]
pub fn import_config(path: String) -> CmdResult<ExportedConfig> {
    let p = check_json_path(&path)?;
    let meta = std::fs::metadata(&p).map_err(err)?;
    if meta.len() > 10 * 1024 * 1024 {
        return Err("configuration file is too large".into());
    }
    let text = std::fs::read_to_string(&p).map_err(err)?;
    // Accept both a full export and a bare category configuration.
    let parsed: ExportedConfig = match serde_json::from_str::<ExportedConfig>(&text) {
        Ok(c) => c,
        Err(_) => ExportedConfig {
            app: "kit-organizer".into(),
            format_version: 1,
            categories: serde_json::from_str(&text).map_err(|e| format!("not a Kit Organizer configuration: {e}"))?,
            settings: None,
            learned: None,
        },
    };
    CompiledConfig::compile(&parsed.categories).map_err(err)?;
    Ok(parsed)
}

// ---------------- scanning ----------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgressEvent<'a> {
    job_id: &'a str,
    #[serde(flatten)]
    progress: &'a ScanProgress,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FinishedEvent {
    job_id: String,
    ok: bool,
    cancelled: bool,
    error: Option<String>,
    report: Option<serde_json::Value>,
}

/// Start scanning `sources`. Returns the import id (also the job id).
#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    state: State<AppState>,
    sources: Vec<String>,
    output_dir: Option<String>,
    mode: Option<OpMode>,
) -> CmdResult<String> {
    if sources.is_empty() {
        return Err("add at least one folder to import".into());
    }
    let settings = state.db().settings().map_err(err)?;
    let cfg = state.db().category_config().map_err(err)?;
    let roots: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    let output = output_dir.clone().or(settings.default_output_dir.clone()).unwrap_or_default();
    let exclude: Vec<PathBuf> =
        if output.is_empty() { vec![] } else { vec![PathBuf::from(&output).join(&cfg.root_folder)] };
    kit_core::scanner::normalise_roots(&roots, &exclude).map_err(err)?;

    let id = uuid::Uuid::new_v4().to_string();
    state
        .db()
        .create_import(&ImportRecord {
            id: id.clone(),
            created_at: now(),
            completed_at: None,
            sources: sources.clone(),
            output_dir: output.clone(),
            library_root: if output.is_empty() { String::new() } else { Path::new(&output).join(&cfg.root_folder).display().to_string() },
            mode: mode.unwrap_or(settings.default_mode),
            status: ImportStatus::Scanning,
            stats: ImportStats::default(),
            messages: vec![],
        })
        .map_err(err)?;
    let cancel = state.register_job(&id);
    let job = id.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let classifier = state.classifier();
        let emit = |p: &ScanProgress| {
            let _ = app.emit("scan-progress", ScanProgressEvent { job_id: &job, progress: p });
        };
        let result = (|| -> CoreResult<ImportStats> {
            let rep = scan(&ScanOptions { roots: &roots, exclude: &exclude, file_types: &settings.file_types, skip_daw_projects: settings.skip_daw_projects }, &cancel, &emit)?;
            let mut stats = ImportStats {
                files_discovered: rep.total,
                supported: rep.files.len() as u64,
                unsupported: rep.unsupported,
                ignored: rep.ignored,
                scan_errors: rep.issues.len() as u64,
                ..Default::default()
            };
            let mut messages: Vec<String> =
                rep.issues.iter().take(200).map(|i| format!("Could not read {}: {}", i.path, i.message)).collect();
            if rep.unsupported > 0 {
                messages.push(format!("{} unsupported file(s) were left untouched", rep.unsupported));
            }
            if !rep.skipped_daw_dirs.is_empty() {
                messages.push(format!(
                    "Skipped {} DAW project folder(s) (project recordings/renders, not samples) — turn this off in Settings to include them",
                    rep.skipped_daw_dirs.len()
                ));
                messages.extend(rep.skipped_daw_dirs.iter().take(50).map(|d| format!("Skipped DAW project folder: {d}")));
            }
            let opts = pipeline::AnalyseOptions { threads: settings.threads(), analyse_audio: settings.analyse_audio };
            let analysed = pipeline::analyse(rep.files, &classifier, &opts, &cancel, &emit)?;
            let rows = {
                let db = state.db();
                pipeline::build_rows(&job, analysed, &db, settings.duplicate_handling)?
            };
            stats.duplicates = rows.iter().filter(|r| r.duplicate_of.is_some()).count() as u64;
            stats.errors = rows.iter().filter(|r| r.status == SampleStatus::Failed).count() as u64;
            let mut db = state.db();
            for chunk in rows.chunks(2000) {
                db.insert_samples(chunk)?;
            }
            if settings.review_budget > 0 {
                let c = state.classifier();
                review::auto_resolve_import(&mut db, c.config(), &job, settings.confidence_threshold, settings.auto_accept_floor, settings.review_budget, false)?;
            }
            let mut imp = db.get_import(&job)?.ok_or_else(|| CoreError::Other("import vanished".into()))?;
            imp.status = ImportStatus::Review;
            imp.stats = stats.clone();
            imp.messages = messages;
            db.update_import(&imp)?;
            Ok(stats)
        })();
        let ev = match result {
            Ok(stats) => FinishedEvent { job_id: job.clone(), ok: true, cancelled: false, error: None, report: serde_json::to_value(stats).ok() },
            Err(e) => {
                let cancelled = matches!(e, CoreError::Cancelled);
                // Discard partial scans entirely; nothing was touched on disk.
                let _ = state.db().delete_import(&job);
                FinishedEvent { job_id: job.clone(), ok: false, cancelled, error: Some(e.to_string()), report: None }
            }
        };
        state.finish_job(&job);
        let _ = app.emit("scan-finished", ev);
    });
    Ok(id)
}

#[tauri::command]
pub fn cancel_job(state: State<AppState>, job_id: String) -> CmdResult<bool> {
    let jobs = state.jobs.lock().map_err(err)?;
    match jobs.get(&job_id) {
        Some(flag) => {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(true)
        }
        None => Ok(false),
    }
}

// ---------------- review ----------------

#[tauri::command]
pub fn query_samples(state: State<AppState>, query: SampleQuery) -> CmdResult<SamplePage> {
    state.db().query_samples(&query).map_err(err)
}

#[tauri::command]
pub fn import_summary(state: State<AppState>, import_id: String) -> CmdResult<ImportSummary> {
    let t = state.db().settings().map_err(err)?.confidence_threshold;
    state.db().import_summary(&import_id, t).map_err(err)
}

fn folders_of(r: &SampleRow) -> Vec<String> {
    r.relative_dir.split('/').filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Manually reassign samples. When learning is on, each correction is
/// remembered as a pattern rule and applied to future classifications.
#[tauri::command]
pub fn reassign_samples(state: State<AppState>, ids: Vec<String>, category_id: String) -> CmdResult<usize> {
    let c = state.classifier();
    let idx = c.config().by_id.get(&category_id).ok_or("unknown category")?;
    let path = c.config().path_of(*idx).to_string();
    let n = state.db().reassign(&ids, &category_id, &path).map_err(err)?;
    propagate(&state, &ids, Some(&category_id)).map_err(err)?;
    if state.db().settings().map_err(err)?.learn_from_corrections {
        let mut learned = false;
        for id in &ids {
            let Some(row) = state.db().get_sample(id).map_err(err)? else { continue };
            if let Some(p) = learned::pattern_of(c.config(), &row.original_filename, &folders_of(&row)) {
                state.db().upsert_learned(&p, &category_id, &row.original_filename).map_err(err)?;
                learned = true;
            }
        }
        if learned {
            state.rebuild_classifier().map_err(err)?;
        }
    }
    Ok(n)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Similar {
    ids: Vec<String>,
    description: String,
    category_id: String,
    category_path: String,
}

/// Pending files in the same import that share a name pattern with the
/// just-corrected `ids` but are in a different category.
#[tauri::command]
pub fn similar_samples(state: State<AppState>, ids: Vec<String>) -> CmdResult<Option<Similar>> {
    let c = state.classifier();
    let db = state.db();
    let mut patterns = Vec::new();
    let mut target: Option<(String, String, String)> = None;
    for id in &ids {
        let Some(r) = db.get_sample(id).map_err(err)? else { continue };
        if let Some(p) = learned::pattern_of(c.config(), &r.original_filename, &folders_of(&r)) {
            target.get_or_insert((r.import_id.clone(), r.category_id.clone(), r.category_path.clone()));
            patterns.push((p, r.original_dir.clone()));
        }
    }
    let Some((import_id, category_id, category_path)) = target else { return Ok(None) };
    let chosen: std::collections::HashSet<&String> = ids.iter().collect();
    let rows = db.samples_for_import(&import_id).map_err(err)?;
    let matches: Vec<String> = rows
        .iter()
        .filter(|r| r.status == SampleStatus::Pending && !r.manual_override && !chosen.contains(&r.id) && r.category_id != category_id)
        .filter(|r| {
            learned::pattern_of(c.config(), &r.original_filename, &folders_of(r)).is_some_and(|p| {
                patterns.iter().any(|(q, dir)| *q == p && (p.kind == learned::RuleKind::Name || *dir == r.original_dir))
            })
        })
        .map(|r| r.id.clone())
        .collect();
    if matches.is_empty() {
        return Ok(None);
    }
    Ok(Some(Similar { ids: matches, description: patterns[0].0.display(), category_id, category_path }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedRuleView {
    #[serde(flatten)]
    rule: learned::LearnedRule,
    category_path: Option<String>,
    description: String,
}

#[tauri::command]
pub fn list_learned_rules(state: State<AppState>) -> CmdResult<Vec<LearnedRuleView>> {
    let c = state.classifier();
    let rules = state.db().learned_rules().map_err(err)?;
    Ok(rules
        .into_iter()
        .map(|r| {
            let path = c.config().by_id.get(&r.category_id).map(|&i| c.config().path_of(i).to_string());
            let description = learned::Pattern { kind: r.kind, tokens: r.tokens.clone(), scope: r.scope.clone() }.display();
            LearnedRuleView { rule: r, category_path: path, description }
        })
        .collect())
}

#[tauri::command]
pub fn delete_learned_rule(state: State<AppState>, id: i64) -> CmdResult<()> {
    state.db().delete_learned(id).map_err(err)?;
    state.rebuild_classifier().map_err(err)
}

#[tauri::command]
pub fn clear_learned_rules(state: State<AppState>) -> CmdResult<()> {
    state.db().clear_learned().map_err(err)?;
    state.rebuild_classifier().map_err(err)
}

#[tauri::command]
pub fn mark_reviewed(state: State<AppState>, ids: Vec<String>, reviewed: bool) -> CmdResult<usize> {
    let n = state.db().mark_reviewed(&ids, reviewed).map_err(err)?;
    if reviewed {
        propagate(&state, &ids, None).map_err(err)?;
    }
    if reviewed && state.db().settings().map_err(err)?.learn_from_corrections {
        let mut rows = Vec::new();
        for id in &ids {
            if let Some(r) = state.db().get_sample(id).map_err(err)? {
                rows.push(r);
            }
        }
        learn(&state, &rows, None).map_err(err)?;
    }
    Ok(n)
}

/// Apply a decision made on review-group representatives to the files in
/// their groups. `category` = None accepts each representative's category.
fn propagate(state: &AppState, ids: &[String], category: Option<&str>) -> CoreResult<usize> {
    let c = state.classifier();
    let unc_id = c.config().nodes[c.config().uncategorised].id.clone();
    let mut changed = 0;
    let mut rules = Vec::new();
    for id in ids {
        let Some(rep) = state.db().get_sample(id)? else { continue };
        let cat = category.map(String::from).unwrap_or_else(|| rep.category_id.clone());
        if rep.review_group.is_some() {
            rules.extend(review::group_rules_for(std::slice::from_ref(&rep), Some(&cat)));
        }
        if cat == unc_id {
            // Kept in Uncategorised: keep its folders like the automatic ones.
            state.db().set_subfolder(&rep.id, review::uncategorised_subfolder(&rep.relative_dir).as_deref())?;
        }
        let Some(group) = rep.review_group.clone() else { continue };
        let members = state.db().group_members(&rep.import_id, std::slice::from_ref(&group))?;
        let member_ids: Vec<String> = members.iter().map(|m| m.id.clone()).collect();
        if member_ids.is_empty() {
            continue;
        }
        rules.extend(review::group_rules_for(&members, Some(&cat)));
        let idx = c.config().by_id.get(&cat).ok_or_else(|| CoreError::Other("unknown category".into()))?;
        let path = c.config().path_of(*idx).to_string();
        changed += state.db().reassign(&member_ids, &cat, &path)?;
        if cat == unc_id {
            for m in &members {
                state.db().set_subfolder(&m.id, review::uncategorised_subfolder(&m.relative_dir).as_deref())?;
            }
        }
    }
    remember_group_decisions(state, rules)?;
    Ok(changed)
}

/// Save group decisions ("files in this folder predicted as X are Y") so
/// later scans of the same folders don't ask again.
fn remember_group_decisions(state: &AppState, rules: Vec<(learned::Pattern, String, String)>) -> CoreResult<()> {
    if rules.is_empty() || !state.db().settings()?.learn_from_corrections {
        return Ok(());
    }
    state.db().upsert_learned_many(&rules)?;
    state.rebuild_classifier()
}

/// Re-apply the review budget for an import (no-op when the budget is 0).
fn keep_within_budget(state: &AppState, import_id: &str, promote: bool) -> CoreResult<review::AutoResult> {
    let s = state.db().settings()?;
    let c = state.classifier();
    let mut db = state.db();
    review::auto_resolve_import(&mut db, c.config(), import_id, s.confidence_threshold, s.auto_accept_floor, s.review_budget, promote)
}

/// Resolve the review queue down to the budget now; `promote` brings the
/// next batch of representatives forward ("Review more").
#[tauri::command]
pub async fn auto_resolve(app: AppHandle, import_id: String, promote: bool) -> CmdResult<review::AutoResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let s = state.db().settings().map_err(err)?;
        if s.review_budget == 0 {
            return Err("Set a review budget in Settings first".to_string());
        }
        keep_within_budget(&state, &import_id, promote).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Put every automatically resolved file back into the review queue with
/// its original prediction.
#[tauri::command]
pub async fn undo_auto_resolve(app: AppHandle, import_id: String) -> CmdResult<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.db().clear_auto(&import_id).map_err(err)?;
        let t = state.db().settings().map_err(err)?.confidence_threshold;
        let classifier = state.classifier();
        let mut db = state.db();
        review::reclassify_import(&mut db, &classifier, &import_id, t).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Record confirmed decisions as learned rules and rebuild the classifier.
fn learn(state: &AppState, rows: &[SampleRow], category: Option<&str>) -> CoreResult<bool> {
    let c = state.classifier();
    let items = review::patterns_for(c.config(), rows, category);
    if items.is_empty() {
        return Ok(false);
    }
    state.db().upsert_learned_many(&items)?;
    state.rebuild_classifier()?;
    Ok(true)
}

#[tauri::command]
pub async fn review_overview(app: AppHandle, import_id: String, up_levels: Option<usize>) -> CmdResult<review::ReviewOverview> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let t = state.db().settings().map_err(err)?.confidence_threshold;
        let rows = review::pending_review(&state.db(), &import_id, t).map_err(err)?;
        let c = state.classifier();
        let unc = c.config().nodes[c.config().uncategorised].id.clone();
        Ok(review::overview_at(&rows, t, &unc, 300, up_levels.unwrap_or(0).min(3)))
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkResult {
    /// Files you decided on.
    changed: usize,
    /// Other files that no longer need review after re-classifying with
    /// what was just learned.
    resolved_automatically: usize,
    /// Files still needing review in this import.
    remaining: usize,
}

/// Apply a decision to every pending file matching `query`, learn from it,
/// then re-classify the rest of the import so similar files follow.
fn bulk(state: &AppState, query: SampleQuery, category: Option<String>) -> CoreResult<BulkResult> {
    let import_id = query.import_id.clone().ok_or_else(|| CoreError::Other("an import is required".into()))?;
    let settings = state.db().settings()?;
    let t = settings.confidence_threshold;
    let before = review::needs_review_count(&state.db(), &import_id, t)?;
    let rows: Vec<SampleRow> =
        state.db().matching_samples(&query)?.into_iter().filter(|r| r.status == SampleStatus::Pending).collect();
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    match &category {
        Some(cat) => {
            let c = state.classifier();
            let idx = c.config().by_id.get(cat).ok_or_else(|| CoreError::Other("unknown category".into()))?;
            let path = c.config().path_of(*idx).to_string();
            state.db().reassign(&ids, cat, &path)?;
        }
        None => {
            state.db().mark_reviewed(&ids, true)?;
        }
    }
    let group_rules = review::group_rules_for(&rows, category.as_deref());
    let followed = propagate(state, &ids, category.as_deref())?;
    remember_group_decisions(state, group_rules)?;
    let after_decision = review::needs_review_count(&state.db(), &import_id, t)?;
    let mut remaining = if settings.learn_from_corrections && learn(state, &rows, category.as_deref())? {
        let classifier = state.classifier();
        let mut db = state.db();
        review::reclassify_import(&mut db, &classifier, &import_id, t)?
    } else {
        after_decision
    };
    if settings.review_budget > 0 {
        keep_within_budget(state, &import_id, false)?;
        remaining = review::needs_review_count(&state.db(), &import_id, t)?;
    }
    Ok(BulkResult {
        changed: ids.len() + followed,
        resolved_automatically: after_decision.saturating_sub(remaining),
        remaining: remaining.min(before),
    })
}

#[tauri::command]
pub async fn accept_matching(app: AppHandle, query: SampleQuery) -> CmdResult<BulkResult> {
    tauri::async_runtime::spawn_blocking(move || bulk(&app.state::<AppState>(), query, None).map_err(err)).await.map_err(err)?
}

#[tauri::command]
pub async fn reassign_matching(app: AppHandle, query: SampleQuery, category_id: String) -> CmdResult<BulkResult> {
    tauri::async_runtime::spawn_blocking(move || bulk(&app.state::<AppState>(), query, Some(category_id)).map_err(err))
        .await
        .map_err(err)?
}

/// Write the review list (names only) in benchmark format.
#[tauri::command]
pub fn export_review_list(state: State<AppState>, import_id: String, path: String) -> CmdResult<usize> {
    let p = PathBuf::from(&path);
    if !p.is_absolute() {
        return Err("path must be absolute".into());
    }
    let t = state.db().settings().map_err(err)?.confidence_threshold;
    let rows = review::pending_review(&state.db(), &import_id, t).map_err(err)?;
    let c = state.classifier();
    let text = review::export_lines(&rows, &c.config().nodes[c.config().uncategorised].id);
    std::fs::write(&p, text).map_err(|e| format!("could not write {}: {e}", p.display()))?;
    Ok(rows.len())
}

#[tauri::command]
pub fn set_duplicate_decision(state: State<AppState>, ids: Vec<String>, decision: Option<DupDecision>) -> CmdResult<usize> {
    state.db().set_dup_decision(&ids, decision).map_err(err)
}

#[tauri::command]
pub fn set_source_pack(state: State<AppState>, ids: Vec<String>, pack: String) -> CmdResult<usize> {
    state.db().set_source_pack(&ids, &pack).map_err(err)
}

/// Re-run the classifier (e.g. after editing categories). Manual
/// reassignments are kept.
#[tauri::command]
pub async fn reclassify_import(app: AppHandle, import_id: String) -> CmdResult<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let rows = state.db().samples_for_import(&import_id).map_err(err)?;
        let updated = pipeline::reclassify(&rows, &state.classifier());
        state.db().update_classification(&updated).map_err(err)?;
        keep_within_budget(&state, &import_id, false).map_err(err)?;
        Ok(updated.len())
    })
    .await
    .map_err(err)?
}

/// Classify an arbitrary name without touching the disk — used by the
/// Category Manager's "test a filename" box.
#[tauri::command]
pub fn classify_preview(state: State<AppState>, relative_path: String) -> CmdResult<Classification> {
    let parts: Vec<&str> = relative_path.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let (file, folders) = parts.split_last().ok_or("enter a filename")?;
    let folders: Vec<String> = folders.iter().map(|s| s.to_string()).collect();
    let settings = state.db().settings().map_err(err)?;
    let kind = settings.file_types.detect(Path::new(file));
    let kind = match kind {
        kit_core::filetypes::FileKind::Unsupported | kit_core::filetypes::FileKind::Ignored => kit_core::filetypes::FileKind::Audio,
        k => k,
    };
    Ok(state.classifier().classify(&ClassifyInput { file_name: file, folders: &folders, kind, audio: None, hints: &[] }))
}

// ---------------- organising ----------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPreview {
    library_root: String,
    mode: OpMode,
    summary: PlanSummary,
    /// First transfers, for a quick look at destinations.
    transfers: Vec<PlannedOp>,
    renamed: Vec<PlannedOp>,
    skipped: Vec<PlannedOp>,
    rerouted: Vec<PlannedOp>,
    available_bytes: Option<u64>,
}

fn make_plan(state: &AppState, import_id: &str, output_dir: &str, mode: OpMode) -> CoreResult<Plan> {
    let settings = state.db().settings()?;
    let classifier = state.classifier();
    let cfg = classifier.config();
    let imp = state.db().get_import(import_id)?.ok_or_else(|| CoreError::Other("import not found".into()))?;
    let sources: Vec<PathBuf> = imp.sources.iter().map(PathBuf::from).collect();
    let library = organiser::validate_output(Path::new(output_dir), &cfg.source.root_folder, &sources)?;
    let samples = state.db().samples_for_import(import_id)?;
    let unc = &cfg.nodes[cfg.uncategorised];
    organiser::build_plan(
        &samples,
        &PlanOptions {
            library_root: &library,
            mode,
            collision: settings.collision_handling,
            low_confidence: settings.low_confidence_action,
            threshold: settings.confidence_threshold,
            uncategorised: (&unc.id, &unc.path),
        },
    )
}

#[tauri::command]
pub async fn preview_plan(app: AppHandle, import_id: String, output_dir: String, mode: OpMode) -> CmdResult<PlanPreview> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let plan = make_plan(&state, &import_id, &output_dir, mode).map_err(err)?;
        let pick = |f: &dyn Fn(&PlannedOp) -> bool| plan.ops.iter().filter(|o| f(o)).take(500).cloned().collect::<Vec<_>>();
        Ok(PlanPreview {
            library_root: plan.library_root.clone(),
            mode: plan.mode,
            available_bytes: organiser::fsops::available_space(Path::new(&plan.library_root)),
            transfers: pick(&|o| o.action != PlannedAction::Skip),
            renamed: pick(&|o| o.renamed_from.is_some()),
            skipped: pick(&|o| o.action == PlannedAction::Skip),
            rerouted: pick(&|o| o.action != PlannedAction::Skip && o.reason.is_some()),
            summary: plan.summary.clone(),
        })
    })
    .await
    .map_err(err)?
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrganiseProgressEvent<'a> {
    job_id: &'a str,
    #[serde(flatten)]
    progress: &'a organiser::ExecProgress,
}

/// Start organising. Move mode requires `confirm_move = true`, set only
/// after the user confirms in the UI.
#[tauri::command]
pub fn organise(
    app: AppHandle,
    state: State<AppState>,
    import_id: String,
    output_dir: String,
    mode: OpMode,
    confirm_move: bool,
) -> CmdResult<String> {
    if mode == OpMode::Move && !confirm_move {
        return Err("move mode must be confirmed".into());
    }
    if state.job_running(&import_id) {
        return Err("this import is already being processed".into());
    }
    // Plan synchronously so validation errors surface immediately.
    let plan = make_plan(&state, &import_id, &output_dir, mode).map_err(err)?;
    if plan.summary.undecided_duplicates > 0 {
        return Err(format!("{} duplicate file(s) need a keep/skip decision first", plan.summary.undecided_duplicates));
    }
    {
        let db = state.db();
        if let Some(mut imp) = db.get_import(&import_id).map_err(err)? {
            imp.output_dir = output_dir.clone();
            db.update_import(&imp).map_err(err)?;
        }
    }
    let cancel = state.register_job(&import_id);
    let job = import_id.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let emit = |p: &organiser::ExecProgress| {
            let _ = app.emit("organise-progress", OrganiseProgressEvent { job_id: &job, progress: p });
        };
        let result: CoreResult<ExecReport> = organiser::execute(&state.db, &job, &plan, &cancel, &emit);
        state.finish_job(&job);
        let ev = match result {
            Ok(rep) => FinishedEvent {
                job_id: job.clone(),
                ok: true,
                cancelled: rep.cancelled,
                error: None,
                report: serde_json::to_value(&rep).ok(),
            },
            Err(e) => {
                // Planning-stage failure (e.g. disk space): nothing was moved.
                if let Ok(Some(mut imp)) = state.db().get_import(&job) {
                    if imp.status == ImportStatus::Organising {
                        imp.status = ImportStatus::Review;
                        let _ = state.db().update_import(&imp);
                    }
                }
                FinishedEvent { job_id: job.clone(), ok: false, cancelled: false, error: Some(e.to_string()), report: None }
            }
        };
        let _ = app.emit("organise-finished", ev);
    });
    Ok(import_id)
}

#[tauri::command]
pub async fn undo_import(app: AppHandle, import_id: String) -> CmdResult<UndoReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if state.job_running(&import_id) {
            return Err("this import is still running".to_string());
        }
        organiser::undo_import(&state.db, &import_id).map_err(err)
    })
    .await
    .map_err(err)?
}

// ---------------- history, dashboard, library ----------------

#[tauri::command]
pub fn list_imports(state: State<AppState>, limit: Option<u32>) -> CmdResult<Vec<ImportRecord>> {
    state.db().list_imports(limit.unwrap_or(100)).map_err(err)
}

#[tauri::command]
pub fn get_import(state: State<AppState>, import_id: String) -> CmdResult<Option<ImportRecord>> {
    state.db().get_import(&import_id).map_err(err)
}

#[tauri::command]
pub fn delete_import(state: State<AppState>, import_id: String) -> CmdResult<()> {
    if state.job_running(&import_id) {
        return Err("this import is still running".into());
    }
    state.db().delete_import(&import_id).map_err(err)
}

#[tauri::command]
pub fn import_operations(state: State<AppState>, import_id: String) -> CmdResult<Vec<OperationRow>> {
    state.db().operations_for_import(&import_id).map_err(err)
}

#[tauri::command]
pub fn dashboard_stats(state: State<AppState>) -> CmdResult<DashboardStats> {
    let t = state.db().settings().map_err(err)?.confidence_threshold;
    state.db().dashboard(t).map_err(err)
}

#[tauri::command]
pub fn library_tree(state: State<AppState>) -> CmdResult<Vec<(String, u64)>> {
    state.db().library_tree().map_err(err)
}

#[tauri::command]
pub fn source_packs(state: State<AppState>, import_id: Option<String>) -> CmdResult<Vec<(String, u64)>> {
    state.db().source_packs(import_id.as_deref()).map_err(err)
}

#[tauri::command]
pub async fn audio_info(app: AppHandle, path: String) -> CmdResult<AudioMeta> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if !state.db().is_known_path(&path).map_err(err)? {
            return Err("not a known sample".to_string());
        }
        Ok(kit_core::audio::probe(Path::new(&path)))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn recovery_messages(state: State<AppState>) -> CmdResult<Vec<String>> {
    Ok(std::mem::take(&mut *state.recovery.lock().map_err(err)?))
}
