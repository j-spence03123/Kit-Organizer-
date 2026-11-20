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
use kit_core::classifier::{Classification, Classifier, ClassifyInput};
use kit_core::config::{default_config, CategoryConfig, CompiledConfig};
use kit_core::db::{now, DashboardStats, ImportSummary, OperationRow, SamplePage, SampleQuery};
use kit_core::error::CoreError;
use kit_core::models::*;
use kit_core::organiser::{self, ExecReport, Plan, PlanOptions, PlanSummary, PlannedAction, PlannedOp, UndoReport};
use kit_core::scanner::{scan, ScanOptions, ScanProgress};
use kit_core::{pipeline, CoreResult};
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
    state.set_classifier(Classifier::new(compiled));
    Ok(())
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
    };
    let json = serde_json::to_string_pretty(&out).map_err(err)?;
    std::fs::write(&p, json).map_err(|e| format!("could not write {}: {e}", p.display()))
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
            let rep = scan(&ScanOptions { roots: &roots, exclude: &exclude, file_types: &settings.file_types }, &cancel, &emit)?;
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
            let analysed = pipeline::analyse(rep.files, &classifier, settings.threads(), &cancel, &emit)?;
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

#[tauri::command]
pub fn reassign_samples(state: State<AppState>, ids: Vec<String>, category_id: String) -> CmdResult<usize> {
    let c = state.classifier();
    let idx = c.config().by_id.get(&category_id).ok_or("unknown category")?;
    let path = c.config().path_of(*idx).to_string();
    state.db().reassign(&ids, &category_id, &path).map_err(err)
}

#[tauri::command]
pub fn mark_reviewed(state: State<AppState>, ids: Vec<String>, reviewed: bool) -> CmdResult<usize> {
    state.db().mark_reviewed(&ids, reviewed).map_err(err)
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
    Ok(state.classifier().classify(&ClassifyInput { file_name: file, folders: &folders, kind, audio: None }))
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
