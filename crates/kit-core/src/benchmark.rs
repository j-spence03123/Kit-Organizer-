//! Accuracy benchmark for the classifier.
//!
//! A benchmark file lists one labelled sample per line:
//!
//! ```text
//! # comments and blank lines are ignored
//! Kicks | XLNT Riddim Vol 1/DRUMS/ONE SHOTS/KICKS/XLNT_Kick_01.wav
//! Bass/Growls | Kompany Kit/Bass/Kompany_Growl_03.wav
//! ```
//!
//! The first path segment is the imported pack folder. Only names are
//! needed, never audio, so real pack listings can be shared without
//! copyright concerns. Files in the same folder are classified together,
//! so sibling consistency is exercised exactly as in the app.

use crate::classifier::Classifier;
use crate::error::{CoreError, CoreResult};
use crate::filetypes::FileTypeSettings;
use crate::pipeline::{classify_batch, BatchItem};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Case {
    pub expected: String,
    pub path: String,
    pub line: usize,
}

pub fn parse(text: &str) -> CoreResult<Vec<Case>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (expected, path) = line
            .split_once('|')
            .or_else(|| line.split_once('\t'))
            .ok_or_else(|| CoreError::Other(format!("line {}: expected `Category | path`", i + 1)))?;
        out.push(Case { expected: expected.trim().trim_matches('/').to_string(), path: path.trim().replace('\\', "/"), line: i + 1 });
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct Miss {
    pub path: String,
    pub expected: String,
    pub got: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Default)]
pub struct CategoryStats {
    pub expected: usize,
    pub correct: usize,
    /// Predicted as this category but wrong.
    pub false_positives: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub total: usize,
    pub correct: usize,
    /// Correct but below the review threshold.
    pub correct_low_confidence: usize,
    /// Wrong but above the threshold (silent mistakes — the worst kind).
    pub wrong_high_confidence: usize,
    pub uncategorised: usize,
    pub threshold: f32,
    pub per_category: BTreeMap<String, CategoryStats>,
    pub confusions: Vec<((String, String), usize)>,
    pub misses: Vec<Miss>,
    pub unknown_expected: Vec<String>,
}

impl Report {
    pub fn accuracy(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.correct as f32 / self.total as f32
        }
    }
}

pub fn run(classifier: &Classifier, cases: &[Case], threshold: f32) -> Report {
    let types = FileTypeSettings::default();
    let split: Vec<(String, Vec<String>, String)> = cases
        .iter()
        .map(|c| {
            let mut parts: Vec<String> = c.path.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
            let file = parts.pop().unwrap_or_default();
            let group = parts.join("/");
            (file, parts, group)
        })
        .collect();
    let items: Vec<BatchItem> = split
        .iter()
        .map(|(file, folders, group)| BatchItem {
            file_name: file,
            folders,
            kind: match types.detect(Path::new(file)) {
                crate::filetypes::FileKind::Unsupported | crate::filetypes::FileKind::Ignored => crate::filetypes::FileKind::Audio,
                k => k,
            },
            audio: None,
            group,
            fixed: None,
        })
        .collect();
    let results = classify_batch(&items, classifier);
    let cfg = classifier.config();
    let unc = cfg.path_of(cfg.uncategorised).to_string();
    let mut rep = Report { threshold, ..Default::default() };
    let mut confusions: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (case, res) in cases.iter().zip(results) {
        let Some(res) = res else { continue };
        if cfg.find_path(&case.expected).is_none() && !rep.unknown_expected.contains(&case.expected) {
            rep.unknown_expected.push(case.expected.clone());
        }
        rep.total += 1;
        let ok = res.category_path.eq_ignore_ascii_case(&case.expected);
        rep.per_category.entry(case.expected.clone()).or_default().expected += 1;
        if res.category_path == unc {
            rep.uncategorised += 1;
        }
        if ok {
            rep.correct += 1;
            rep.per_category.get_mut(&case.expected).expect("inserted").correct += 1;
            if res.confidence < threshold {
                rep.correct_low_confidence += 1;
            }
        } else {
            if res.confidence >= threshold {
                rep.wrong_high_confidence += 1;
            }
            rep.per_category.entry(res.category_path.clone()).or_default().false_positives += 1;
            *confusions.entry((case.expected.clone(), res.category_path.clone())).or_default() += 1;
            rep.misses.push(Miss { path: case.path.clone(), expected: case.expected.clone(), got: res.category_path, confidence: res.confidence });
        }
    }
    let mut c: Vec<_> = confusions.into_iter().collect();
    c.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    rep.confusions = c;
    rep
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pct = |n: usize| if self.total == 0 { 0.0 } else { n as f32 * 100.0 / self.total as f32 };
        writeln!(f, "Accuracy: {:.1}% ({} / {})", self.accuracy() * 100.0, self.correct, self.total)?;
        writeln!(f, "  correct but flagged for review (< {:.0}%): {}", self.threshold * 100.0, self.correct_low_confidence)?;
        writeln!(f, "  wrong with high confidence (silent mistakes): {} ({:.1}%)", self.wrong_high_confidence, pct(self.wrong_high_confidence))?;
        writeln!(f, "  sent to Uncategorised: {}", self.uncategorised)?;
        if !self.unknown_expected.is_empty() {
            writeln!(f, "  WARNING: expected categories not in the configuration: {}", self.unknown_expected.join(", "))?;
        }
        let weak: Vec<_> = self
            .per_category
            .iter()
            .filter(|(_, s)| s.expected > 0 && s.correct < s.expected)
            .collect();
        if !weak.is_empty() {
            writeln!(f, "\nCategories with misses (recall):")?;
            for (cat, s) in weak {
                writeln!(f, "  {:<36} {:>3}/{:<3} ({:.0}%)", cat, s.correct, s.expected, s.correct as f32 * 100.0 / s.expected as f32)?;
            }
        }
        if !self.confusions.is_empty() {
            writeln!(f, "\nMost common confusions (expected → got):")?;
            for ((e, g), n) in self.confusions.iter().take(15) {
                writeln!(f, "  {n:>3} × {e} → {g}")?;
            }
            writeln!(f, "\nMisses:")?;
            for m in &self.misses {
                writeln!(f, "  {:>3.0}%  {} → {}   [{}]", m.confidence * 100.0, m.expected, m.got, m.path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_separators() {
        let c = parse("# c\n\nKicks | P/Kick.wav\nSnares\tP/Snare.wav\n").unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].expected, "Snares");
        assert!(parse("no separator").is_err());
    }
}
