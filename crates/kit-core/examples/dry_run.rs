//! Developer tool: scan and classify folders from the command line without
//! touching any files. Prints the proposed destination for every sample.
//!
//! ```text
//! cargo run -p kit-core --example dry_run -- <folder>... [--db <path>]
//! ```
//!
//! With `--db`, results are also stored as a new import in that database
//! (for example the app's own database) so they can be reviewed in the UI.

use kit_core::classifier::Classifier;
use kit_core::config::CompiledConfig;
use kit_core::db::{now, Database};
use kit_core::models::*;
use kit_core::pipeline::{analyse, build_rows, AnalyseOptions};
use kit_core::scanner::{scan, ScanOptions};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let db_path = args.iter().position(|a| a == "--db").map(|i| {
        args.remove(i);
        PathBuf::from(args.remove(i))
    });
    if args.is_empty() {
        eprintln!("usage: dry_run <folder>... [--db <path>]");
        std::process::exit(2);
    }
    let roots: Vec<PathBuf> = args.iter().map(|a| std::fs::canonicalize(a).unwrap_or_else(|_| PathBuf::from(a))).collect();
    let mut db = match &db_path {
        Some(p) => Database::open(p)?,
        None => Database::open_in_memory()?,
    };
    let settings = db.settings()?;
    // Same classifier as the app: saved categories plus learned rules.
    let classifier = Classifier::new(CompiledConfig::compile(&db.category_config()?)?).with_learned(db.learned_rules()?);
    let cancel = AtomicBool::new(false);
    let rep = scan(&ScanOptions { roots: &roots, exclude: &[], file_types: &settings.file_types, skip_daw_projects: settings.skip_daw_projects }, &cancel, &|_| {})?;
    let (total, unsupported) = (rep.total, rep.unsupported);
    let opts = AnalyseOptions { threads: settings.threads(), analyse_audio: settings.analyse_audio };
    let analysed = analyse(rep.files, &classifier, &opts, &cancel, &|_| {})?;
    let id = uuid::Uuid::new_v4().to_string();
    db.create_import(&ImportRecord {
        id: id.clone(),
        created_at: now(),
        completed_at: None,
        sources: roots.iter().map(|r| r.display().to_string()).collect(),
        output_dir: settings.default_output_dir.clone().unwrap_or_default(),
        library_root: String::new(),
        mode: settings.default_mode,
        status: ImportStatus::Review,
        stats: ImportStats { files_discovered: total, supported: analysed.len() as u64, unsupported, ..Default::default() },
        messages: vec![],
    })?;
    let rows = build_rows(&id, analysed, &db, settings.duplicate_handling)?;
    for r in &rows {
        println!(
            "{:>4.0}%  {:<9} {:<38} <- {}{}",
            r.confidence * 100.0,
            r.sample_type.as_str(),
            r.category_path,
            r.relative_dir.split('/').skip(1).chain([r.original_filename.as_str()]).collect::<Vec<_>>().join("/"),
            if r.duplicate_of.is_some() { "  [duplicate]" } else { "" }
        );
    }
    let duplicates = rows.iter().filter(|r| r.duplicate_of.is_some()).count() as u64;
    if let Some(mut imp) = db.get_import(&id)? {
        imp.stats.duplicates = duplicates;
        db.update_import(&imp)?;
    }
    db.insert_samples(&rows)?;
    eprintln!("{} files, {} supported, {} unsupported{}", total, rows.len(), unsupported, if db_path.is_some() { format!(", saved as import {id}") } else { String::new() });
    Ok(())
}
