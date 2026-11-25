//! Stage 4: loop vs one-shot detection.
//!
//! Each indicator adds or subtracts points; the sample is a loop when the
//! total reaches `weights.loop_threshold` (default 2.0). No single weak
//! indicator (a BPM tag, a long duration) is enough on its own — they have
//! to agree.
//!
//! | Indicator                                   | Points |
//! |---------------------------------------------|--------|
//! | Loop term in filename (`loop`, `groove`…)    | +3.0   |
//! | Loop term in folder (decays with distance)   | +2.0   |
//! | One-shot term in filename (`one shot`, `hit`)| −3.0   |
//! | One-shot term in folder (decays)             | −2.0   |
//! | BPM stated in filename                       | +1.5   |
//! | …and a key stated too                        | +1.0   |
//! | WAV ACID chunk: loop with tempo              | +2.0   |
//! | WAV ACID chunk: one-shot flag                | −2.0   |
//! | WAV sampler loop points (`smpl`)             | +0.5   |
//! | Duration ≥ 4 s                               | +0.5   |
//! | Duration < 1.5 s                             | −1.0   |
//! | Note + octave in name (`C4`)                 | −0.5   |
//! | Loop / one-shot term in embedded tags        | ±1.5   |
//! | BPM stored in embedded tags                  | +1.0   |
//! | Audio analysis: regular rhythmic pattern     | +1.5   |
//! | Audio analysis: single decaying hit          | −1.0   |

use super::tokenize::singular;
use crate::audio::AudioMeta;
use crate::config::CompiledConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopDecision {
    pub is_loop: bool,
    pub score: f32,
    pub ambiguous: bool,
    pub evidence: Vec<String>,
}

pub struct LoopInput<'a> {
    pub name_tokens: &'a [String],
    /// Words from embedded tags.
    pub meta_tokens: &'a [String],
    /// (tokens, distance from file: 1 = parent, is_import_root)
    pub folders: &'a [(Vec<String>, usize, bool)],
    pub bpm_in_name: bool,
    /// A musical key is stated in the name or tags.
    pub key_known: bool,
    pub note_octave: bool,
    pub audio: Option<&'a AudioMeta>,
}

fn find_term(tokens: &[String], terms: &HashMap<String, usize>) -> Option<String> {
    for n in (1..=2).rev() {
        for w in tokens.windows(n) {
            let key = w.join(" ");
            if terms.contains_key(&key) {
                return Some(key);
            }
            if let Some(s) = singular(&w[n - 1]) {
                let mut v = w.to_vec();
                v[n - 1] = s;
                let key = v.join(" ");
                if terms.contains_key(&key) {
                    return Some(key);
                }
            }
        }
    }
    None
}

pub fn detect(cfg: &CompiledConfig, input: &LoopInput) -> LoopDecision {
    let w = &cfg.source.weights;
    let mut score = 0.0f32;
    let mut ev = Vec::new();
    let mut add = |pts: f32, why: String, ev: &mut Vec<String>| {
        score += pts;
        ev.push(format!("{why} ({pts:+.1})"));
    };

    if let Some(t) = find_term(input.name_tokens, &cfg.loop_terms) {
        add(3.0, format!("Filename contains loop term \"{t}\""), &mut ev);
    }
    if let Some(t) = find_term(input.name_tokens, &cfg.one_shot_terms) {
        add(-3.0, format!("Filename contains one-shot term \"{t}\""), &mut ev);
    }
    if let Some(t) = find_term(input.meta_tokens, &cfg.loop_terms) {
        add(1.5, format!("Embedded tags contain loop term \"{t}\""), &mut ev);
    }
    if let Some(t) = find_term(input.meta_tokens, &cfg.one_shot_terms) {
        add(-1.5, format!("Embedded tags contain one-shot term \"{t}\""), &mut ev);
    }
    // Strongest folder signal of each type only, so "Loops/Loops" does not
    // double-count.
    let mut best_loop: Option<(f32, String)> = None;
    let mut best_os: Option<(f32, String)> = None;
    for (toks, dist, _root) in input.folders {
        let factor = w.folder_decay.powi(*dist as i32 - 1);
        if let Some(t) = find_term(toks, &cfg.loop_terms) {
            let p = 2.0 * factor;
            if best_loop.as_ref().is_none_or(|(b, _)| p > *b) {
                best_loop = Some((p, t));
            }
        }
        if let Some(t) = find_term(toks, &cfg.one_shot_terms) {
            let p = 2.0 * factor;
            if best_os.as_ref().is_none_or(|(b, _)| p > *b) {
                best_os = Some((p, t));
            }
        }
    }
    if let Some((p, t)) = best_loop {
        add(p, format!("Folder indicates loops (\"{t}\")"), &mut ev);
    }
    if let Some((p, t)) = best_os {
        add(-p, format!("Folder indicates one-shots (\"{t}\")"), &mut ev);
    }
    if input.bpm_in_name {
        add(1.5, "BPM stated in filename".into(), &mut ev);
        if input.key_known {
            // One-shots are tuned to a note but almost never tagged with a tempo.
            add(1.0, "Both tempo and key are stated".into(), &mut ev);
        }
    }
    if input.note_octave {
        add(-0.5, "Note with octave in filename".into(), &mut ev);
    }
    if let Some(a) = input.audio {
        match (a.acid_one_shot, a.acid_tempo) {
            (Some(true), _) => add(-2.0, "ACID metadata marks a one-shot".into(), &mut ev),
            (Some(false), Some(t)) if t > 0.0 => add(2.0, format!("ACID metadata marks a loop at {t:.0} BPM"), &mut ev),
            _ => {}
        }
        if a.tag_bpm.is_some() && !input.bpm_in_name {
            add(1.0, "BPM stored in embedded tags".into(), &mut ev);
        }
        if let Some(f) = a.features.as_ref().filter(|_| w.audio > 0.0) {
            if f.analysed_secs >= 2.0 && f.onsets >= 4 && f.periodicity > 0.45 {
                add(1.5 * w.audio, format!("Audio analysis: regular rhythmic pattern ({} hits)", f.onsets), &mut ev);
            } else if f.onsets <= 1 && f.decay_ms.is_some() && f.analysed_secs < 3.0 {
                add(-w.audio, "Audio analysis: single hit that decays".into(), &mut ev);
            }
        }
        if a.smpl_loop_count.unwrap_or(0) > 0 {
            add(0.5, "Sampler loop points present".into(), &mut ev);
        }
        if let Some(d) = a.duration_secs {
            if d < 1.5 {
                add(-1.0, format!("Short duration ({d:.2}s)"), &mut ev);
            } else if d >= 4.0 {
                add(0.5, format!("Long duration ({d:.1}s)"), &mut ev);
            }
        }
    }
    let is_loop = score >= w.loop_threshold;
    LoopDecision { is_loop, score, ambiguous: (score - w.loop_threshold).abs() < 1.0, evidence: ev }
}
