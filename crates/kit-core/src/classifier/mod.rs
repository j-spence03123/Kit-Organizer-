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

pub mod learned;
pub mod loops;
pub mod musical;
pub mod text;
pub mod tokenize;

use crate::audio::AudioMeta;
use crate::config::{is_ancestor, CompiledConfig, KindHint};
use crate::filetypes::FileKind;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokenize::{fine_tokens, stem};

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
    /// Extra evidence supplied by the caller, e.g. what sibling files in
    /// the same folder were classified as.
    pub hints: &'a [Hint],
}

/// Caller-supplied evidence for a category (by id).
#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    pub category_id: String,
    pub points: f32,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Origin {
    Filename { position: usize },
    Folder { distance: usize },
    Metadata,
    /// Sibling files / caller hints.
    Context,
    /// Rules learned from the user's corrections.
    Learned,
    /// Signal analysis of the audio.
    Audio,
    /// Reserved for a future ML model.
    Model,
}

impl Origin {
    /// Evidence of the same class is not double-counted: only the best
    /// item per category and class contributes.
    fn class(&self) -> usize {
        match self {
            Origin::Filename { .. } => 0,
            Origin::Folder { .. } => 1,
            Origin::Metadata => 2,
            Origin::Context => 3,
            Origin::Learned => 4,
            Origin::Audio => 5,
            Origin::Model => 6,
        }
    }
    fn method_name(&self) -> &'static str {
        match self {
            Origin::Filename { .. } => "filename",
            Origin::Folder { .. } => "folder",
            Origin::Metadata => "tags",
            Origin::Context => "siblings",
            Origin::Learned => "learned",
            Origin::Audio => "audio",
            Origin::Model => "model",
        }
    }
}

/// One piece of evidence for a category.
#[derive(Debug, Clone)]
pub struct Evidence {
    pub node: usize,
    pub points: f32,
    pub label: String,
    pub origin: Origin,
    /// Came from a weak alias.
    pub weak: bool,
}

/// Prepared, tokenised view of the input shared by all sources.
pub struct Prepared<'a> {
    pub input: &'a ClassifyInput<'a>,
    pub stem: &'a str,
    /// Filename words with the vendor prefix removed and compounds split.
    pub name_tokens: Vec<String>,
    pub vendor: Option<String>,
    /// (tokens, distance, is_root, original name)
    pub folders: Vec<(Vec<String>, usize, bool, &'a str)>,
    /// Words from embedded tags (title, genre, comment).
    pub meta_tokens: Vec<String>,
    /// Every word in the filename, folders and tags (for context rules).
    pub context: HashSet<String>,
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
    pub hits: Vec<(usize, bool, usize, bool)>, // node, weak, alias token count, auto-weak
    /// The alias a misspelt word was matched to.
    pub fuzzy: Option<String>,
}

/// Greedy longest-phrase-first matching using the tokens themselves as
/// context. See [`text::match_tokens`] for the full matcher.
pub fn match_phrases(cfg: &CompiledConfig, tokens: &[String]) -> Vec<PhraseMatch> {
    let ctx: HashSet<String> = tokens.iter().cloned().collect();
    text::match_tokens(cfg, tokens, &ctx, false)
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
        for m in text::match_tokens(cfg, &p.name_tokens, &p.context, true) {
            for &(node, weak, ntok, _) in &m.hits {
                let mut pts = w.filename + w.phrase_bonus * (ntok.saturating_sub(1)) as f32;
                pts *= node_factor(cfg, node);
                if weak {
                    pts *= w.weak_factor;
                }
                if m.fuzzy.is_some() {
                    pts *= w.fuzzy_factor;
                }
                out.push(Evidence {
                    node,
                    points: pts,
                    label: format!(
                        "Filename contains \"{}\"{}{} → {}",
                        m.text,
                        m.fuzzy.as_ref().map(|a| format!(" (probable typo of \"{a}\")")).unwrap_or_default(),
                        if weak { " (weak alias)" } else { "" },
                        cfg.path_of(node)
                    ),
                    origin: Origin::Filename { position: m.start },
                    weak,
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
            for m in text::match_tokens(cfg, toks, &p.context, false) {
                // A folder named exactly "FX" or "HH" is deliberate, so short
                // aliases are only weak when they are part of a longer name.
                let whole_name = m.start == 0 && m.len == toks.len();
                for &(node, weak, ntok, auto_weak) in &m.hits {
                    let weak = weak && !(auto_weak && whole_name);
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
                        weak,
                    });
                }
            }
        }
        out
    }
}

/// Keywords in embedded tags (WAV INFO, ID3, Vorbis comments).
pub struct MetadataSource;

impl EvidenceSource for MetadataSource {
    fn name(&self) -> &'static str {
        "metadata"
    }
    fn collect(&self, cfg: &CompiledConfig, p: &Prepared) -> Vec<Evidence> {
        let w = &cfg.source.weights;
        let mut out = Vec::new();
        for m in text::match_tokens(cfg, &p.meta_tokens, &p.context, false) {
            for &(node, weak, ntok, _) in &m.hits {
                let mut pts = (w.metadata + w.phrase_bonus * 0.5 * ntok.saturating_sub(1) as f32) * node_factor(cfg, node);
                if weak {
                    pts *= w.weak_factor;
                }
                out.push(Evidence {
                    node,
                    points: pts,
                    label: format!("Embedded tags mention \"{}\" → {}", m.text, cfg.path_of(node)),
                    origin: Origin::Metadata,
                    weak,
                });
            }
        }
        out
    }
}

/// Caller-supplied hints (sibling consistency).
pub struct ContextSource;

impl EvidenceSource for ContextSource {
    fn name(&self) -> &'static str {
        "context"
    }
    fn collect(&self, cfg: &CompiledConfig, p: &Prepared) -> Vec<Evidence> {
        p.input
            .hints
            .iter()
            .filter_map(|h| {
                let node = *cfg.by_id.get(&h.category_id)?;
                Some(Evidence { node, points: h.points, label: h.label.clone(), origin: Origin::Context, weak: false })
            })
            .collect()
    }
}

/// Supporting evidence from signal analysis. Thresholds are deliberately
/// conservative, and this evidence is only used when it agrees with the
/// names or when there is nothing else to go on (see `classify`).
pub struct AudioSource;

impl EvidenceSource for AudioSource {
    fn name(&self) -> &'static str {
        "audio"
    }
    fn collect(&self, cfg: &CompiledConfig, p: &Prepared) -> Vec<Evidence> {
        let scale = cfg.source.weights.audio;
        let Some(audio) = p.input.audio else { return vec![] };
        let Some(f) = audio.features.as_ref() else { return vec![] };
        if scale <= 0.0 {
            return vec![];
        }
        let dur = audio.duration_secs.map(|d| d as f32).unwrap_or(f.analysed_secs);
        let decays = f.decay_ms.is_some();
        let mut out = Vec::new();
        let mut add = |id: &str, pts: f32, why: String| {
            if let Some(&node) = cfg.by_id.get(id) {
                out.push(Evidence {
                    node,
                    points: pts * scale,
                    label: format!("Audio analysis: {why} → {}", cfg.path_of(node)),
                    origin: Origin::Audio,
                    weak: false,
                });
            }
        };
        let single = f.onsets <= 2;
        if f.low_ratio > 0.55 && dur < 1.5 && f.attack_ms < 30.0 && single && decays {
            add("kicks", 1.2, format!("short, punchy, bass-heavy hit ({:.0}% low end)", f.low_ratio * 100.0));
        }
        if f.low_ratio > 0.55 && dur >= 0.8 && f.decay_ms.is_none_or(|d| d > 500.0) && single {
            add("bass", 1.0, format!("sustained low-frequency tone ({:.0}% low end)", f.low_ratio * 100.0));
        }
        if f.zcr > 0.25 && f.low_ratio < 0.05 && dur < 0.6 && single && decays {
            add("hi-hats", 1.2, "short, bright, noisy hit".into());
        }
        if f.zcr > 0.2 && f.low_ratio < 0.08 && dur >= 1.0 && f.attack_ms < 50.0 && single && decays {
            add("cymbals", 1.0, "bright hit with a long decay".into());
        }
        if dur >= 2.0 && f.onsets >= 4 && f.periodicity > 0.45 && f.zcr < 0.3 {
            let what = if f.low_ratio > 0.55 { "kicks" } else { "drum-loops" };
            add(what, 1.0, format!("regular pattern of {} hits", f.onsets));
        }
        if dur >= 2.0 && f.rise_db > 12.0 && f.attack_ms > dur * 500.0 {
            add("fx/risers", 1.2, format!("level rises steadily (+{:.0} dB)", f.rise_db));
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
    #[serde(default)]
    pub key_source: Option<String>,
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
        Self {
            cfg,
            sources: vec![
                Box::new(FilenameSource),
                Box::new(FolderSource),
                Box::new(MetadataSource),
                Box::new(ContextSource),
                Box::new(AudioSource),
            ],
        }
    }

    /// Add rules learned from the user's corrections.
    pub fn with_learned(self, rules: Vec<learned::LearnedRule>) -> Self {
        self.with_source(Box::new(learned::LearnedSource::new(rules)))
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
        let (vendor, name_tokens) = text::name_tokens(cfg, stem);
        let folders: Vec<(Vec<String>, usize, bool, &str)> = input
            .folders
            .iter()
            .enumerate()
            .map(|(i, f)| (text::expand_compounds(cfg, fine_tokens(f)), n_folders - i, i == 0, f.as_str()))
            .collect();
        let mut meta_tokens = Vec::new();
        if let Some(a) = input.audio {
            let raw_name = fine_tokens(stem);
            if let Some(t) = &a.title {
                let tt = fine_tokens(t);
                // Titles that just repeat the filename add nothing.
                if tt != raw_name && tt != name_tokens {
                    meta_tokens.extend(tt);
                }
            }
            for t in [&a.genre, &a.comment].into_iter().flatten() {
                meta_tokens.extend(fine_tokens(t));
            }
            meta_tokens = text::expand_compounds(cfg, meta_tokens);
        }
        let context: HashSet<String> = name_tokens
            .iter()
            .chain(folders.iter().flat_map(|f| f.0.iter()))
            .chain(meta_tokens.iter())
            .cloned()
            .collect();
        let prepared = Prepared { input, stem, name_tokens, vendor, folders, meta_tokens, context };

        let bpm_name = musical::bpm_from_name(stem);
        let loop_folders: Vec<(Vec<String>, usize, bool)> =
            prepared.folders.iter().map(|(t, d, r, _)| (t.clone(), *d, *r)).collect();
        let lp = loops::detect(
            cfg,
            &loops::LoopInput {
                name_tokens: &prepared.name_tokens,
                meta_tokens: &prepared.meta_tokens,
                folders: &loop_folders,
                bpm_in_name: bpm_name.is_some(),
                key_known: musical::key_from_name(stem).is_some() || input.audio.and_then(|a| a.tag_key.as_ref()).is_some(),
                note_octave: musical::has_note_octave(stem),
                audio: input.audio,
            },
        );
        let sample_type = if lp.is_loop { SampleType::Loop } else { SampleType::OneShot };
        let acid = input.audio.and_then(|a| a.acid_tempo).filter(|t| *t > 0.0 && input.audio.and_then(|a| a.acid_one_shot) == Some(false));
        let tag_bpm = input.audio.and_then(|a| a.tag_bpm);
        let (bpm, bpm_source) = match (bpm_name, acid, tag_bpm) {
            (Some(b), _, _) => (Some(b), Some("filename".to_string())),
            (None, Some(t), _) => (Some((t * 100.0).round() / 100.0), Some("ACID tag".to_string())),
            (None, None, Some(t)) => (Some((t * 100.0).round() / 100.0), Some("tag".to_string())),
            _ => (None, None),
        };
        let (key, key_source) = key_of(stem, input.audio);

        // --- Evidence aggregation ---
        let mut evidence: Vec<Evidence> = Vec::new();
        for s in &self.sources {
            evidence.extend(s.collect(cfg, &prepared));
        }
        let w = &cfg.source.weights;
        // Loop routing happens *before* scoring: when the sample is a loop,
        // evidence for an instrument (Kicks) is evidence for its loop
        // destination (Drum Loops/Kick Loops), so folder context such as
        // "Drum Loops" supports it instead of competing with it.
        let mut loop_routed_nodes = std::collections::HashSet::new();
        if lp.is_loop {
            for e in evidence.iter_mut() {
                if let Some(t) = cfg.nodes[e.node].loop_target {
                    if t != e.node {
                        e.label = format!("{} (loop → {})", e.label, cfg.path_of(t));
                        e.node = t;
                        loop_routed_nodes.insert(t);
                    }
                }
            }
        }
        // A loop with a stated key but no instrument named — "cashier [133
        // BPM D minor]" — is almost always a composed melody loop. Only used
        // when no keyword gave usable evidence (a pack named "Melody Kit"
        // is too weak to count; a "Vocals" folder is not).
        let strongest = evidence.iter().filter(|e| e.origin != Origin::Audio).map(|e| e.points).fold(0.0f32, f32::max);
        if lp.is_loop && key.is_some() && strongest < w.min_score {
            if let Some(&node) = cfg.by_id.get("melodic-loops/full-melody-loops") {
                evidence.push(Evidence {
                    node,
                    points: w.key_loop,
                    label: format!(
                        "Loop with a musical key ({}) but no instrument named — typical of melody loops",
                        key.as_deref().unwrap_or("")
                    ),
                    origin: Origin::Context,
                    weak: false,
                });
            }
        }
        // Audio analysis only *supports*: it is kept when nothing else is
        // known, or when it agrees with (is related to) other evidence.
        let named: Vec<usize> = evidence.iter().filter(|e| e.origin != Origin::Audio).map(|e| e.node).collect();
        let mut ignored_audio = Vec::new();
        evidence.retain(|e| {
            let keep = e.origin != Origin::Audio
                || named.is_empty()
                || named.iter().any(|&n| n == e.node || is_ancestor(&cfg.nodes, n, e.node) || is_ancestor(&cfg.nodes, e.node, n));
            if !keep {
                ignored_audio.push(format!("{} — ignored because the names disagree", e.label));
            }
            keep
        });
        // Best item per (category, evidence class); classes are summed.
        let mut best: HashMap<(usize, usize), usize> = HashMap::new();
        for (i, e) in evidence.iter().enumerate() {
            let entry = best.entry((e.node, e.origin.class())).or_insert(i);
            if evidence[*entry].points < e.points {
                *entry = i;
            }
        }
        let best_file: HashMap<usize, usize> =
            best.iter().filter(|((_, c), _)| *c == 0).map(|((n, _), &i)| (*n, i)).collect();
        let mut direct: HashMap<usize, (f32, Vec<usize>)> = HashMap::new();
        let mut keys: Vec<_> = best.iter().collect();
        keys.sort();
        for (&(node, _), &i) in keys {
            let d = direct.entry(node).or_default();
            d.0 += evidence[i].points;
            d.1.push(i);
        }

        // Head-noun rule: when several unrelated categories are named in the
        // filename, the last one is usually the noun ("Neuro Growl").
        let mut head_bonus: Option<(usize, String)> = None;
        {
            // Weak aliases are descriptors ("melody", "groove"), never the noun.
            let mut file_nodes: Vec<(usize, usize)> = best_file
                .iter()
                .filter(|(_, &i)| !evidence[i].weak)
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
            let mut ev = vec!["No category keywords found in the filename, folders or tags".to_string()];
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
                key_source,
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
        if s1 < w.min_score {
            let mut ev = vec![format!(
                "Evidence too weak to classify (best: {} at {:.2}, minimum {:.2})",
                cfg.path_of(winner),
                s1,
                w.min_score
            )];
            ev.extend(evidence.iter().map(|e| format!("{} ({:+.2})", e.label, e.points)));
            ev.extend(lp.evidence.iter().cloned());
            return Classification {
                category_id: cfg.nodes[cfg.uncategorised].id.clone(),
                category_path: cfg.path_of(cfg.uncategorised).to_string(),
                confidence: 0.0,
                evidence: ev,
                alternatives: vec![Alternative {
                    category_id: cfg.nodes[winner].id.clone(),
                    path: cfg.path_of(winner).to_string(),
                    score: (s1 * 100.0).round() / 100.0,
                }],
                sample_type,
                loop_score: Some(lp.score),
                bpm,
                bpm_source,
                key,
                key_source,
                method: "fallback".into(),
            };
        }
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
        let (final_node, routed_now) = route(winner, &mut ev_out);
        let loop_routed = routed_now || loop_routed_nodes.contains(&winner);
        for (n, s) in competitors.iter().take(3) {
            ev_out.push(format!("Conflicting evidence for {} (score {:.2})", cfg.path_of(*n), s));
        }
        ev_out.extend(ignored_audio);

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

        // Method: which kinds of evidence support the result.
        let mut kinds: Vec<&'static str> = Vec::new();
        for &n in &chain {
            if let Some((_, idxs)) = direct.get(&n) {
                for &i in idxs {
                    let k = evidence[i].origin.method_name();
                    if !kinds.contains(&k) {
                        kinds.push(k);
                    }
                }
            }
        }
        let order = ["folder", "filename", "tags", "siblings", "learned", "audio", "model"];
        kinds.sort_by_key(|k| order.iter().position(|o| o == k));
        let method = if kinds.is_empty() { "other".to_string() } else { kinds.join("+") };

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
            key_source,
            method,
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
            key_source: musical::key_from_name(stem).map(|_| "filename".into()),
            method: "file-type".into(),
        }
    }
}

/// Key from the filename, then embedded tags, then the sampler root note.
/// The root note is ignored when it is 60 (C4), the usual default.
fn key_of(stem: &str, audio: Option<&AudioMeta>) -> (Option<String>, Option<String>) {
    if let Some(k) = musical::key_from_name(stem) {
        return (Some(k), Some("filename".into()));
    }
    if let Some(k) = audio.and_then(|a| a.tag_key.clone()) {
        return (Some(k), Some("tag".into()));
    }
    if let Some(n) = audio.and_then(|a| a.root_note).filter(|&n| n != 60) {
        return (Some(musical::note_name(n).to_string()), Some("sampler root note".into()));
    }
    (None, None)
}

#[cfg(test)]
mod tests;
