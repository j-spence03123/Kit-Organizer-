//! The rule-based classification engine.
//!
//! # Pipeline
//!
//! 1. **File type** — MIDI and preset files are classified by extension
//!    with certainty; only audio continues.
//! 2. **Evidence collection** — each [`EvidenceSource`] produces scored
//!    evidence for categories. Built in: [`FilenameSource`] and
//!    [`FolderSource`]. A future ML model plugs in as another source.
//! 3. **Loop detection** ([`loops`]).
//! 4. **Scoring** — per category:
//!    `total = direct + Σ ancestor_direct × ancestor_support + priority × priority_step`
//!    where `direct` is the best filename evidence plus the best folder
//!    evidence for that category.
//! 5. **Routing** — loops follow the winner's `loop_target`; parent
//!    categories with only generic evidence go to their `default_child`.
//! 6. **Confidence** —
//!    `strength = 1 − exp(−S1 / saturation)`,
//!    `margin = 1 − 0.5 × S2 / S1` (S2 = best *conflicting* candidate, i.e.
//!    not an ancestor/descendant of the winner), and
//!    `confidence = strength × margin × loop_factor`, where `loop_factor`
//!    is 0.9 when the loop/one-shot call was borderline and changed the
//!    destination. The result is rule certainty, not a calibrated
//!    probability.
//!
//! ## Phrase matching
//! Filenames and folder names are tokenised (see [`tokenize`]) and matched
//! greedily, longest phrase first, so `reverse crash` (one 2-token match →
//! Cymbals/Reverse Cymbals) wins over `reverse` + `crash`. Tokens consumed
//! by a phrase are not matched again. Each window is tried spaced
//! (`hi hat`), concatenated (`hihat`) and with a singular last token
//! (`kicks` → `kick`).

pub mod loops;
pub mod musical;
pub mod tokenize;

use crate::audio::AudioMeta;
use crate::config::{is_ancestor, CompiledConfig, KindHint};
use crate::filetypes::FileKind;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokenize::{fine_tokens, singular, stem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SampleType {
    OneShot,
    Loop,
    Midi,
    Preset,
}

impl SampleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SampleType::OneShot => "one-shot",
            SampleType::Loop => "loop",
            SampleType::Midi => "midi",
            SampleType::Preset => "preset",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "one-shot" => SampleType::OneShot,
            "loop" => SampleType::Loop,
            "midi" => SampleType::Midi,
            "preset" => SampleType::Preset,
            _ => return None,
        })
    }
}

/// Everything the classifier needs to know about a file. Paths are
/// pre-split so the classifier never touches the filesystem.
#[derive(Debug, Clone)]
pub struct ClassifyInput<'a> {
    pub file_name: &'a str,
    /// Folder names from the imported root (index 0) down to the immediate
    /// parent (last).
    pub folders: &'a [String],
    pub kind: FileKind,
    pub audio: Option<&'a AudioMeta>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Origin {
    Filename { position: usize },
    Folder { distance: usize },
    Metadata,
    Model,
}

/// One piece of evidence for a category.
#[derive(Debug, Clone)]
pub struct Evidence {
    pub node: usize,
    pub points: f32,
    pub label: String,
    pub origin: Origin,
}

/// Prepared, tokenised view of the input shared by all sources.
pub struct Prepared<'a> {
    pub input: &'a ClassifyInput<'a>,
    pub stem: &'a str,
    pub name_tokens: Vec<String>,
    /// (tokens, distance, is_root, original name)
    pub folders: Vec<(Vec<String>, usize, bool, &'a str)>,
}

/// A pluggable producer of classification evidence. Implement this to add
/// e.g. an audio-analysis model; its evidence is combined with the
/// rule-based sources using the same scoring.
pub trait EvidenceSource: Send + Sync {
    fn name(&self) -> &'static str;
    fn collect(&self, cfg: &CompiledConfig, p: &Prepared) -> Vec<Evidence>;
}

#[derive(Debug, Clone)]
pub struct PhraseMatch {
    pub start: usize,
    pub len: usize,
    pub text: String,
    pub hits: Vec<(usize, bool, usize)>, // node, weak, alias token count
}

/// Greedy longest-phrase-first matching against the alias table.
pub fn match_phrases(cfg: &CompiledConfig, tokens: &[String]) -> Vec<PhraseMatch> {
    let mut out = Vec::new();
    let mut i = 0;
    'outer: while i < tokens.len() {
        let max = cfg.max_phrase.min(tokens.len() - i);
        for n in (1..=max).rev() {
            let w = &tokens[i..i + n];
            let mut keys = vec![w.join(" ")];
            if n > 1 {
                keys.push(w.concat());
            }
            if let Some(s) = singular(&w[n - 1]) {
                let mut v = w.to_vec();
                v[n - 1] = s;
                keys.push(v.join(" "));
                if n > 1 {
                    keys.push(v.concat());
                }
            }
            for k in keys {
                if let Some(hits) = cfg.aliases.get(&k) {
                    out.push(PhraseMatch {
                        start: i,
                        len: n,
                        text: w.join(" "),
                        hits: hits.iter().map(|h| (h.node, h.weak, h.tokens)).collect(),
                    });
                    i += n;
                    continue 'outer;
                }
            }
        }
        i += 1;
    }
    out
}

fn node_factor(cfg: &CompiledConfig, node: usize) -> f32 {
    if cfg.nodes[node].is_leaf() {
        1.0
    } else {
        cfg.source.weights.parent_factor
    }
}

pub struct FilenameSource;

impl EvidenceSource for FilenameSource {
    fn name(&self) -> &'static str {
        "filename"
    }
    fn collect(&self, cfg: &CompiledConfig, p: &Prepared) -> Vec<Evidence> {
        let w = &cfg.source.weights;
        let mut out = Vec::new();
        for m in match_phrases(cfg, &p.name_tokens) {
            for &(node, weak, ntok) in &m.hits {
                let mut pts = w.filename + w.phrase_bonus * (ntok.saturating_sub(1)) as f32;
                pts *= node_factor(cfg, node);
                if weak {
                    pts *= w.weak_factor;
                }
                out.push(Evidence {
                    node,
                    points: pts,
                    label: format!(
                        "Filename contains \"{}\"{} → {}",
                        m.text,
                        if weak { " (weak alias)" } else { "" },
                        cfg.path_of(node)
                    ),
                    origin: Origin::Filename { position: m.start },
                });
            }
        }
        out
    }
}

pub struct FolderSource;

impl EvidenceSource for FolderSource {
    fn name(&self) -> &'static str {
        "folder"
    }
    fn collect(&self, cfg: &CompiledConfig, p: &Prepared) -> Vec<Evidence> {
        let w = &cfg.source.weights;
        let mut out = Vec::new();
        for (toks, dist, root, original) in &p.folders {
            if toks.is_empty() || cfg.generic_folders.contains(&toks.join(" ")) {
                continue;
            }
            let mut factor = w.folder_decay.powi(*dist as i32 - 1);
            if *root {
                factor *= w.root_folder_factor;
            }
            for m in match_phrases(cfg, toks) {
                for &(node, weak, ntok) in &m.hits {
                    let mut pts = (w.folder_near + w.phrase_bonus * ntok.saturating_sub(1) as f32) * factor;
                    pts *= node_factor(cfg, node);
                    if weak {
                        pts *= w.weak_factor;
                    }
                    let which = match (dist, root) {
                        (_, true) => "Imported folder".to_string(),
                        (1, _) => "Parent folder".to_string(),
                        (d, _) => format!("Folder {d} levels up"),
                    };
                    out.push(Evidence {
                        node,
                        points: pts,
                        label: format!("{which} \"{original}\" indicates {}", cfg.path_of(node)),
                        origin: Origin::Folder { distance: *dist },
                    });
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Alternative {
    pub category_id: String,
    pub path: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category_id: String,
    pub category_path: String,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub alternatives: Vec<Alternative>,
    pub sample_type: SampleType,
    pub loop_score: Option<f32>,
    pub bpm: Option<f32>,
    pub bpm_source: Option<String>,
    pub key: Option<String>,
    pub method: String,
}

impl Classification {
    /// Top-level category ("Bass").
    pub fn category(&self) -> &str {
        self.category_path.split('/').next().unwrap_or("")
    }
    /// Everything below the top level ("Growls"), if any.
    pub fn subcategory(&self) -> Option<&str> {
        self.category_path.split_once('/').map(|(_, s)| s)
    }
}

pub struct Classifier {
    cfg: CompiledConfig,
    sources: Vec<Box<dyn EvidenceSource>>,
}

impl Classifier {
    pub fn new(cfg: CompiledConfig) -> Self {
        Self { cfg, sources: vec![Box::new(FilenameSource), Box::new(FolderSource)] }
    }

    /// Add another evidence source (e.g. an audio model).
    pub fn with_source(mut self, s: Box<dyn EvidenceSource>) -> Self {
        self.sources.push(s);
        self
    }

    pub fn config(&self) -> &CompiledConfig {
        &self.cfg
    }

    pub fn classify(&self, input: &ClassifyInput) -> Classification {
        let cfg = &self.cfg;
        let stem = stem(input.file_name);
        let ext = input.file_name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
        match input.kind {
            FileKind::Midi => return self.fixed(cfg.midi, SampleType::Midi, format!("File type .{ext} is MIDI"), stem),
            FileKind::Preset => {
                return self.fixed(cfg.preset, SampleType::Preset, format!("File type .{ext} is a synth/effect preset"), stem)
            }
            _ => {}
        }

        let n_folders = input.folders.len();
        let prepared = Prepared {
            input,
            stem,
            name_tokens: fine_tokens(stem),
            folders: input
                .folders
                .iter()
                .enumerate()
                .map(|(i, f)| (fine_tokens(f), n_folders - i, i == 0, f.as_str()))
                .collect(),
        };

        let bpm_name = musical::bpm_from_name(stem);
        let loop_folders: Vec<(Vec<String>, usize, bool)> =
            prepared.folders.iter().map(|(t, d, r, _)| (t.clone(), *d, *r)).collect();
        let lp = loops::detect(
            cfg,
            &loops::LoopInput {
                name_tokens: &prepared.name_tokens,
                folders: &loop_folders,
                bpm_in_name: bpm_name.is_some(),
                note_octave: musical::has_note_octave(stem),
                audio: input.audio,
            },
        );
        let sample_type = if lp.is_loop { SampleType::Loop } else { SampleType::OneShot };
        let (bpm, bpm_source) = match (bpm_name, input.audio.and_then(|a| a.acid_tempo)) {
            (Some(b), _) => (Some(b), Some("filename".to_string())),
            (None, Some(t)) if t > 0.0 && input.audio.and_then(|a| a.acid_one_shot) == Some(false) => {
                (Some((t * 100.0).round() / 100.0), Some("metadata".to_string()))
            }
            _ => (None, None),
        };
        let key = musical::key_from_name(stem);

        // --- Evidence aggregation ---
        let mut evidence: Vec<Evidence> = Vec::new();
        for s in &self.sources {
            evidence.extend(s.collect(cfg, &prepared));
        }
        let w = &cfg.source.weights;
        // Best filename + best folder (+ sums for other origins) per node.
        let mut best_file: HashMap<usize, usize> = HashMap::new();
        let mut best_folder: HashMap<usize, usize> = HashMap::new();
        let mut other: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, e) in evidence.iter().enumerate() {
            let slot = match e.origin {
                Origin::Filename { .. } => &mut best_file,
                Origin::Folder { .. } => &mut best_folder,
                _ => {
                    other.entry(e.node).or_default().push(i);
                    continue;
                }
            };
            let entry = slot.entry(e.node).or_insert(i);
            if evidence[*entry].points < e.points {
                *entry = i;
            }
        }
        let mut direct: HashMap<usize, (f32, Vec<usize>)> = HashMap::new();
        for (&node, &i) in best_file.iter().chain(best_folder.iter()) {
            let d = direct.entry(node).or_default();
            d.0 += evidence[i].points;
            d.1.push(i);
        }
        for (node, idxs) in &other {
            let d = direct.entry(*node).or_default();
            for &i in idxs {
                d.0 += evidence[i].points;
                d.1.push(i);
            }
        }

        // Head-noun rule: when several unrelated categories are named in the
        // filename, the last one is usually the noun ("Neuro Growl").
        let mut head_bonus: Option<(usize, String)> = None;
        {
            let mut file_nodes: Vec<(usize, usize)> = best_file
                .iter()
                .map(|(&node, &i)| match evidence[i].origin {
                    Origin::Filename { position } => (position, node),
                    _ => unreachable!(),
                })
                .collect();
            file_nodes.sort();
            if let Some(&(_, last)) = file_nodes.last() {
                let conflicts = file_nodes.iter().any(|&(_, n)| {
                    n != last && !is_ancestor(&cfg.nodes, n, last) && !is_ancestor(&cfg.nodes, last, n)
                });
                if conflicts {
                    head_bonus = Some((last, format!("Head-noun rule: last keyword points to {}", cfg.path_of(last))));
                }
            }
        }

        if direct.is_empty() {
            let mut ev = vec!["No category keywords found in the filename or folders".to_string()];
            ev.extend(lp.evidence.iter().cloned());
            return Classification {
                category_id: cfg.nodes[cfg.uncategorised].id.clone(),
                category_path: cfg.path_of(cfg.uncategorised).to_string(),
                confidence: 0.0,
                evidence: ev,
                alternatives: vec![],
                sample_type,
                loop_score: Some(lp.score),
                bpm,
                bpm_source,
                key,
                method: "fallback".into(),
            };
        }

        let total_of = |node: usize| -> f32 {
            let mut t = direct.get(&node).map(|d| d.0).unwrap_or(0.0);
            let mut cur = cfg.nodes[node].parent;
            while let Some(p) = cur {
                if let Some(d) = direct.get(&p) {
                    t += d.0 * w.ancestor_support;
                }
                cur = cfg.nodes[p].parent;
            }
            t += cfg.nodes[node].priority as f32 * w.priority_step;
            if head_bonus.as_ref().is_some_and(|(n, _)| *n == node) {
                t += w.head_noun_bonus;
            }
            t
        };
        let mut ranked: Vec<(usize, f32)> = direct.keys().map(|&n| (n, total_of(n))).collect();
        // Deterministic: score desc, then config order.
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        let (winner, s1) = ranked[0];
        let related = |a: usize, b: usize| a == b || is_ancestor(&cfg.nodes, a, b) || is_ancestor(&cfg.nodes, b, a);
        let competitors: Vec<(usize, f32)> = ranked.iter().copied().filter(|&(n, _)| !related(n, winner)).collect();
        let s2 = competitors.first().map(|c| c.1).unwrap_or(0.0).max(0.0);

        // --- Routing ---
        let route = |node: usize, ev: &mut Vec<String>| -> (usize, bool) {
            let mut fin = node;
            let mut loop_routed = false;
            let n = &cfg.nodes[node];
            if lp.is_loop {
                if let Some(t) = n.loop_target {
                    if t != node {
                        ev.push(format!("Detected as a loop → {}", cfg.path_of(t)));
                        fin = t;
                        loop_routed = true;
                    }
                }
            }
            if let Some(dc) = cfg.nodes[fin].default_child {
                ev.push(format!("No subcategory evidence → default {}", cfg.path_of(dc)));
                fin = dc;
            }
            (fin, loop_routed)
        };

        let mut ev_out = Vec::new();
        // Winner evidence first (direct + ancestor support).
        let mut chain = vec![winner];
        let mut cur = cfg.nodes[winner].parent;
        while let Some(p) = cur {
            chain.push(p);
            cur = cfg.nodes[p].parent;
        }
        for &n in &chain {
            if let Some((pts, idxs)) = direct.get(&n) {
                for &i in idxs {
                    let e = &evidence[i];
                    let shown = if n == winner { e.points } else { e.points * w.ancestor_support };
                    ev_out.push(format!("{} ({:+.2})", e.label, shown));
                }
                let _ = pts;
            }
        }
        if let Some((n, label)) = &head_bonus {
            if *n == winner {
                ev_out.push(format!("{label} ({:+.2})", w.head_noun_bonus));
            }
        }
        if cfg.nodes[winner].priority != 0 {
            ev_out.push(format!("Category priority {} ({:+.2})", cfg.nodes[winner].priority, cfg.nodes[winner].priority as f32 * w.priority_step));
        }
        ev_out.push(format!(
            "Loop analysis: {} (score {:.1}{})",
            if lp.is_loop { "loop" } else { "one-shot" },
            lp.score,
            if lp.evidence.is_empty() { String::new() } else { format!("; {}", lp.evidence.join("; ")) }
        ));
        let (final_node, loop_routed) = route(winner, &mut ev_out);
        for (n, s) in competitors.iter().take(3) {
            ev_out.push(format!("Conflicting evidence for {} (score {:.2})", cfg.path_of(*n), s));
        }

        // --- Confidence ---
        let strength = 1.0 - (-s1.max(0.0) / w.saturation).exp();
        let margin = if s1 > 0.0 { 1.0 - 0.5 * (s2 / s1).min(1.0) } else { 0.0 };
        let mut factor = 1.0;
        if lp.ambiguous && (loop_routed || cfg.nodes[winner].loop_target.is_some() || cfg.nodes[winner].kind == KindHint::Loop) {
            factor *= 0.9;
            ev_out.push("Loop/one-shot decision is borderline (×0.9)".into());
        }
        if !lp.is_loop && cfg.nodes[final_node].kind == KindHint::Loop && lp.score < 0.0 {
            factor *= 0.8;
            ev_out.push("Category expects loops but the sample looks like a one-shot (×0.8)".into());
        }
        let confidence = ((strength * margin * factor).clamp(0.0, 0.99) * 100.0).round() / 100.0;

        let mut alternatives = Vec::new();
        for (n, s) in competitors.iter().take(4) {
            let (alt, _) = route(*n, &mut Vec::new());
            if alt != final_node && !alternatives.iter().any(|a: &Alternative| a.category_id == cfg.nodes[alt].id) {
                alternatives.push(Alternative {
                    category_id: cfg.nodes[alt].id.clone(),
                    path: cfg.path_of(alt).to_string(),
                    score: (s * 100.0).round() / 100.0,
                });
            }
        }

        let has_file = direct.get(&winner).is_some_and(|d| d.1.iter().any(|&i| matches!(evidence[i].origin, Origin::Filename { .. })))
            || chain.iter().any(|n| best_file.contains_key(n));
        let has_folder = chain.iter().any(|n| best_folder.contains_key(n));
        let method = match (has_folder, has_file) {
            (true, true) => "folder-and-filename",
            (false, true) => "filename",
            (true, false) => "folder",
            (false, false) => "other",
        };

        Classification {
            category_id: cfg.nodes[final_node].id.clone(),
            category_path: cfg.path_of(final_node).to_string(),
            confidence,
            evidence: ev_out,
            alternatives,
            sample_type,
            loop_score: Some(lp.score),
            bpm,
            bpm_source,
            key,
            method: method.into(),
        }
    }

    fn fixed(&self, node: usize, st: SampleType, why: String, stem: &str) -> Classification {
        Classification {
            category_id: self.cfg.nodes[node].id.clone(),
            category_path: self.cfg.path_of(node).to_string(),
            confidence: 0.99,
            evidence: vec![why],
            alternatives: vec![],
            sample_type: st,
            loop_score: None,
            bpm: musical::bpm_from_name(stem),
            bpm_source: musical::bpm_from_name(stem).map(|_| "filename".into()),
            key: musical::key_from_name(stem),
            method: "file-type".into(),
        }
    }
}

#[cfg(test)]
mod tests;
