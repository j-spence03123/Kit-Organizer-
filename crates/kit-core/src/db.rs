//! SQLite persistence: settings, category configuration, imports, samples
//! (with provenance) and the organisation operation journal.

use crate::audio::AudioMeta;
use crate::classifier::SampleType;
use crate::config::{default_config, CategoryConfig};
use crate::error::{CoreError, CoreResult};
use crate::filetypes::FileKind;
use crate::models::*;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS imports (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    sources TEXT NOT NULL,
    output_dir TEXT NOT NULL,
    library_root TEXT NOT NULL,
    mode TEXT NOT NULL,
    status TEXT NOT NULL,
    stats TEXT NOT NULL,
    messages TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS samples (
    id TEXT PRIMARY KEY,
    import_id TEXT NOT NULL REFERENCES imports(id) ON DELETE CASCADE,
    original_path TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    original_dir TEXT NOT NULL,
    relative_dir TEXT NOT NULL,
    source_pack TEXT NOT NULL,
    size INTEGER NOT NULL,
    hash TEXT,
    file_kind TEXT NOT NULL,
    category_id TEXT NOT NULL,
    category_path TEXT NOT NULL,
    confidence REAL NOT NULL,
    evidence TEXT NOT NULL,
    alternatives TEXT NOT NULL,
    sample_type TEXT NOT NULL,
    bpm REAL,
    bpm_source TEXT,
    musical_key TEXT,
    method TEXT NOT NULL,
    audio TEXT,
    manual_override INTEGER NOT NULL DEFAULT 0,
    reviewed INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    duplicate_of TEXT,
    duplicate_path TEXT,
    dup_decision TEXT,
    destination_path TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_samples_import ON samples(import_id);
CREATE INDEX IF NOT EXISTS idx_samples_hash ON samples(hash);
CREATE INDEX IF NOT EXISTS idx_samples_status ON samples(status);
CREATE INDEX IF NOT EXISTS idx_samples_category ON samples(category_path);
CREATE TABLE IF NOT EXISTS operations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    import_id TEXT NOT NULL REFERENCES imports(id) ON DELETE CASCADE,
    sample_id TEXT NOT NULL,
    op TEXT NOT NULL,
    src TEXT NOT NULL,
    dest TEXT NOT NULL,
    temp TEXT,
    hash TEXT,
    created_dirs TEXT NOT NULL DEFAULT '[]',
    state TEXT NOT NULL,
    error TEXT,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ops_import ON operations(import_id);
CREATE TABLE IF NOT EXISTS learned_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    tokens TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT '',
    category_id TEXT NOT NULL,
    example TEXT NOT NULL,
    hits INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(kind, tokens, scope)
);
"#;

pub struct Database {
    conn: Connection,
}

/// Columns accepted for sorting (whitelisted to keep SQL injection-free).
fn sort_column(s: &str) -> &'static str {
    match s {
        "filename" => "original_filename COLLATE NOCASE",
        "folder" => "relative_dir COLLATE NOCASE",
        "category" => "category_path COLLATE NOCASE",
        "confidence" => "confidence",
        "status" => "status",
        "pack" => "source_pack COLLATE NOCASE",
        "size" => "size",
        _ => "original_path COLLATE NOCASE",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SampleQuery {
    pub import_id: Option<String>,
    pub statuses: Vec<String>,
    pub search: Option<String>,
    /// Category path prefix ("Bass" matches "Bass/Growls").
    pub category: Option<String>,
    pub min_confidence: Option<f32>,
    pub max_confidence: Option<f32>,
    /// Only unreviewed samples below this threshold.
    pub needs_review_below: Option<f32>,
    pub duplicates_only: bool,
    pub source_pack: Option<String>,
    pub sort: Option<String>,
    pub descending: bool,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplePage {
    pub rows: Vec<SampleRow>,
    pub total: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub total: u64,
    pub pending: u64,
    pub needs_review: u64,
    pub duplicates: u64,
    pub duplicates_undecided: u64,
    pub organised: u64,
    pub skipped: u64,
    pub failed: u64,
    pub by_category: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub total_imported: u64,
    pub organised: u64,
    pub awaiting_review: u64,
    pub duplicates: u64,
    pub imports: u64,
    pub last_output_dir: Option<String>,
}

#[cfg(test)]
mod learned_tests {
    use super::*;
    use crate::classifier::learned::{Pattern, RuleKind};

    #[test]
    fn upsert_replaces_and_counts() {
        let db = Database::open_in_memory().unwrap();
        let p = Pattern { kind: RuleKind::Name, tokens: vec!["shot".into()], scope: Some("xlnt".into()) };
        db.upsert_learned(&p, "bass/bass-stabs", "XLNT_Shot_01.wav").unwrap();
        db.upsert_learned(&p, "bass/bass-stabs", "XLNT_Shot_02.wav").unwrap();
        let r = db.learned_rules().unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].hits, 2);
        db.upsert_learned(&p, "kicks", "XLNT_Shot_03.wav").unwrap();
        let r = db.learned_rules().unwrap();
        assert_eq!((r[0].category_id.as_str(), r[0].hits), ("kicks", 1));
        db.delete_learned(r[0].id).unwrap();
        assert!(db.learned_rules().unwrap().is_empty());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRow {
    pub id: i64,
    pub import_id: String,
    pub sample_id: String,
    pub op: String,
    pub src: String,
    pub dest: String,
    pub temp: Option<String>,
    pub hash: Option<String>,
    pub created_dirs: Vec<String>,
    pub state: String,
    pub error: Option<String>,
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

fn from_json<T: for<'de> Deserialize<'de> + Default>(s: Option<String>) -> T {
    s.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

const SAMPLE_COLS: &str = "id, import_id, original_path, original_filename, original_dir, relative_dir, source_pack, size, hash, \
    file_kind, category_id, category_path, confidence, evidence, alternatives, sample_type, bpm, bpm_source, musical_key, method, \
    audio, manual_override, reviewed, status, duplicate_of, duplicate_path, dup_decision, destination_path, error, created_at, updated_at, key_source";

fn sample_from_row(r: &Row) -> rusqlite::Result<SampleRow> {
    let kind: String = r.get(9)?;
    let st: String = r.get(15)?;
    let status: String = r.get(23)?;
    let dup: Option<String> = r.get(26)?;
    let audio: Option<String> = r.get(20)?;
    Ok(SampleRow {
        id: r.get(0)?,
        import_id: r.get(1)?,
        original_path: r.get(2)?,
        original_filename: r.get(3)?,
        original_dir: r.get(4)?,
        relative_dir: r.get(5)?,
        source_pack: r.get(6)?,
        size: r.get::<_, i64>(7)? as u64,
        hash: r.get(8)?,
        file_kind: serde_json::from_value(serde_json::Value::String(kind)).unwrap_or(FileKind::Audio),
        category_id: r.get(10)?,
        category_path: r.get(11)?,
        confidence: r.get::<_, f64>(12)? as f32,
        evidence: from_json(r.get(13)?),
        alternatives: from_json(r.get(14)?),
        sample_type: SampleType::parse(&st).unwrap_or(SampleType::OneShot),
        bpm: r.get::<_, Option<f64>>(16)?.map(|v| v as f32),
        bpm_source: r.get(17)?,
        key: r.get(18)?,
        method: r.get(19)?,
        audio: audio.and_then(|a| serde_json::from_str::<AudioMeta>(&a).ok()),
        manual_override: r.get::<_, i64>(21)? != 0,
        reviewed: r.get::<_, i64>(22)? != 0,
        status: SampleStatus::parse(&status),
        duplicate_of: r.get(24)?,
        duplicate_path: r.get(25)?,
        dup_decision: dup.and_then(|d| match d.as_str() {
            "keep" => Some(DupDecision::Keep),
            "skip" => Some(DupDecision::Skip),
            _ => None,
        }),
        destination_path: r.get(27)?,
        error: r.get(28)?,
        created_at: r.get(29)?,
        updated_at: r.get(30)?,
        key_source: r.get(31)?,
    })
}

fn import_from_row(r: &Row) -> rusqlite::Result<ImportRecord> {
    let mode: String = r.get(6)?;
    let status: String = r.get(7)?;
    Ok(ImportRecord {
        id: r.get(0)?,
        created_at: r.get(1)?,
        completed_at: r.get(2)?,
        sources: from_json(r.get(3)?),
        output_dir: r.get(4)?,
        library_root: r.get(5)?,
        mode: if mode == "move" { OpMode::Move } else { OpMode::Copy },
        status: ImportStatus::parse(&status),
        stats: from_json(r.get(8)?),
        messages: from_json(r.get(9)?),
    })
}

const IMPORT_COLS: &str = "id, created_at, completed_at, sources, output_dir, library_root, mode, status, stats, messages";

fn dup_str(d: Option<DupDecision>) -> Option<&'static str> {
    d.map(|d| match d {
        DupDecision::Keep => "keep",
        DupDecision::Skip => "skip",
    })
}

impl Database {
    pub fn open(path: &Path) -> CoreResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> CoreResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> CoreResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        // Additive migrations for databases created by older versions.
        let has_col = |table: &str, col: &str| -> rusqlite::Result<bool> {
            let mut st = conn.prepare(&format!("PRAGMA table_info({table})"))?;
            let names = st.query_map([], |r| r.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>()?;
            Ok(names.iter().any(|n| n == col))
        };
        if !has_col("samples", "key_source")? {
            conn.execute("ALTER TABLE samples ADD COLUMN key_source TEXT", [])?;
        }
        conn.execute("INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1')", [])?;
        Ok(Self { conn })
    }

    // ---------- settings & config ----------

    fn get_setting(&self, key: &str) -> CoreResult<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    fn put_setting(&self, key: &str, value: &str) -> CoreResult<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn settings(&self) -> CoreResult<Settings> {
        Ok(self.get_setting("app")?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
    }

    pub fn save_settings(&self, s: &Settings) -> CoreResult<()> {
        if !(0.0..=1.0).contains(&s.confidence_threshold) {
            return Err(CoreError::InvalidConfig("confidence threshold must be between 0 and 1".into()));
        }
        self.put_setting("app", &json(s))
    }

    pub fn category_config(&self) -> CoreResult<CategoryConfig> {
        match self.get_setting("categories")? {
            Some(s) => match CategoryConfig::from_json(&s) {
                Ok(c) => Ok(c),
                Err(_) => Ok(default_config()),
            },
            None => Ok(default_config()),
        }
    }

    pub fn save_category_config(&self, c: &CategoryConfig) -> CoreResult<()> {
        crate::config::CompiledConfig::compile(c)?;
        self.put_setting("categories", &json(c))
    }

    // ---------- imports ----------

    pub fn create_import(&self, rec: &ImportRecord) -> CoreResult<()> {
        self.conn.execute(
            &format!("INSERT INTO imports({IMPORT_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"),
            params![
                rec.id,
                rec.created_at,
                rec.completed_at,
                json(&rec.sources),
                rec.output_dir,
                rec.library_root,
                rec.mode.as_str(),
                rec.status.as_str(),
                json(&rec.stats),
                json(&rec.messages)
            ],
        )?;
        Ok(())
    }

    pub fn update_import(&self, rec: &ImportRecord) -> CoreResult<()> {
        self.conn.execute(
            "UPDATE imports SET completed_at = ?2, output_dir = ?3, library_root = ?4, mode = ?5, status = ?6, stats = ?7, messages = ?8 WHERE id = ?1",
            params![
                rec.id,
                rec.completed_at,
                rec.output_dir,
                rec.library_root,
                rec.mode.as_str(),
                rec.status.as_str(),
                json(&rec.stats),
                json(&rec.messages)
            ],
        )?;
        Ok(())
    }

    pub fn get_import(&self, id: &str) -> CoreResult<Option<ImportRecord>> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {IMPORT_COLS} FROM imports WHERE id = ?1"), [id], import_from_row)
            .optional()?)
    }

    pub fn list_imports(&self, limit: u32) -> CoreResult<Vec<ImportRecord>> {
        let mut st = self.conn.prepare(&format!("SELECT {IMPORT_COLS} FROM imports ORDER BY created_at DESC, rowid DESC LIMIT ?1"))?;
        let rows = st.query_map([limit], import_from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn imports_with_status(&self, status: ImportStatus) -> CoreResult<Vec<ImportRecord>> {
        let mut st = self.conn.prepare(&format!("SELECT {IMPORT_COLS} FROM imports WHERE status = ?1"))?;
        let rows = st.query_map([status.as_str()], import_from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete an import that never organised anything (discarding a scan).
    pub fn delete_import(&self, id: &str) -> CoreResult<()> {
        let organised: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM samples WHERE import_id = ?1 AND status = 'organised'",
            [id],
            |r| r.get(0),
        )?;
        if organised > 0 {
            return Err(CoreError::Other("this import has organised files; undo it before deleting".into()));
        }
        self.conn.execute("DELETE FROM operations WHERE import_id = ?1", [id])?;
        self.conn.execute("DELETE FROM samples WHERE import_id = ?1", [id])?;
        self.conn.execute("DELETE FROM imports WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- samples ----------

    pub fn insert_samples(&mut self, rows: &[SampleRow]) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare(&format!(
                "INSERT INTO samples({SAMPLE_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32)"
            ))?;
            for s in rows {
                st.execute(params![
                    s.id,
                    s.import_id,
                    s.original_path,
                    s.original_filename,
                    s.original_dir,
                    s.relative_dir,
                    s.source_pack,
                    s.size as i64,
                    s.hash,
                    serde_json::to_value(s.file_kind)?.as_str().unwrap_or("audio"),
                    s.category_id,
                    s.category_path,
                    s.confidence as f64,
                    json(&s.evidence),
                    json(&s.alternatives),
                    s.sample_type.as_str(),
                    s.bpm.map(|b| b as f64),
                    s.bpm_source,
                    s.key,
                    s.method,
                    s.audio.as_ref().map(json),
                    s.manual_override as i64,
                    s.reviewed as i64,
                    s.status.as_str(),
                    s.duplicate_of,
                    s.duplicate_path,
                    dup_str(s.dup_decision),
                    s.destination_path,
                    s.error,
                    s.created_at,
                    s.updated_at,
                    s.key_source,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_sample(&self, id: &str) -> CoreResult<Option<SampleRow>> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {SAMPLE_COLS} FROM samples WHERE id = ?1"), [id], sample_from_row)
            .optional()?)
    }

    pub fn samples_for_import(&self, import_id: &str) -> CoreResult<Vec<SampleRow>> {
        let mut st =
            self.conn.prepare(&format!("SELECT {SAMPLE_COLS} FROM samples WHERE import_id = ?1 ORDER BY original_path"))?;
        let rows = st.query_map([import_id], sample_from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn query_samples(&self, q: &SampleQuery) -> CoreResult<SamplePage> {
        use rusqlite::types::Value as V;
        let mut where_ = vec!["1=1".to_string()];
        let mut args: Vec<V> = Vec::new();
        // Bind a value and return its placeholder.
        let mut bind = |v: V| -> String {
            args.push(v);
            format!("?{}", args.len())
        };
        if let Some(id) = &q.import_id {
            where_.push(format!("import_id = {}", bind(V::Text(id.clone()))));
        }
        if !q.statuses.is_empty() {
            let ph: Vec<String> = q.statuses.iter().map(|s| bind(V::Text(s.clone()))).collect();
            where_.push(format!("status IN ({})", ph.join(",")));
        }
        if let Some(s) = q.search.as_ref().filter(|s| !s.trim().is_empty()) {
            let like = format!("%{}%", s.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
            let p = bind(V::Text(like));
            where_.push(format!(
                "(original_filename LIKE {p} ESCAPE '\\' OR relative_dir LIKE {p} ESCAPE '\\' OR source_pack LIKE {p} ESCAPE '\\')"
            ));
        }
        if let Some(c) = q.category.as_ref().filter(|c| !c.is_empty()) {
            let p = bind(V::Text(c.clone()));
            where_.push(format!("(category_path = {p} OR category_path LIKE {p} || '/%')"));
        }
        if let Some(v) = q.min_confidence {
            where_.push(format!("confidence >= {}", bind(V::Real(v as f64))));
        }
        if let Some(v) = q.max_confidence {
            where_.push(format!("confidence <= {}", bind(V::Real(v as f64))));
        }
        if let Some(t) = q.needs_review_below {
            where_.push(format!("(confidence < {} AND reviewed = 0 AND status = 'pending')", bind(V::Real(t as f64))));
        }
        if q.duplicates_only {
            where_.push("duplicate_of IS NOT NULL".into());
        }
        if let Some(p) = q.source_pack.as_ref().filter(|p| !p.is_empty()) {
            where_.push(format!("source_pack = {}", bind(V::Text(p.clone()))));
        }
        let w = where_.join(" AND ");
        let total: i64 =
            self.conn.query_row(&format!("SELECT COUNT(*) FROM samples WHERE {w}"), params_from_iter(args.iter()), |r| r.get(0))?;
        let order = sort_column(q.sort.as_deref().unwrap_or(""));
        let dir = if q.descending { "DESC" } else { "ASC" };
        let limit = if q.limit == 0 { 200 } else { q.limit.min(1000) };
        let sql = format!(
            "SELECT {SAMPLE_COLS} FROM samples WHERE {w} ORDER BY {order} {dir}, original_path LIMIT {limit} OFFSET {}",
            q.offset
        );
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map(params_from_iter(args.iter()), sample_from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(SamplePage { rows, total: total as u64 })
    }

    fn id_list(ids: &[String]) -> (String, Vec<rusqlite::types::Value>) {
        let ph = (0..ids.len()).map(|i| format!("?{}", i + 2)).collect::<Vec<_>>().join(",");
        (ph, ids.iter().map(|s| rusqlite::types::Value::Text(s.clone())).collect())
    }

    /// Manually reassign samples. Only pending samples can be changed.
    pub fn reassign(&mut self, ids: &[String], category_id: &str, category_path: &str) -> CoreResult<usize> {
        let tx = self.conn.transaction()?;
        let mut n = 0;
        {
            let mut st = tx.prepare(
                "UPDATE samples SET category_id = ?1, category_path = ?2, manual_override = 1, reviewed = 1, updated_at = ?3 \
                 WHERE id = ?4 AND status = 'pending'",
            )?;
            let ts = now();
            for chunk in ids {
                n += st.execute(params![category_id, category_path, ts, chunk])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// Accept predictions as-is (clears the review flag).
    pub fn mark_reviewed(&mut self, ids: &[String], reviewed: bool) -> CoreResult<usize> {
        let tx = self.conn.transaction()?;
        let mut n = 0;
        {
            let mut st = tx.prepare("UPDATE samples SET reviewed = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'pending'")?;
            let ts = now();
            for id in ids {
                n += st.execute(params![reviewed as i64, ts, id])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    pub fn set_dup_decision(&mut self, ids: &[String], d: Option<DupDecision>) -> CoreResult<usize> {
        let tx = self.conn.transaction()?;
        let mut n = 0;
        {
            let mut st = tx.prepare(
                "UPDATE samples SET dup_decision = ?1, updated_at = ?2 WHERE id = ?3 AND duplicate_of IS NOT NULL AND status = 'pending'",
            )?;
            let ts = now();
            for id in ids {
                n += st.execute(params![dup_str(d), ts, id])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    pub fn set_source_pack(&mut self, ids: &[String], pack: &str) -> CoreResult<usize> {
        let pack = pack.trim();
        if pack.is_empty() {
            return Err(CoreError::Other("source pack name cannot be empty".into()));
        }
        let tx = self.conn.transaction()?;
        let mut n = 0;
        {
            let mut st = tx.prepare("UPDATE samples SET source_pack = ?1, updated_at = ?2 WHERE id = ?3")?;
            let ts = now();
            for id in ids {
                n += st.execute(params![pack, ts, id])?;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// Replace classification results for non-manual pending samples (used
    /// after the category configuration changes).
    pub fn update_classification(&mut self, rows: &[SampleRow]) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare(
                "UPDATE samples SET category_id=?2, category_path=?3, confidence=?4, evidence=?5, alternatives=?6, sample_type=?7, \
                 bpm=?8, bpm_source=?9, musical_key=?10, method=?11, updated_at=?12, key_source=?13 WHERE id=?1 AND manual_override = 0 AND status='pending'",
            )?;
            let ts = now();
            for s in rows {
                st.execute(params![
                    s.id,
                    s.category_id,
                    s.category_path,
                    s.confidence as f64,
                    json(&s.evidence),
                    json(&s.alternatives),
                    s.sample_type.as_str(),
                    s.bpm.map(|b| b as f64),
                    s.bpm_source,
                    s.key,
                    s.method,
                    ts,
                    s.key_source
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_sample_result(
        &self,
        id: &str,
        status: SampleStatus,
        destination: Option<&str>,
        error: Option<&str>,
    ) -> CoreResult<()> {
        self.conn.execute(
            "UPDATE samples SET status = ?2, destination_path = ?3, error = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, status.as_str(), destination, error, now()],
        )?;
        Ok(())
    }

    /// Record the category a file was actually placed in, keeping the
    /// original prediction in the evidence trail.
    pub fn set_sample_category(&self, id: &str, category_id: &str, category_path: &str, note: &str) -> CoreResult<()> {
        let ev: Option<String> = self.conn.query_row("SELECT evidence FROM samples WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        let mut evidence: Vec<String> = from_json(ev);
        evidence.push(format!("Placed in {category_path}: {note}"));
        self.conn.execute(
            "UPDATE samples SET category_id = ?2, category_path = ?3, evidence = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, category_id, category_path, json(&evidence), now()],
        )?;
        Ok(())
    }

    /// Organised samples whose content hash is among `hashes`
    /// (duplicates already in the library).
    pub fn organised_with_hashes(&self, hashes: &[String]) -> CoreResult<Vec<(String, String, String)>> {
        let mut out = Vec::new();
        for chunk in hashes.chunks(500) {
            let (ph, vals) = Self::id_list(chunk);
            let sql = format!(
                "SELECT id, hash, COALESCE(destination_path, original_path) FROM samples WHERE status = 'organised' AND ?1 = ?1 AND hash IN ({ph})"
            );
            // ?1 is a dummy so the id placeholders can start at ?2.
            let mut all = vec![rusqlite::types::Value::Integer(1)];
            all.extend(vals);
            let mut st = self.conn.prepare(&sql)?;
            let rows = st.query_map(params_from_iter(all.iter()), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            for r in rows {
                out.push(r?);
            }
        }
        Ok(out)
    }

    pub fn import_summary(&self, import_id: &str, threshold: f32) -> CoreResult<ImportSummary> {
        let count = |where_: &str| -> CoreResult<u64> {
            let n: i64 = self.conn.query_row(
                &format!("SELECT COUNT(*) FROM samples WHERE import_id = ?1 AND {where_}"),
                params![import_id, threshold as f64],
                |r| r.get(0),
            )?;
            Ok(n as u64)
        };
        let mut st = self.conn.prepare(
            "SELECT category_path, COUNT(*) FROM samples WHERE import_id = ?1 GROUP BY category_path ORDER BY category_path",
        )?;
        let by_category = st
            .query_map([import_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ImportSummary {
            total: count("?2 = ?2")?,
            pending: count("status = 'pending' AND ?2 = ?2")?,
            needs_review: count("status = 'pending' AND reviewed = 0 AND confidence < ?2")?,
            duplicates: count("duplicate_of IS NOT NULL AND ?2 = ?2")?,
            duplicates_undecided: count("duplicate_of IS NOT NULL AND dup_decision IS NULL AND status = 'pending' AND ?2 = ?2")?,
            organised: count("status = 'organised' AND ?2 = ?2")?,
            skipped: count("status = 'skipped' AND ?2 = ?2")?,
            failed: count("status = 'failed' AND ?2 = ?2")?,
            by_category,
        })
    }

    pub fn dashboard(&self, threshold: f32) -> CoreResult<DashboardStats> {
        let q = |sql: &str| -> CoreResult<u64> { Ok(self.conn.query_row(sql, params![threshold as f64], |r| r.get::<_, i64>(0))? as u64) };
        let last_output_dir = self
            .conn
            .query_row("SELECT output_dir FROM imports WHERE output_dir != '' ORDER BY created_at DESC, rowid DESC LIMIT 1", [], |r| r.get(0))
            .optional()?;
        Ok(DashboardStats {
            total_imported: q("SELECT COUNT(*) FROM samples WHERE ?1 = ?1")?,
            organised: q("SELECT COUNT(*) FROM samples WHERE status = 'organised' AND ?1 = ?1")?,
            awaiting_review: q("SELECT COUNT(*) FROM samples WHERE status = 'pending' AND reviewed = 0 AND confidence < ?1")?,
            duplicates: q("SELECT COUNT(*) FROM samples WHERE duplicate_of IS NOT NULL AND ?1 = ?1")?,
            imports: q("SELECT COUNT(*) FROM imports WHERE ?1 = ?1")?,
            last_output_dir,
        })
    }

    /// Category → count for organised samples (the library tree).
    pub fn library_tree(&self) -> CoreResult<Vec<(String, u64)>> {
        let mut st = self.conn.prepare(
            "SELECT category_path, COUNT(*) FROM samples WHERE status = 'organised' GROUP BY category_path ORDER BY category_path",
        )?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn source_packs(&self, import_id: Option<&str>) -> CoreResult<Vec<(String, u64)>> {
        let mut st = self.conn.prepare(
            "SELECT source_pack, COUNT(*) FROM samples WHERE (?1 IS NULL OR import_id = ?1) GROUP BY source_pack ORDER BY source_pack",
        )?;
        let rows = st
            .query_map([import_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Is `path` a file Kit Organizer knows about (source or destination)?
    /// Used to restrict which files the preview protocol may read.
    pub fn is_known_path(&self, path: &str) -> CoreResult<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM samples WHERE original_path = ?1 OR destination_path = ?1",
            [path],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Count of pending samples referencing each category id, for warnings
    /// in the Category Manager.
    pub fn category_usage(&self) -> CoreResult<Vec<(String, u64)>> {
        let mut st = self.conn.prepare("SELECT category_id, COUNT(*) FROM samples WHERE status = 'pending' GROUP BY category_id")?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Point pending samples at a different category (merge/delete).
    pub fn remap_category(&self, from_id: &str, to_id: &str, to_path: &str) -> CoreResult<usize> {
        Ok(self.conn.execute(
            "UPDATE samples SET category_id = ?2, category_path = ?3, updated_at = ?4 WHERE category_id = ?1 AND status = 'pending'",
            params![from_id, to_id, to_path, now()],
        )?)
    }

    /// Refresh stored paths after categories are renamed.
    pub fn refresh_category_paths(&mut self, id_to_path: &[(String, String)]) -> CoreResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare("UPDATE samples SET category_path = ?2 WHERE category_id = ?1 AND status = 'pending'")?;
            for (id, path) in id_to_path {
                st.execute(params![id, path])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---------- learned rules ----------

    /// Remember a correction. Re-teaching the same pattern with a new
    /// category replaces the old one.
    pub fn upsert_learned(&self, p: &crate::classifier::learned::Pattern, category_id: &str, example: &str) -> CoreResult<()> {
        let ts = now();
        self.conn.execute(
            "INSERT INTO learned_rules(kind, tokens, scope, category_id, example, hits, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6) \
             ON CONFLICT(kind, tokens, scope) DO UPDATE SET \
               hits = CASE WHEN learned_rules.category_id = excluded.category_id THEN learned_rules.hits + 1 ELSE 1 END, \
               category_id = excluded.category_id, example = excluded.example, updated_at = excluded.updated_at",
            params![p.kind.as_str(), p.tokens.join(" "), p.scope.clone().unwrap_or_default(), category_id, example, ts],
        )?;
        Ok(())
    }

    pub fn learned_rules(&self) -> CoreResult<Vec<crate::classifier::learned::LearnedRule>> {
        use crate::classifier::learned::{LearnedRule, RuleKind};
        let mut st = self.conn.prepare(
            "SELECT id, kind, tokens, scope, category_id, example, hits, created_at FROM learned_rules ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = st
            .query_map([], |r| {
                let kind: String = r.get(1)?;
                let tokens: String = r.get(2)?;
                let scope: String = r.get(3)?;
                Ok(LearnedRule {
                    id: r.get(0)?,
                    kind: if kind == "folder" { RuleKind::Folder } else { RuleKind::Name },
                    tokens: tokens.split(' ').filter(|t| !t.is_empty()).map(String::from).collect(),
                    scope: (!scope.is_empty()).then_some(scope),
                    category_id: r.get(4)?,
                    example: r.get(5)?,
                    hits: r.get::<_, i64>(6)? as u32,
                    created_at: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_learned(&self, id: i64) -> CoreResult<()> {
        self.conn.execute("DELETE FROM learned_rules WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_learned(&self) -> CoreResult<()> {
        self.conn.execute("DELETE FROM learned_rules", [])?;
        Ok(())
    }

    // ---------- operations journal ----------

    pub fn insert_operations(&mut self, ops: &[OperationRow]) -> CoreResult<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut ids = Vec::with_capacity(ops.len());
        {
            let mut st = tx.prepare(
                "INSERT INTO operations(import_id, sample_id, op, src, dest, temp, hash, created_dirs, state, error, updated_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )?;
            let ts = now();
            for o in ops {
                st.execute(params![o.import_id, o.sample_id, o.op, o.src, o.dest, o.temp, o.hash, json(&o.created_dirs), o.state, o.error, ts])?;
                ids.push(tx.last_insert_rowid());
            }
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn update_operation(&self, o: &OperationRow) -> CoreResult<()> {
        self.conn.execute(
            "UPDATE operations SET dest = ?2, temp = ?3, created_dirs = ?4, state = ?5, error = ?6, updated_at = ?7 WHERE id = ?1",
            params![o.id, o.dest, o.temp, json(&o.created_dirs), o.state, o.error, now()],
        )?;
        Ok(())
    }

    pub fn operations_for_import(&self, import_id: &str) -> CoreResult<Vec<OperationRow>> {
        let mut st = self.conn.prepare(
            "SELECT id, import_id, sample_id, op, src, dest, temp, hash, created_dirs, state, error FROM operations WHERE import_id = ?1 ORDER BY id",
        )?;
        let rows = st
            .query_map([import_id], |r| {
                Ok(OperationRow {
                    id: r.get(0)?,
                    import_id: r.get(1)?,
                    sample_id: r.get(2)?,
                    op: r.get(3)?,
                    src: r.get(4)?,
                    dest: r.get(5)?,
                    temp: r.get(6)?,
                    hash: r.get(7)?,
                    created_dirs: from_json(r.get(8)?),
                    state: r.get(9)?,
                    error: r.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
