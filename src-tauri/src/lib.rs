//! Tauri shell for Kit Organizer. All heavy lifting lives in `kit-core`;
//! this crate wires it to IPC commands, background jobs, progress events
//! and the audio preview protocol.

mod commands;
mod protocol;
mod state;

use kit_core::db::Database;
use state::AppState;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("kitaudio", protocol::handle)
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let db = Database::open(&dir.join("kit-organizer.db"))?;
            let db = Arc::new(Mutex::new(db));
            // Reconcile anything interrupted by a crash or power loss before
            // the UI can start new work.
            let recovery = kit_core::organiser::recover(&db).unwrap_or_else(|e| vec![format!("Recovery check failed: {e}")]);
            let state = AppState::new(db, recovery)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_category_config,
            commands::save_category_config,
            commands::reset_category_config,
            commands::category_usage,
            commands::export_config,
            commands::import_config,
            commands::start_scan,
            commands::cancel_job,
            commands::query_samples,
            commands::import_summary,
            commands::reassign_samples,
            commands::similar_samples,
            commands::list_learned_rules,
            commands::delete_learned_rule,
            commands::clear_learned_rules,
            commands::mark_reviewed,
            commands::set_duplicate_decision,
            commands::set_source_pack,
            commands::reclassify_import,
            commands::preview_plan,
            commands::organise,
            commands::undo_import,
            commands::list_imports,
            commands::get_import,
            commands::delete_import,
            commands::import_operations,
            commands::dashboard_stats,
            commands::library_tree,
            commands::source_packs,
            commands::audio_info,
            commands::recovery_messages,
            commands::classify_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kit Organizer");
}
