//! Learning from corrections.
//!
//! When you reassign a sample, Kit Organizer remembers its *name pattern*:
//! the filename's words with numbers, keys, BPM and the vendor prefix
//! removed (`XLNT_Shot_03_F.wav` → `shot`). Files whose pattern matches get
//! strong evidence for your chosen category — strongest within the same
//! vendor or pack. Files with no meaningful name (`XLNT_07.wav`) learn from
//! their folder name instead.

use super::{musical, text, Evidence, EvidenceSource, Origin, Prepared};
use crate::config::CompiledConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RuleKind {
    /// Matches the filename's words.
    Name,
    /// Matches the parent folder (for files with no meaningful name).
    Folder,
}

impl RuleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleKind::Name => "name",
            RuleKind::Folder => "folder",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearnedRule {
    pub id: i64,
    pub kind: RuleKind,
    pub tokens: Vec<String>,
    /// Vendor prefix or source pack the correction was made in.
    pub scope: Option<String>,
    pub category_id: String,
    /// The file that taught this rule.
    pub example: String,
    pub hits: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern {
    pub kind: RuleKind,
    pub tokens: Vec<String>,
    pub scope: Option<String>,
}

impl Pattern {
    pub fn display(&self) -> String {
        match self.kind {
            RuleKind::Name => format!("files named like “{}”", self.tokens.join(" ")),
            RuleKind::Folder => format!("unnamed files in folders called “{}”", self.tokens.join(" ")),
        }
    }
}

/// Derive the learnable pattern of a file.
pub fn pattern_of(cfg: &CompiledConfig, file_name: &str, folders: &[String]) -> Option<Pattern> {
    let stem = super::tokenize::stem(file_name);
    let (vendor, toks) = text::name_tokens(cfg, stem);
    let key = musical::key_from_name(stem).map(|k| k.to_lowercase().replace('#', ""));
    let mut words: Vec<String> = toks
        .into_iter()
        .filter(|t| {
            t.chars().count() > 1
                && !t.chars().all(|c| c.is_ascii_digit())
                && t != "bpm"
                && key.as_deref() != Some(t.as_str())
        })
        .collect();
    words.sort();
    words.dedup();
    let scope = vendor.or_else(|| (!folders.is_empty()).then(|| crate::pipeline::detect_source_pack(folders).to_lowercase()));
    if !words.is_empty() {
        return Some(Pattern { kind: RuleKind::Name, tokens: words, scope });
    }
    let parent = folders.last()?;
    let mut ft: Vec<String> = super::tokenize::fine_tokens(parent).into_iter().filter(|t| !t.chars().all(|c| c.is_ascii_digit())).collect();
    if ft.is_empty() || cfg.generic_folders.contains(&ft.join(" ")) {
        return None;
    }
    ft.dedup();
    Some(Pattern { kind: RuleKind::Folder, tokens: ft, scope })
}

pub struct LearnedSource {
    rules: Vec<LearnedRule>,
}

impl LearnedSource {
    pub fn new(rules: Vec<LearnedRule>) -> Self {
        Self { rules }
    }
}

impl EvidenceSource for LearnedSource {
    fn name(&self) -> &'static str {
        "learned"
    }
    fn collect(&self, cfg: &CompiledConfig, p: &Prepared) -> Vec<Evidence> {
        if self.rules.is_empty() {
            return vec![];
        }
        let Some(pat) = pattern_of(cfg, p.input.file_name, p.input.folders) else { return vec![] };
        let have: HashSet<&String> = pat.tokens.iter().collect();
        let w = cfg.source.weights.learned;
        let mut out = Vec::new();
        for r in self.rules.iter().filter(|r| r.kind == pat.kind) {
            let Some(&node) = cfg.by_id.get(&r.category_id) else { continue };
            let exact = r.tokens.len() == pat.tokens.len() && r.tokens.iter().all(|t| have.contains(t));
            let subset = !exact && r.tokens.iter().all(|t| have.contains(t));
            if !exact && !subset {
                continue;
            }
            let same_scope = r.scope.is_some() && r.scope == pat.scope;
            let mut pts = if same_scope { w } else { w * 0.6 };
            if subset {
                pts *= 0.5;
            }
            out.push(Evidence {
                node,
                points: pts,
                label: format!(
                    "Learned from your correction of “{}” → {}{}{}",
                    r.example,
                    cfg.path_of(node),
                    if same_scope { " (same pack/vendor)" } else { "" },
                    if subset { " (partial match)" } else { "" }
                ),
                origin: Origin::Learned,
                weak: false,
            });
        }
        out
    }
}
