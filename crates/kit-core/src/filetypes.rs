//! Stage 1 of classification: file-type detection by extension.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Audio,
    Midi,
    Preset,
    /// Recognised OS junk (`.DS_Store`, AppleDouble `._x.wav`, `Thumbs.db`).
    Ignored,
    /// Anything else. Never touched by the organiser.
    Unsupported,
}

/// Which extensions are enabled. Users can toggle these in Settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeSettings {
    pub audio: Vec<String>,
    pub midi: Vec<String>,
    pub presets: Vec<String>,
}

pub const ALL_AUDIO: &[&str] = &["wav", "mp3", "aif", "aiff", "flac", "ogg"];
pub const ALL_MIDI: &[&str] = &["mid", "midi"];
/// Synth/effect preset formats that are self-contained (moving them does not
/// break references). Kontakt `.nki` etc. are deliberately excluded because
/// they reference sample folders relative to themselves.
pub const ALL_PRESETS: &[&str] = &[
    "fxp", "fxb", "vstpreset", "aupreset", "vital", "serumpreset", "phaseplant", "h2p",
    "nmsv", "fst", "adv", "adg", "spf", "pgtx",
];

impl Default for FileTypeSettings {
    fn default() -> Self {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect();
        Self { audio: v(ALL_AUDIO), midi: v(ALL_MIDI), presets: v(ALL_PRESETS) }
    }
}

impl FileTypeSettings {
    pub fn detect(&self, path: &Path) -> FileKind {
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => return FileKind::Unsupported,
        };
        if is_os_junk(name) {
            return FileKind::Ignored;
        }
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_ascii_lowercase(),
            None => return FileKind::Unsupported,
        };
        let has = |list: &[String]| list.iter().any(|x| x.eq_ignore_ascii_case(&ext));
        if has(&self.audio) {
            FileKind::Audio
        } else if has(&self.midi) {
            FileKind::Midi
        } else if has(&self.presets) {
            FileKind::Preset
        } else {
            FileKind::Unsupported
        }
    }
}

/// macOS resource forks (`._Kick.wav`) carry an audio extension but are not
/// audio; treating them as samples would be a classic bug.
pub fn is_os_junk(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    name.starts_with("._")
        || lower == ".ds_store"
        || lower == "thumbs.db"
        || lower == "desktop.ini"
        || lower == "icon\r"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kinds() {
        let s = FileTypeSettings::default();
        assert_eq!(s.detect(Path::new("a/Kick.WAV")), FileKind::Audio);
        assert_eq!(s.detect(Path::new("a/x.aif")), FileKind::Audio);
        assert_eq!(s.detect(Path::new("a/beat.MID")), FileKind::Midi);
        assert_eq!(s.detect(Path::new("a/bass.fxp")), FileKind::Preset);
        assert_eq!(s.detect(Path::new("a/._Kick.wav")), FileKind::Ignored);
        assert_eq!(s.detect(Path::new("a/.DS_Store")), FileKind::Ignored);
        assert_eq!(s.detect(Path::new("a/readme.txt")), FileKind::Unsupported);
        assert_eq!(s.detect(Path::new("a/noext")), FileKind::Unsupported);
    }
}
