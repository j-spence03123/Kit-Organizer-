//! Data transfer types shared between the engine, database and UI.
//! All types serialise as camelCase JSON for the TypeScript frontend.

use crate::audio::AudioMeta;
use crate::classifier::{Alternative, SampleType};
use crate::filetypes::{FileKind, FileTypeSettings};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OpMode {
    #[default]
    Copy,
    Move,
}

impl OpMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpMode::Copy => "copy",
            OpMode::Move => "move",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateHandling {
    #[default]
    Skip,
    Keep,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DupDecision {
    Keep,
    Skip,
}

/// What to do with files still below the confidence threshold that the
/// user has not reviewed when organising.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LowConfidenceAction {
    /// Put them in Uncategorised (safe default).
    #[default]
    Uncategorised,
    /// Trust the prediction anyway.
    Predicted,
    /// Leave them out of this run.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CollisionHandling {
    /// `Kick 01 (2).wav` — the only behaviour that never loses data.
    #[default]
    Rename,
    /// Leave the file out if the name is taken.
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSettings {
    pub volume: f32,
    pub autoplay: bool,
    pub loop_playback: bool,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        Self { volume: 0.8, autoplay: true, loop_playback: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    /// One of the accent presets defined in the UI.
    pub accent: String,
    pub compact_rows: bool,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self { accent: "amber".into(), compact_rows: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub default_output_dir: Option<String>,
    pub default_mode: OpMode,
    pub duplicate_handling: DuplicateHandling,
    pub confidence_threshold: f32,
    pub low_confidence_action: LowConfidenceAction,
    pub file_types: FileTypeSettings,
    pub collision_handling: CollisionHandling,
    pub preview: PreviewSettings,
    pub appearance: AppearanceSettings,
    /// Worker threads for hashing/probing. `0` = automatic.
    pub worker_threads: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_output_dir: None,
            default_mode: OpMode::Copy,
            duplicate_handling: DuplicateHandling::Skip,
            confidence_threshold: 0.6,
            low_confidence_action: LowConfidenceAction::Uncategorised,
            file_types: FileTypeSettings::default(),
            collision_handling: CollisionHandling::Rename,
            preview: PreviewSettings::default(),
            appearance: AppearanceSettings::default(),
            worker_threads: 0,
        }
    }
}

impl Settings {
    pub fn threads(&self) -> usize {
        if self.worker_threads > 0 {
            self.worker_threads.min(32)
        } else {
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SampleStatus {
    /// Classified, waiting to be organised.
    Pending,
    Organised,
    Skipped,
    Failed,
    /// Organised and later reverted by undo.
    Undone,
}

impl SampleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SampleStatus::Pending => "pending",
            SampleStatus::Organised => "organised",
            SampleStatus::Skipped => "skipped",
            SampleStatus::Failed => "failed",
            SampleStatus::Undone => "undone",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "organised" => SampleStatus::Organised,
            "skipped" => SampleStatus::Skipped,
            "failed" => SampleStatus::Failed,
            "undone" => SampleStatus::Undone,
            _ => SampleStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SampleRow {
    pub id: String,
    pub import_id: String,
    pub original_path: String,
    pub original_filename: String,
    pub original_dir: String,
    /// Folders between the imported root and the file, for display.
    pub relative_dir: String,
    pub source_pack: String,
    pub size: u64,
    pub hash: Option<String>,
    pub file_kind: FileKind,
    pub category_id: String,
    pub category_path: String,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub alternatives: Vec<Alternative>,
    pub sample_type: SampleType,
    pub bpm: Option<f32>,
    pub bpm_source: Option<String>,
    pub key: Option<String>,
    pub method: String,
    pub audio: Option<AudioMeta>,
    pub manual_override: bool,
    pub reviewed: bool,
    pub status: SampleStatus,
    pub duplicate_of: Option<String>,
    pub duplicate_path: Option<String>,
    pub dup_decision: Option<DupDecision>,
    pub destination_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportStatus {
    Scanning,
    Review,
    Organising,
    Completed,
    Cancelled,
    Interrupted,
    Undone,
    #[serde(rename = "partially-undone")]
    PartiallyUndone,
    Failed,
}

impl ImportStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportStatus::Scanning => "scanning",
            ImportStatus::Review => "review",
            ImportStatus::Organising => "organising",
            ImportStatus::Completed => "completed",
            ImportStatus::Cancelled => "cancelled",
            ImportStatus::Interrupted => "interrupted",
            ImportStatus::Undone => "undone",
            ImportStatus::PartiallyUndone => "partially-undone",
            ImportStatus::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "scanning" => Self::Scanning,
            "organising" => Self::Organising,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "interrupted" => Self::Interrupted,
            "undone" => Self::Undone,
            "partially-undone" => Self::PartiallyUndone,
            "failed" => Self::Failed,
            _ => Self::Review,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportStats {
    pub files_discovered: u64,
    pub supported: u64,
    pub unsupported: u64,
    pub ignored: u64,
    pub scan_errors: u64,
    pub processed: u64,
    pub copied: u64,
    pub moved: u64,
    pub skipped: u64,
    pub duplicates: u64,
    pub renamed: u64,
    pub errors: u64,
    pub reverted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecord {
    pub id: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub sources: Vec<String>,
    pub output_dir: String,
    pub library_root: String,
    pub mode: OpMode,
    pub status: ImportStatus,
    pub stats: ImportStats,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FsIssue {
    pub path: String,
    pub message: String,
}
