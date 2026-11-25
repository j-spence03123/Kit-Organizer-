//! Category hierarchy and classifier configuration.
//!
//! The configuration is plain serde data so it can be stored in SQLite,
//! exported to JSON and edited by the Category Manager. Categories are
//! referenced by stable `id`s (not paths), so renaming a category never
//! breaks the rules that point at it.

mod defaults;

pub use defaults::default_config;

use crate::error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// What kind of sample a category is intended for. Used for loop/one-shot
/// routing and for the Category Manager UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum KindHint {
    #[default]
    Any,
    OneShot,
    Loop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CategoryNode {
    /// Stable identifier. Defaults use slugged paths (`bass/growls`);
    /// user-created categories get random ids.
    pub id: String,
    pub name: String,
    /// Keywords/phrases that identify this category. Matching is
    /// token-aware and case/punctuation-insensitive. Prefix an alias with
    /// `~` to mark it as weak evidence (half weight). Single-token aliases of
    /// two characters or fewer (e.g. `bd`, `sd`) are automatically weak.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Tie-breaking priority (-5..=5). Each point adds
    /// `weights.priority_step` to the candidate score.
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub kind: KindHint,
    /// Where samples matching this category go when they are detected as
    /// loops (e.g. Kicks -> Drum Loops/Kick Loops).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_target: Option<String>,
    /// For parent categories: where samples go when only the parent matched
    /// (e.g. Bass -> Bass/Other Basses). When unset files land in the
    /// parent folder itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_child: Option<String>,
    #[serde(default)]
    pub children: Vec<CategoryNode>,
}

/// Tunable scoring weights. See `classifier` module docs for how they
/// combine into a confidence value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierWeights {
    /// Points for a filename keyword match on a leaf category.
    pub filename: f32,
    /// Extra points per additional token in a multi-word phrase match.
    pub phrase_bonus: f32,
    /// Points for a keyword in the file's immediate parent folder.
    pub folder_near: f32,
    /// Multiplier applied per extra level of folder distance.
    pub folder_decay: f32,
    /// Multiplier for evidence found in the imported root folder (usually
    /// the pack name, which is weak evidence).
    pub root_folder_factor: f32,
    /// Multiplier for evidence pointing at a parent (non-leaf) category.
    pub parent_factor: f32,
    /// Multiplier for weak aliases.
    pub weak_factor: f32,
    /// Fraction of an ancestor's direct evidence that supports its
    /// descendants (a "Bass" folder supports "Bass/Growls").
    pub ancestor_support: f32,
    /// Bonus for the last filename match when several conflict
    /// ("Neuro Growl": growl is the head noun).
    pub head_noun_bonus: f32,
    /// Score added per point of category priority.
    pub priority_step: f32,
    /// Saturation constant: strength = 1 - exp(-score / saturation).
    pub saturation: f32,
    /// Loop score at or above which a sample is treated as a loop.
    pub loop_threshold: f32,
    /// Below this best score the evidence is too weak to act on and the
    /// sample goes to Uncategorised.
    #[serde(default = "default_min_score")]
    pub min_score: f32,
    /// Points for a keyword in embedded tags (title/genre/comment).
    #[serde(default = "d_metadata")]
    pub metadata: f32,
    /// Multiplier for typo-tolerant (fuzzy) filename matches.
    #[serde(default = "d_fuzzy")]
    pub fuzzy_factor: f32,
    /// Points for an exact match of a rule learned from your corrections
    /// in the same pack/vendor (60% elsewhere, half for partial matches).
    #[serde(default = "d_learned")]
    pub learned: f32,
    /// Points from sibling files: multiplied by the share of confident
    /// files in the same folder that agree.
    #[serde(default = "d_sibling")]
    pub sibling: f32,
    /// Multiplier for audio-analysis evidence (0 disables it).
    #[serde(default = "d_audio")]
    pub audio: f32,
    /// Points for "loop with a key but no instrument named" → Full Melody
    /// Loops (only when nothing else matched).
    #[serde(default = "d_key_loop")]
    pub key_loop: f32,
}

fn d_key_loop() -> f32 {
    2.5
}

fn d_metadata() -> f32 {
    1.5
}
fn d_fuzzy() -> f32 {
    0.6
}
fn d_learned() -> f32 {
    4.0
}
fn d_sibling() -> f32 {
    2.5
}
fn d_audio() -> f32 {
    1.0
}

fn default_min_score() -> f32 {
    0.75
}

impl Default for ClassifierWeights {
    fn default() -> Self {
        Self {
            filename: 3.0,
            phrase_bonus: 0.5,
            folder_near: 2.0,
            folder_decay: 0.75,
            root_folder_factor: 0.4,
            parent_factor: 0.6,
            weak_factor: 0.5,
            ancestor_support: 0.5,
            head_noun_bonus: 0.3,
            priority_step: 0.25,
            saturation: 2.0,
            loop_threshold: 2.0,
            min_score: default_min_score(),
            metadata: d_metadata(),
            fuzzy_factor: d_fuzzy(),
            learned: d_learned(),
            sibling: d_sibling(),
            audio: d_audio(),
            key_loop: d_key_loop(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CategoryConfig {
    pub version: u32,
    /// Folder created inside the chosen output directory.
    pub root_folder: String,
    pub categories: Vec<CategoryNode>,
    pub weights: ClassifierWeights,
    /// Folder names that carry little classification information.
    pub generic_folder_terms: Vec<String>,
    /// Terms that indicate a loop.
    pub loop_terms: Vec<String>,
    /// Terms that indicate a one-shot.
    pub one_shot_terms: Vec<String>,
    pub midi_category: String,
    pub preset_category: String,
    pub uncategorised_category: String,
    /// Known vendor/label prefixes ignored when matching filenames
    /// (`XLNT_`, `BOS_`). Unknown ALL-CAPS prefixes are detected too.
    #[serde(default = "default_vendor_prefixes")]
    pub vendor_prefixes: Vec<String>,
}

pub fn default_vendor_prefixes() -> Vec<String> {
    [
        "xlnt", "bos", "kshmr", "splice", "cymatics", "ghosthack", "loopmasters", "vengeance", "vec", "sm", "sm101",
        "ph", "pml", "ss", "ue", "oc", "ppr", "lm", "zen", "rv", "dlb", "kompany", "subtronics", "wvwd", "ww",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Flattened, index-based view of a category for fast lookups.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent: Option<usize>,
    pub depth: usize,
    pub children: Vec<usize>,
    pub kind: KindHint,
    pub loop_target: Option<usize>,
    pub default_child: Option<usize>,
    pub priority: i32,
}

impl NodeInfo {
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

/// A single alias entry in the compiled lookup table.
#[derive(Debug, Clone)]
pub struct AliasHit {
    pub node: usize,
    pub weak: bool,
    /// Weak only because it is a very short single token (not marked `~`).
    pub auto_weak: bool,
    /// Context words, one of which must appear somewhere in the filename,
    /// folders or tags (`roll@snare|drum`).
    pub requires: Vec<String>,
    pub tokens: usize,
    pub alias: String,
}

/// Compiled, validated configuration ready for classification.
#[derive(Debug, Clone)]
pub struct CompiledConfig {
    pub source: CategoryConfig,
    pub nodes: Vec<NodeInfo>,
    pub by_id: HashMap<String, usize>,
    pub by_path: HashMap<String, usize>,
    pub aliases: HashMap<String, Vec<AliasHit>>,
    pub max_phrase: usize,
    pub generic_folders: HashSet<String>,
    pub loop_terms: HashMap<String, usize>,
    pub one_shot_terms: HashMap<String, usize>,
    pub midi: usize,
    pub preset: usize,
    pub uncategorised: usize,
    /// Every word used by any alias or term (for compound splitting and
    /// vendor-prefix detection).
    pub vocab: HashSet<String>,
    /// Single-word aliases of 4+ letters (typo-tolerant matching).
    pub fuzzy_aliases: Vec<String>,
    pub vendor_prefixes: HashSet<String>,
}

/// Normalise text into the keys used by the alias table: lowercase tokens
/// joined by single spaces.
pub fn normalise_phrase(s: &str) -> Vec<String> {
    crate::classifier::tokenize::fine_tokens(s)
}

impl CompiledConfig {
    pub fn compile(cfg: &CategoryConfig) -> CoreResult<Self> {
        let mut nodes = Vec::new();
        let mut by_id = HashMap::new();
        fn walk(
            list: &[CategoryNode],
            parent: Option<usize>,
            prefix: &str,
            depth: usize,
            nodes: &mut Vec<NodeInfo>,
            by_id: &mut HashMap<String, usize>,
        ) -> CoreResult<Vec<usize>> {
            let mut out = Vec::new();
            let mut seen_names = HashSet::new();
            for n in list {
                validate_name(&n.name)?;
                if !seen_names.insert(n.name.to_lowercase()) {
                    return Err(CoreError::InvalidConfig(format!(
                        "duplicate category name '{}' under '{}'",
                        n.name,
                        if prefix.is_empty() { "root" } else { prefix }
                    )));
                }
                let path = if prefix.is_empty() { n.name.clone() } else { format!("{prefix}/{}", n.name) };
                let idx = nodes.len();
                if by_id.insert(n.id.clone(), idx).is_some() {
                    return Err(CoreError::InvalidConfig(format!("duplicate category id '{}'", n.id)));
                }
                nodes.push(NodeInfo {
                    id: n.id.clone(),
                    name: n.name.clone(),
                    path: path.clone(),
                    parent,
                    depth,
                    children: vec![],
                    kind: n.kind,
                    loop_target: None,
                    default_child: None,
                    priority: n.priority.clamp(-5, 5),
                });
                let kids = walk(&n.children, Some(idx), &path, depth + 1, nodes, by_id)?;
                nodes[idx].children = kids;
                out.push(idx);
            }
            Ok(out)
        }
        walk(&cfg.categories, None, "", 0, &mut nodes, &mut by_id)?;

        let resolve = |id: &str, what: &str| -> CoreResult<usize> {
            by_id.get(id).copied().ok_or_else(|| CoreError::InvalidConfig(format!("{what} refers to unknown category id '{id}'")))
        };

        // Second pass: references and aliases.
        let mut aliases: HashMap<String, Vec<AliasHit>> = HashMap::new();
        let mut max_phrase = 1;
        fn each<'a>(list: &'a [CategoryNode], f: &mut dyn FnMut(&'a CategoryNode)) {
            for n in list {
                f(n);
                each(&n.children, f);
            }
        }
        let mut err = None;
        each(&cfg.categories, &mut |n| {
            let idx = by_id[&n.id];
            if let Some(t) = &n.loop_target {
                match resolve(t, &format!("loop target of '{}'", n.name)) {
                    Ok(i) => nodes[idx].loop_target = Some(i),
                    Err(e) => err = Some(e),
                }
            }
            if let Some(t) = &n.default_child {
                match resolve(t, &format!("default child of '{}'", n.name)) {
                    Ok(i) => nodes[idx].default_child = Some(i),
                    Err(e) => err = Some(e),
                }
            }
            for raw in &n.aliases {
                let (weak_marked, text) = match raw.strip_prefix('~') {
                    Some(rest) => (true, rest),
                    None => (false, raw.as_str()),
                };
                let (text, requires) = match text.split_once('@') {
                    Some((a, req)) => (
                        a,
                        req.split('|').map(|r| normalise_phrase(r).join(" ")).filter(|r| !r.is_empty()).collect::<Vec<_>>(),
                    ),
                    None => (text, Vec::new()),
                };
                let toks = normalise_phrase(text);
                if toks.is_empty() {
                    continue;
                }
                let auto_weak = !weak_marked && toks.len() == 1 && toks[0].chars().count() <= 2;
                let weak = weak_marked || auto_weak;
                max_phrase = max_phrase.max(toks.len());
                let hit = AliasHit { node: idx, weak, auto_weak, requires, tokens: toks.len(), alias: text.trim().to_string() };
                let spaced = toks.join(" ");
                push_unique(&mut aliases, spaced, hit.clone());
                if toks.len() > 1 {
                    push_unique(&mut aliases, toks.concat(), hit);
                }
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
        // Validate default children really are descendants.
        for i in 0..nodes.len() {
            if let Some(c) = nodes[i].default_child {
                if !is_ancestor(&nodes, i, c) {
                    return Err(CoreError::InvalidConfig(format!(
                        "default child of '{}' must be one of its subcategories",
                        nodes[i].path
                    )));
                }
            }
        }

        let by_path = nodes.iter().enumerate().map(|(i, n)| (n.path.to_lowercase(), i)).collect();
        let term_map = |terms: &[String]| -> HashMap<String, usize> {
            terms
                .iter()
                .filter_map(|t| {
                    let toks = normalise_phrase(t);
                    (!toks.is_empty()).then(|| (toks.join(" "), toks.len()))
                })
                .collect()
        };
        let generic_folders =
            cfg.generic_folder_terms.iter().map(|t| normalise_phrase(t).join(" ")).filter(|s| !s.is_empty()).collect();
        let mut vocab: HashSet<String> = HashSet::new();
        for key in aliases.keys() {
            vocab.extend(key.split(' ').map(String::from));
        }
        for t in cfg.loop_terms.iter().chain(&cfg.one_shot_terms) {
            vocab.extend(normalise_phrase(t));
        }
        let mut fuzzy_aliases: Vec<String> = aliases
            .keys()
            .filter(|k| !k.contains(' ') && k.chars().count() >= 4 && k.chars().all(char::is_alphabetic))
            .cloned()
            .collect();
        fuzzy_aliases.sort();

        Ok(Self {
            midi: resolve(&cfg.midi_category, "MIDI category")?,
            preset: resolve(&cfg.preset_category, "preset category")?,
            uncategorised: resolve(&cfg.uncategorised_category, "uncategorised category")?,
            loop_terms: term_map(&cfg.loop_terms),
            one_shot_terms: term_map(&cfg.one_shot_terms),
            generic_folders,
            vocab,
            fuzzy_aliases,
            vendor_prefixes: cfg.vendor_prefixes.iter().map(|v| v.to_lowercase()).collect(),
            source: cfg.clone(),
            nodes,
            by_id,
            by_path,
            aliases,
            max_phrase,
        })
    }

    pub fn path_of(&self, idx: usize) -> &str {
        &self.nodes[idx].path
    }

    pub fn find_path(&self, path: &str) -> Option<usize> {
        self.by_path.get(&path.trim_matches('/').to_lowercase()).copied()
    }
}

fn push_unique(map: &mut HashMap<String, Vec<AliasHit>>, key: String, hit: AliasHit) {
    let v = map.entry(key).or_default();
    if let Some(existing) = v.iter_mut().find(|h| h.node == hit.node) {
        // Same node listed twice: keep the strongest variant.
        if existing.weak && !hit.weak {
            *existing = hit;
        }
    } else {
        v.push(hit);
    }
}

/// True when `anc` is a strict ancestor of `node`.
pub fn is_ancestor(nodes: &[NodeInfo], anc: usize, node: usize) -> bool {
    let mut cur = nodes[node].parent;
    while let Some(p) = cur {
        if p == anc {
            return true;
        }
        cur = nodes[p].parent;
    }
    false
}

/// Category names become folder names, so they must be valid on every
/// supported OS.
pub fn validate_name(name: &str) -> CoreResult<()> {
    let trimmed = name.trim();
    let bad = trimmed.is_empty()
        || trimmed != name
        || name == "."
        || name == ".."
        || name.ends_with('.')
        || name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control());
    let reserved = ["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "lpt1", "lpt2", "lpt3"];
    if bad || reserved.contains(&name.to_lowercase().as_str()) {
        return Err(CoreError::InvalidConfig(format!("'{name}' is not a valid folder name")));
    }
    Ok(())
}

impl CategoryConfig {
    pub fn from_json(s: &str) -> CoreResult<Self> {
        let cfg: CategoryConfig = serde_json::from_str(s)?;
        CompiledConfig::compile(&cfg)?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_compiles() {
        let c = CompiledConfig::compile(&default_config()).unwrap();
        assert!(c.find_path("Bass/Growls").is_some());
        assert!(c.find_path("One-Shots/Keys/Pianos").is_some());
        assert!(c.find_path("Uncategorised").is_some());
        println!("{} categories", c.nodes.len());
        assert!(c.nodes.len() > 130);
    }

    #[test]
    fn rejects_bad_names_and_refs() {
        let mut cfg = default_config();
        cfg.categories[0].name = "Ba/ss".into();
        assert!(CompiledConfig::compile(&cfg).is_err());
        let mut cfg = default_config();
        cfg.categories[0].loop_target = Some("nope".into());
        assert!(CompiledConfig::compile(&cfg).is_err());
    }

    #[test]
    fn json_round_trip() {
        let cfg = default_config();
        let s = serde_json::to_string(&cfg).unwrap();
        assert_eq!(CategoryConfig::from_json(&s).unwrap(), cfg);
    }
}
