//! End-to-end filesystem tests using temporary directories and generated
//! files.

use super::*;
use crate::classifier::Classifier;
use crate::config::{default_config, CompiledConfig};
use crate::db::{Database, SampleQuery};
use crate::filetypes::FileTypeSettings;
use crate::models::*;
use crate::pipeline::{analyse, build_rows, AnalyseOptions};
use crate::scanner::{scan, ScanOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

fn write(p: &Path, content: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

struct Env {
    _tmp: tempfile::TempDir,
    src: PathBuf,
    out: PathBuf,
    db: Mutex<Database>,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Packs").join("Test Pack");
    let out = tmp.path().join("Output");
    fs::create_dir_all(&out).unwrap();
    write(&src.join("Drums/Kicks/Kick 01.wav"), b"kick-a");
    write(&src.join("Drums/Kicks/Kick_Hard.wav"), b"kick-b");
    write(&src.join("Other/Kick 01.wav"), b"kick-c"); // same name, different content
    write(&src.join("Drums/Snares/Snare_01.wav"), b"snare");
    write(&src.join("Copies/Snare Copy.wav"), b"snare"); // same content, different name
    write(&src.join("Bass/Growl_F.wav"), b"growl");
    write(&src.join("MIDI/chords.mid"), b"midi");
    write(&src.join("readme.txt"), b"text");
    Env { _tmp: tmp, src, out, db: Mutex::new(Database::open_in_memory().unwrap()) }
}

fn import(e: &Env, dup: DuplicateHandling) -> String {
    let types = FileTypeSettings::default();
    let rep = scan(&ScanOptions { roots: std::slice::from_ref(&e.src), exclude: std::slice::from_ref(&e.out), file_types: &types, skip_daw_projects: true }, &AtomicBool::new(false), &|_| {})
        .unwrap();
    assert_eq!(rep.unsupported, 1);
    let c = Classifier::new(CompiledConfig::compile(&default_config()).unwrap());
    let analysed = analyse(rep.files, &c, &AnalyseOptions { threads: 2, analyse_audio: false }, &AtomicBool::new(false), &|_| {}).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut db = e.db.lock().unwrap();
    db.create_import(&ImportRecord {
        id: id.clone(),
        created_at: crate::db::now(),
        completed_at: None,
        sources: vec![e.src.display().to_string()],
        output_dir: e.out.display().to_string(),
        library_root: String::new(),
        mode: OpMode::Copy,
        status: ImportStatus::Review,
        stats: ImportStats::default(),
        messages: vec![],
    })
    .unwrap();
    let rows = build_rows(&id, analysed, &db, dup).unwrap();
    db.insert_samples(&rows).unwrap();
    id
}

fn plan_for(e: &Env, id: &str, mode: OpMode) -> Plan {
    let lib = validate_output(&e.out, "Organised Samples", std::slice::from_ref(&e.src)).unwrap();
    let samples = e.db.lock().unwrap().samples_for_import(id).unwrap();
    build_plan(
        &samples,
        &PlanOptions {
            library_root: &lib,
            mode,
            collision: CollisionHandling::Rename,
            low_confidence: LowConfidenceAction::Predicted,
            threshold: 0.6,
            uncategorised: ("uncategorised", "Uncategorised"),
        },
    )
    .unwrap()
}

fn lib(e: &Env) -> PathBuf {
    e.out.join("Organised Samples")
}

#[test]
fn plan_does_not_touch_filesystem() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Copy);
    assert!(!lib(&e).exists());
    assert_eq!(p.summary.skipped_duplicates, 1);
    assert_eq!(p.summary.renamed, 1);
    assert!(p.ops.iter().any(|o| o.dest_name == "Kick 01 (2).wav"));
}

#[test]
fn copy_mode_leaves_originals_and_handles_collisions() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Copy);
    let rep = execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!(rep.failed, 0, "{:?}", rep.errors);
    assert_eq!(rep.copied, 6);
    assert_eq!(rep.skipped, 1); // duplicate snare
    let l = lib(&e);
    assert_eq!(fs::read(l.join("Kicks/Kick 01.wav")).unwrap().len(), 6);
    assert!(l.join("Kicks/Kick 01 (2).wav").exists());
    assert!(l.join("Bass/Growls/Growl_F.wav").exists());
    assert!(l.join("MIDI/chords.mid").exists());
    assert!(!l.join("Uncategorised").exists(), "folders are only created when used");
    // Originals untouched; no temp files left behind.
    assert!(e.src.join("Drums/Kicks/Kick 01.wav").exists());
    assert!(e.src.join("readme.txt").exists());
    let leftovers: Vec<_> = walkdir::WalkDir::new(&l)
        .into_iter()
        .filter_map(|x| x.ok())
        .filter(|x| x.file_name().to_string_lossy().starts_with(".kitorg"))
        .collect();
    assert!(leftovers.is_empty());
    let imp = e.db.lock().unwrap().get_import(&id).unwrap().unwrap();
    assert_eq!(imp.status, ImportStatus::Completed);
    assert_eq!(imp.stats.copied, 6);
}

#[test]
fn duplicates_kept_when_requested() {
    let e = env();
    let id = import(&e, DuplicateHandling::Keep);
    let p = plan_for(&e, &id, OpMode::Copy);
    assert_eq!(p.summary.kept_duplicates, 1);
    let rep = execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!(rep.copied, 7);
}

#[test]
fn ask_mode_blocks_until_decided() {
    let e = env();
    let id = import(&e, DuplicateHandling::Ask);
    let p = plan_for(&e, &id, OpMode::Copy);
    assert_eq!(p.summary.undecided_duplicates, 1);
    assert!(execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).is_err());
    let dup_ids: Vec<String> = {
        let db = e.db.lock().unwrap();
        db.query_samples(&SampleQuery { import_id: Some(id.clone()), duplicates_only: true, ..Default::default() })
            .unwrap()
            .rows
            .into_iter()
            .map(|r| r.id)
            .collect()
    };
    assert_eq!(dup_ids.len(), 1);
    e.db.lock().unwrap().set_dup_decision(&dup_ids, Some(DupDecision::Skip)).unwrap();
    let p = plan_for(&e, &id, OpMode::Copy);
    assert!(execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).is_ok());
}

#[test]
fn never_overwrites_existing_files() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    // The first copy of the snare content (by path) is "Copies/Snare Copy.wav".
    write(&lib(&e).join("Snares/Snare Copy.wav"), b"precious");
    let p = plan_for(&e, &id, OpMode::Copy);
    execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!(fs::read(lib(&e).join("Snares/Snare Copy.wav")).unwrap(), b"precious");
    assert_eq!(fs::read(lib(&e).join("Snares/Snare Copy (2).wav")).unwrap(), b"snare");
}

#[test]
fn claim_name_never_replaces() {
    let d = tempfile::tempdir().unwrap();
    let desired = d.path().join("a.wav");
    fs::write(&desired, b"old").unwrap();
    let temp = d.path().join("t.part");
    fs::write(&temp, b"new").unwrap();
    let got = fsops::claim_name(&temp, &desired).unwrap();
    assert_eq!(got, d.path().join("a (2).wav"));
    assert_eq!(fs::read(&desired).unwrap(), b"old");
}

#[test]
fn move_mode_moves_and_verifies() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Move);
    let rep = execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!(rep.moved, 6, "{:?}", rep.errors);
    assert!(!e.src.join("Drums/Kicks/Kick 01.wav").exists());
    assert_eq!(fs::read(lib(&e).join("Kicks/Kick 01.wav")).unwrap(), b"kick-a");
    // Skipped duplicate and unsupported files stay; source folders are kept.
    assert!(e.src.join("Drums/Snares/Snare_01.wav").exists());
    assert!(e.src.join("readme.txt").exists());
    assert!(e.src.join("Drums/Kicks").is_dir());
}

#[test]
fn cancellation_leaves_samples_pending() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Copy);
    let rep = execute(&e.db, &id, &p, &AtomicBool::new(true), &|_| {}).unwrap();
    assert!(rep.cancelled);
    assert_eq!(rep.copied, 0);
    assert!(!lib(&e).join("Kicks").exists());
    let s = e.db.lock().unwrap().import_summary(&id, 0.6).unwrap();
    assert_eq!(s.pending, 6);
    assert_eq!(e.db.lock().unwrap().get_import(&id).unwrap().unwrap().status, ImportStatus::Cancelled);
}

#[test]
fn missing_source_is_reported() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    fs::remove_file(e.src.join("Bass/Growl_F.wav")).unwrap();
    let p = plan_for(&e, &id, OpMode::Copy);
    assert_eq!(p.summary.missing, 1);
    let rep = execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!(rep.failed, 1);
    assert_eq!(rep.copied, 5);
}

#[test]
fn source_changed_after_scan_fails_verification() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    fs::write(e.src.join("Bass/Growl_F.wav"), b"changed!").unwrap();
    let p = plan_for(&e, &id, OpMode::Copy);
    let rep = execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!(rep.failed, 1);
    assert!(rep.errors[0].message.contains("verification failed"));
    let growls = lib(&e).join("Bass/Growls");
    assert!(!growls.join("Growl_F.wav").exists());
    let leftovers = fs::read_dir(&growls).map(|d| d.count()).unwrap_or(0);
    assert_eq!(leftovers, 0, "temp file must be cleaned up");
}

#[test]
fn undo_copy_removes_only_unmodified_files() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Copy);
    execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    fs::write(lib(&e).join("Bass/Growls/Growl_F.wav"), b"edited by user").unwrap();
    let rep = undo_import(&e.db, &id).unwrap();
    assert_eq!(rep.reverted, 5);
    assert_eq!(rep.skipped.len(), 1);
    assert!(lib(&e).join("Bass/Growls/Growl_F.wav").exists());
    assert!(!lib(&e).join("Kicks").exists(), "empty folders created by the run are removed");
    assert!(e.src.join("Drums/Kicks/Kick 01.wav").exists());
    assert_eq!(e.db.lock().unwrap().get_import(&id).unwrap().unwrap().status, ImportStatus::PartiallyUndone);
}

#[test]
fn undo_move_restores_originals_without_overwriting() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Move);
    execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    // Something new now occupies one original location.
    write(&e.src.join("Bass/Growl_F.wav"), b"new file");
    let rep = undo_import(&e.db, &id).unwrap();
    assert_eq!(rep.reverted, 5);
    assert_eq!(rep.skipped.len(), 1);
    assert_eq!(fs::read(e.src.join("Drums/Kicks/Kick 01.wav")).unwrap(), b"kick-a");
    assert_eq!(fs::read(e.src.join("Other/Kick 01.wav")).unwrap(), b"kick-c");
    assert_eq!(fs::read(e.src.join("Bass/Growl_F.wav")).unwrap(), b"new file");
    assert_eq!(fs::read(lib(&e).join("Bass/Growls/Growl_F.wav")).unwrap(), b"growl");
}

#[test]
fn recovers_interrupted_operations() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Copy);
    // Simulate a crash: journal written, one file finished but not marked
    // done, one temp file left behind, the rest never started.
    let transfers: Vec<_> = p.ops.iter().filter(|o| o.action != PlannedAction::Skip).collect();
    let rows: Vec<crate::db::OperationRow> = transfers
        .iter()
        .map(|o| crate::db::OperationRow {
            id: 0,
            import_id: id.clone(),
            sample_id: o.sample_id.clone(),
            op: "copy".into(),
            src: o.src.clone(),
            dest: o.dest_path.clone(),
            temp: None,
            hash: o.hash.clone(),
            created_dirs: vec![],
            state: "planned".into(),
            error: None,
        })
        .collect();
    let ids = e.db.lock().unwrap().insert_operations(&rows).unwrap();
    {
        let db = e.db.lock().unwrap();
        let mut imp = db.get_import(&id).unwrap().unwrap();
        imp.status = ImportStatus::Organising;
        db.update_import(&imp).unwrap();
        // op 0: completed copy
        let mut r0 = rows[0].clone();
        r0.id = ids[0];
        r0.state = "in_progress".into();
        write(Path::new(&r0.dest), &fs::read(&r0.src).unwrap());
        db.update_operation(&r0).unwrap();
        // op 1: half-written temp
        let mut r1 = rows[1].clone();
        r1.id = ids[1];
        r1.state = "in_progress".into();
        let temp = fsops::temp_path_for(Path::new(&r1.dest));
        write(&temp, b"partial");
        r1.temp = Some(temp.display().to_string());
        db.update_operation(&r1).unwrap();
    }
    let msgs = recover(&e.db).unwrap();
    assert_eq!(msgs.len(), 1);
    let db = e.db.lock().unwrap();
    let ops = db.operations_for_import(&id).unwrap();
    assert_eq!(ops.iter().filter(|o| o.state == "done").count(), 1);
    assert!(ops.iter().all(|o| o.state == "done" || o.state == "cancelled"));
    assert!(!Path::new(ops[1].temp.as_deref().unwrap_or("/nonexistent")).exists());
    assert_eq!(db.get_import(&id).unwrap().unwrap().status, ImportStatus::Interrupted);
    let s = db.import_summary(&id, 0.6).unwrap();
    assert_eq!(s.organised, 1);
}

#[test]
fn rejects_unwritable_output() {
    let d = tempfile::tempdir().unwrap();
    assert!(validate_output(Path::new("relative/path"), "Organised Samples", &[]).is_err());
    assert!(validate_output(&d.path().join("missing/deeper"), "Organised Samples", &[]).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let ro = d.path().join("ro");
        fs::create_dir(&ro).unwrap();
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
        let writable_anyway = fs::write(ro.join("probe"), b"").is_ok(); // root ignores modes
        if !writable_anyway {
            assert!(validate_output(&ro, "Organised Samples", &[]).is_err());
        }
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn library_duplicates_detected_on_reimport() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Copy);
    execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    let id2 = import(&e, DuplicateHandling::Skip);
    let s = e.db.lock().unwrap().import_summary(&id2, 0.6).unwrap();
    assert_eq!(s.duplicates, 7, "every file is already in the library");
}

#[test]
fn rerouted_low_confidence_files_record_their_real_category() {
    let e = env();
    write(&e.src.join("Misc/Mystery.wav"), b"mystery");
    write(&e.src.join("Bass/Neuro_Growl.wav"), b"ng"); // ~46%: below threshold
    let id = import(&e, DuplicateHandling::Skip);
    let lib_root = validate_output(&e.out, "Organised Samples", std::slice::from_ref(&e.src)).unwrap();
    let samples = e.db.lock().unwrap().samples_for_import(&id).unwrap();
    let p = build_plan(
        &samples,
        &PlanOptions {
            library_root: &lib_root,
            mode: OpMode::Copy,
            collision: CollisionHandling::Rename,
            low_confidence: LowConfidenceAction::Uncategorised,
            threshold: 0.6,
            uncategorised: ("uncategorised", "Uncategorised"),
        },
    )
    .unwrap();
    assert!(p.summary.low_confidence_rerouted >= 1);
    execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    assert!(lib(&e).join("Uncategorised/Neuro_Growl.wav").exists());
    let db = e.db.lock().unwrap();
    let row = db.samples_for_import(&id).unwrap().into_iter().find(|r| r.original_filename == "Neuro_Growl.wav").unwrap();
    assert_eq!(row.category_path, "Uncategorised");
    assert!(row.evidence.last().unwrap().contains("predicted Bass/Growls"));
    let tree = db.library_tree().unwrap();
    assert!(tree.iter().any(|(p, _)| p == "Uncategorised"));
}

#[test]
fn construction_kits_get_their_own_folder() {
    let e = env();
    write(&e.src.join("VFH3 128BPM Shakin Kit/VFH3 128BPM Shakin Kit 5.wav"), b"part5");
    write(&e.src.join("VFH3 128BPM Shakin Kit/Unlooped/VFH3 128BPM Shakin Lead.wav"), b"lead");
    let id = import(&e, DuplicateHandling::Skip);
    let p = plan_for(&e, &id, OpMode::Copy);
    execute(&e.db, &id, &p, &AtomicBool::new(false), &|_| {}).unwrap();
    let kit = lib(&e).join("Construction Kits/VFH3 128BPM Shakin Kit");
    assert!(kit.join("VFH3 128BPM Shakin Kit 5.wav").exists());
    assert!(kit.join("Unlooped/VFH3 128BPM Shakin Lead.wav").exists());
}

#[test]
fn moving_files_into_kits_uses_their_folder_name() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let mut db = e.db.lock().unwrap();
    let rows = db.samples_for_import(&id).unwrap();
    let growl = rows.iter().find(|r| r.original_filename == "Growl_F.wav").unwrap().id.clone();
    db.reassign(std::slice::from_ref(&growl), "construction-kits", "Construction Kits").unwrap();
    assert_eq!(db.get_sample(&growl).unwrap().unwrap().subfolder.as_deref(), Some("Bass"));
    db.reassign(std::slice::from_ref(&growl), "kicks", "Kicks").unwrap();
    assert_eq!(db.get_sample(&growl).unwrap().unwrap().subfolder, None);
}

#[test]
fn unsafe_kit_folder_names_are_sanitised() {
    assert_eq!(plan::safe_segment("Kit: A/B?"), "Kit_ A_B_");
    assert_eq!(plan::safe_segment(".."), "");
    assert_eq!(plan::safe_segment("Name. "), "Name");
}

#[test]
fn manual_decisions_clear_the_auto_flag() {
    let e = env();
    let id = import(&e, DuplicateHandling::Skip);
    let mut db = e.db.lock().unwrap();
    let rows = db.samples_for_import(&id).unwrap();
    let cfg = crate::config::CompiledConfig::compile(&default_config()).unwrap();
    let (ups, _) = crate::review::auto_resolve(&rows, &cfg, 0.99, 0.35, 1, false);
    db.apply_auto(&ups).unwrap();
    let auto: Vec<String> = db.samples_for_import(&id).unwrap().into_iter().filter(|r| r.auto_resolved).map(|r| r.id).collect();
    assert!(!auto.is_empty());
    db.reassign(&auto, "kicks", "Kicks").unwrap();
    assert!(db.samples_for_import(&id).unwrap().iter().all(|r| !r.auto_resolved));
}
