//! Smarter name matching: vendor-prefix stripping, compound splitting,
//! context-dependent aliases and typo tolerance.

use super::tokenize::{coarse_tokens, fine_tokens, singular};
use super::PhraseMatch;
use crate::config::CompiledConfig;
use std::collections::HashSet;

/// Split a leading vendor/label prefix off a filename stem
/// (`XLNT_Growl_F` → vendor `xlnt`, rest `Growl_F`). A prefix is a known
/// vendor name, or an ALL-CAPS 2–6 letter word that is not a keyword
/// (`BOS_TT_Kick` strips `BOS` and `TT`). At most two words are stripped
/// and at least one word is always kept.
pub fn split_vendor(cfg: &CompiledConfig, stem: &str) -> (Option<String>, String) {
    let toks = coarse_tokens(stem);
    let mut skip = 0;
    while skip < 2 && skip + 1 < toks.len() {
        let t = &toks[skip];
        let lower = t.to_lowercase();
        let known = cfg.vendor_prefixes.contains(&lower);
        // ALL-CAPS code, optionally numbered: "XLNT", "VEE2", "VDUB2", "VTDS3".
        let letters: String = t.chars().take_while(|c| c.is_ascii_uppercase()).collect();
        let digits = &t[letters.len()..];
        let letters_lower = letters.to_lowercase();
        let caps = (2..=6).contains(&letters.len())
            && digits.len() <= 2
            && digits.chars().all(|c| c.is_ascii_digit())
            && !cfg.vocab.contains(&letters_lower)
            && singular(&letters_lower).is_none_or(|s| !cfg.vocab.contains(&s));
        if known || caps {
            skip += 1;
        } else {
            break;
        }
    }
    if skip == 0 {
        return (None, stem.to_string());
    }
    (Some(toks[0].to_lowercase()), toks[skip..].join("_"))
}

fn in_vocab(cfg: &CompiledConfig, t: &str) -> bool {
    cfg.vocab.contains(t) || singular(t).is_some_and(|s| cfg.vocab.contains(&s))
}

/// Split run-together words into known keywords: `growlbass` → `growl
/// bass`, `vocalchop` → `vocal chop`. Only words of 6+ letters that are not
/// already known are split, into 2–3 known words of 3+ letters each. If
/// that fails, one unknown word is allowed next to a known word of 4+
/// letters: `houseloops` → `house loops`, `filtachords` → `filta chords`.
pub fn expand_compounds(cfg: &CompiledConfig, tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(tokens.len());
    for t in tokens {
        if t.chars().count() >= 6 && t.chars().all(|c| c.is_ascii_alphabetic()) && !in_vocab(cfg, &t) {
            if let Some(parts) = segment(cfg, &t).or_else(|| segment_one_unknown(cfg, &t)) {
                out.extend(parts);
                continue;
            }
        }
        out.push(t);
    }
    out
}

fn segment(cfg: &CompiledConfig, t: &str) -> Option<Vec<String>> {
    let n = t.len();
    // best[i] = fewest pieces covering t[..i]
    let mut best: Vec<Option<(usize, usize)>> = vec![None; n + 1]; // (pieces, prev)
    best[0] = Some((0, 0));
    for i in 0..n {
        let Some((pieces, _)) = best[i] else { continue };
        if pieces >= 3 {
            continue;
        }
        for j in (i + 3)..=n {
            if in_vocab(cfg, &t[i..j]) && best[j].is_none_or(|(p, _)| p > pieces + 1) {
                best[j] = Some((pieces + 1, i));
            }
        }
    }
    let (pieces, _) = best[n]?;
    if pieces < 2 {
        return None;
    }
    let mut parts = Vec::new();
    let mut j = n;
    while j > 0 {
        let (_, i) = best[j]?;
        parts.push(t[i..j].to_string());
        j = i;
    }
    parts.reverse();
    Some(parts)
}

fn segment_one_unknown(cfg: &CompiledConfig, t: &str) -> Option<Vec<String>> {
    if t.len() > 16 {
        return None;
    }
    // Prefer the longest known word, at the end ("houseloops") or start ("kickstart" is not split: needs 4+).
    let mut best: Option<(usize, Vec<String>)> = None;
    // The unknown part must be 4+ letters: "outbreak" is not "out" + "break".
    for i in 4..=t.len().saturating_sub(4) {
        let (a, b) = (&t[..i], &t[i..]);
        // "epiano" is a keyword, but in "housepiano" the word is "piano":
        // skip a match that is just a shorter known word with a letter in front.
        let shorter_known = b.len() > 4 && in_vocab(cfg, &b[1..]);
        if in_vocab(cfg, b) && !in_vocab(cfg, a) && !shorter_known && best.as_ref().is_none_or(|(l, _)| b.len() > *l) {
            best = Some((b.len(), vec![a.to_string(), b.to_string()]));
        }
    }
    for i in 4..=t.len().saturating_sub(4) {
        let (a, b) = (&t[..i], &t[i..]);
        if in_vocab(cfg, a) && !in_vocab(cfg, b) && a.len() >= 5 && best.as_ref().is_none_or(|(l, _)| a.len() > *l) {
            best = Some((a.len(), vec![a.to_string(), b.to_string()]));
        }
    }
    best.map(|(_, v)| v)
}

/// Optimal-string-alignment edit distance (adjacent swaps count as one).
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// Closest single-word alias for a misspelt word: same first letter, one
/// edit for words of 5–7 letters, two for 8+. Ambiguous ties are rejected.
fn fuzzy_alias<'a>(cfg: &'a CompiledConfig, tok: &str) -> Option<&'a str> {
    let len = tok.chars().count();
    if len < 5 || !tok.chars().all(|c| c.is_ascii_alphabetic()) || in_vocab(cfg, tok) {
        return None;
    }
    let max = if len >= 8 { 2 } else { 1 };
    let first = tok.chars().next()?;
    let mut best: Option<(usize, &str)> = None;
    let mut tie = false;
    for a in cfg.fuzzy_aliases.iter().filter(|a| a.starts_with(first) && a.chars().count().abs_diff(len) <= max) {
        let d = edit_distance(tok, a);
        if d == 0 || d > max {
            continue;
        }
        match best {
            Some((bd, ba)) if d == bd && cfg.aliases[a.as_str()][0].node != cfg.aliases[ba][0].node => tie = true,
            Some((bd, _)) if d < bd => {
                best = Some((d, a));
                tie = false;
            }
            None => best = Some((d, a)),
            _ => {}
        }
    }
    if tie {
        None
    } else {
        best.map(|(_, a)| a)
    }
}

fn requirement_met(req: &[String], context: &HashSet<String>) -> bool {
    req.is_empty() || req.iter().any(|r| r.split(' ').all(|w| context.contains(w)))
}

/// Greedy longest-phrase-first matching. Aliases with context requirements
/// only match when a required word appears in `context`. With
/// `allow_fuzzy`, misspelt words are matched only if nothing matched
/// exactly (so a correctly named file never gets a speculative match).
pub fn match_tokens(cfg: &CompiledConfig, tokens: &[String], context: &HashSet<String>, allow_fuzzy: bool) -> Vec<PhraseMatch> {
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
                    // "Loop 808" is file number 808, "808 Long" is an 808:
                    // a number at the very end of a name only counts as weak.
                    let index_like = n == 1 && i + 1 == tokens.len() && i > 0 && w[0].chars().all(|c| c.is_ascii_digit());
                    let hits: Vec<_> = hits
                        .iter()
                        .filter(|h| requirement_met(&h.requires, context))
                        .map(|h| (h.node, h.weak || index_like, h.tokens, h.auto_weak))
                        .collect();
                    if hits.is_empty() {
                        continue;
                    }
                    out.push(PhraseMatch { start: i, len: n, text: w.join(" "), hits, fuzzy: None });
                    i += n;
                    continue 'outer;
                }
            }
        }
        i += 1;
    }
    if out.is_empty() && allow_fuzzy {
        for (i, t) in tokens.iter().enumerate() {
            if let Some(a) = fuzzy_alias(cfg, t) {
                let hits: Vec<_> = cfg.aliases[a]
                    .iter()
                    .filter(|h| requirement_met(&h.requires, context))
                    .map(|h| (h.node, h.weak, h.tokens, h.auto_weak))
                    .collect();
                if !hits.is_empty() {
                    out.push(PhraseMatch { start: i, len: 1, text: t.clone(), hits, fuzzy: Some(a.to_string()) });
                }
            }
        }
    }
    out
}

/// Remove producer credits: `@handle`, `[prod. name]`, `(prod by name)`,
/// and a trailing `prod. name`. They name people, not sounds, and their
/// words could otherwise match keywords.
pub fn strip_credits(stem: &str) -> String {
    let mut s = String::with_capacity(stem.len());
    let mut chars = stem.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '@' {
            while chars.peek().is_some_and(|n| !n.is_whitespace() && !matches!(n, '[' | '(' | ')' | ']')) {
                chars.next();
            }
            s.push(' ');
            continue;
        }
        s.push(c);
    }
    let lower = s.to_lowercase();
    // Bracketed credits.
    let mut out = String::new();
    let mut i = 0;
    let b = s.as_bytes();
    while i < s.len() {
        let ch = b[i] as char;
        if (ch == '[' || ch == '(') && lower[i + 1..].trim_start().starts_with("prod") {
            let close = if ch == '[' { ']' } else { ')' };
            match s[i..].find(close) {
                Some(j) => {
                    i += j + 1;
                    out.push(' ');
                    continue;
                }
                None => break,
            }
        }
        let n = s[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        out.push_str(&s[i..i + n]);
        i += n;
    }
    // Unbracketed "prod. by name" / "prod name" at the end.
    let lo = out.to_lowercase();
    if let Some(p) = lo.rfind(" prod") {
        let tail = &lo[p + 5..];
        if tail.starts_with('.') || tail.starts_with(' ') || tail.starts_with("by") || tail.starts_with("uced") {
            out.truncate(p);
        }
    }
    out
}

/// Fine tokens of a filename stem with credits and the vendor prefix
/// removed and compounds expanded.
pub fn name_tokens(cfg: &CompiledConfig, stem: &str) -> (Option<String>, Vec<String>) {
    let cleaned = strip_credits(stem);
    let (vendor, rest) = split_vendor(cfg, &cleaned);
    let rest = drop_chord_names(&rest);
    (vendor, expand_compounds(cfg, fine_tokens(&rest)))
}

/// Remove chord/key names written in musical case — "Fm", "A#m", "Cmaj",
/// "Bbmin", "A#sus4" — so "Fm" is not read as FM synthesis. All-caps "FM"
/// is left alone.
pub fn drop_chord_names(s: &str) -> String {
    coarse_tokens(s)
        .into_iter()
        .filter(|t| !is_chord_name(t))
        .collect::<Vec<_>>()
        .join("_")
}

fn is_chord_name(t: &str) -> bool {
    let mut chars = t.chars();
    let Some(first) = chars.next() else { return false };
    if !('A'..='G').contains(&first) {
        return false;
    }
    let mut rest: &str = &t[1..];
    if let Some(r) = rest.strip_prefix('#').or_else(|| rest.strip_prefix('b')) {
        rest = r;
    }
    matches!(rest, "m" | "min" | "maj" | "dim" | "aug" | "m7" | "maj7" | "7" | "9" | "sus2" | "sus4" | "add9" | "m9" | "min7")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{default_config, CompiledConfig};

    fn cfg() -> CompiledConfig {
        CompiledConfig::compile(&default_config()).unwrap()
    }

    #[test]
    fn vendor_prefixes() {
        let c = cfg();
        assert_eq!(split_vendor(&c, "XLNT_Growl_F"), (Some("xlnt".into()), "Growl_F".into()));
        assert_eq!(split_vendor(&c, "BOS_TT_Kick_01"), (Some("bos".into()), "Kick_01".into()));
        assert_eq!(split_vendor(&c, "Kompany_Bass_Wub"), (Some("kompany".into()), "Bass_Wub".into()));
        assert_eq!(split_vendor(&c, "OH_02").0, None, "keywords are never vendors");
        assert_eq!(split_vendor(&c, "BD_01").0, None);
        assert_eq!(split_vendor(&c, "FX_Riser").0, None);
        assert_eq!(split_vendor(&c, "XLNT").0, None, "never strip everything");
    }

    #[test]
    fn compounds() {
        let c = cfg();
        let v = |s: &str| expand_compounds(&c, fine_tokens(s));
        assert_eq!(v("growlbass_01"), ["growl", "bass", "01"]);
        assert_eq!(v("vocalchop"), ["vocalchop"], "joined alias forms match directly");
        assert_eq!(v("screechbass"), ["screech", "bass"]);
        assert_eq!(v("kicksub"), ["kick", "sub"]);
        assert_eq!(v("hihat"), ["hihat"], "known compounds are kept");
        assert_eq!(v("closedhat"), ["closedhat"]);
        assert_eq!(v("whatever"), ["whatever"]);
    }

    #[test]
    fn numbered_vendor_codes_and_partial_compounds() {
        let c = cfg();
        assert_eq!(split_vendor(&c, "VEE2 Electro Loop 001"), (Some("vee2".into()), "Electro_Loop_001".into()));
        assert_eq!(split_vendor(&c, "VDUB2 ML 4 140 Bpm").1, "4_140_Bpm");
        assert_eq!(split_vendor(&c, "TR808 Kick").0, None, "three digits is a model number");
        let v = |s: &str| expand_compounds(&c, fine_tokens(s));
        assert_eq!(v("Houseloops"), ["house", "loops"]);
        assert_eq!(v("Housepiano"), ["house", "piano"]);
        assert_eq!(v("Filtachords"), ["filta", "chords"]);
        assert_eq!(v("monsterunderyourbed"), ["monsterunderyourbed"]);
        assert_eq!(v("Outbreak"), ["outbreak"]);
        assert_eq!(v("Organity"), ["organity"]);
    }

    #[test]
    fn chord_names_are_not_keywords() {
        assert_eq!(drop_chord_names("Melody 37 Fm D# A#m C# 140BPM"), "Melody_37_D#_C#_140BPM");
        assert_eq!(drop_chord_names("FM_Bass_03"), "FM_Bass_03");
        assert_eq!(drop_chord_names("Melody 27 Fm D# A#sus4"), "Melody_27_D#");
    }

    #[test]
    fn credits() {
        let t = |s: &str| fine_tokens(&strip_credits(s));
        assert_eq!(t("cashier [133 BPM D minor] @fivethreesosa"), ["cashier", "133", "bpm", "d", "minor"]);
        assert_eq!(t("alien 155BPM (g4) [prod. slime jax]"), ["alien", "155", "bpm", "g", "4"]);
        assert_eq!(t("goons @a @b"), ["goons"]);
        assert_eq!(t("Dark Keys 140 prod. by someone"), ["dark", "keys", "140"]);
        assert_eq!(t("Kick_Prodigy_01"), ["kick", "prodigy", "01"]);
    }

    #[test]
    fn distance() {
        assert_eq!(edit_distance("snare", "snere"), 1);
        assert_eq!(edit_distance("growl", "grwol"), 1);
        assert_eq!(edit_distance("screech", "screetch"), 1);
    }

    #[test]
    fn fuzzy() {
        let c = cfg();
        assert_eq!(fuzzy_alias(&c, "snere"), Some("snare"));
        assert_eq!(fuzzy_alias(&c, "shakker"), Some("shaker"));
        assert_eq!(fuzzy_alias(&c, "snare"), None, "exact words are not fuzzy");
        assert_eq!(fuzzy_alias(&c, "kik"), None, "too short");
    }
}
