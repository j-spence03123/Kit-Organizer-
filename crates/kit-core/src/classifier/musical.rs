//! BPM and key extraction from filenames.
//!
//! We only report values that are explicitly stated: a BPM requires the
//! `bpm` marker next to a number (`128bpm`, `128 BPM`, `BPM_128`), and a key
//! requires a recognisable note/quality token. Bare numbers such as
//! `Kick_128` are never treated as tempo because they are usually indices.

use super::tokenize::{coarse_tokens, fine_tokens};

/// BPM stated in the filename, if any.
pub fn bpm_from_name(stem: &str) -> Option<f32> {
    let toks = fine_tokens(stem);
    for (i, t) in toks.iter().enumerate() {
        if t == "bpm" {
            let before = i.checked_sub(1).and_then(|j| toks.get(j));
            let after = toks.get(i + 1);
            for cand in [before, after].into_iter().flatten() {
                if let Ok(n) = cand.parse::<f32>() {
                    if (40.0..=300.0).contains(&n) {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
}

fn parse_note(s: &str) -> Option<(String, &str)> {
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    let letter = first.to_ascii_uppercase();
    if !('A'..='G').contains(&letter) {
        return None;
    }
    let mut note = letter.to_string();
    let mut rest = &s[first.len_utf8()..];
    if let Some(r) = rest.strip_prefix('#').or_else(|| rest.strip_prefix('♯')) {
        note.push('#');
        rest = r;
    } else if let Some(r) = rest.strip_prefix('♭') {
        note.push('b');
        rest = r;
    } else if rest.starts_with('b') && !rest.starts_with("bpm") {
        // "Bb", "Ebmin" — but not "Bass" (handled by quality check below).
        let after = &rest[1..];
        if after.is_empty() || quality(after).is_some() || after.chars().all(|c| c.is_ascii_digit()) {
            note.push('b');
            rest = after;
        }
    }
    Some((note, rest))
}

fn quality(s: &str) -> Option<&'static str> {
    match s {
        "m" | "min" | "minor" | "Min" | "Minor" | "MIN" | "mi" => Some("m"),
        "maj" | "major" | "Maj" | "Major" | "MAJ" | "M" | "ma" => Some(""),
        _ => None,
    }
}

/// Musical key stated in the filename, normalised like `F#m`, `C`, `Ebm`.
pub fn key_from_name(stem: &str) -> Option<String> {
    let toks: Vec<String> = coarse_tokens(stem);
    // Explicit "key" marker: "Key_Am", "key of F".
    for (i, t) in toks.iter().enumerate() {
        if t.eq_ignore_ascii_case("key") {
            let mut j = i + 1;
            if toks.get(j).is_some_and(|x| x.eq_ignore_ascii_case("of")) {
                j += 1;
            }
            if let Some(k) = toks.get(j).and_then(|x| parse_key_token(x, true)) {
                return Some(k);
            }
        }
    }
    let n = toks.len();
    for (i, t) in toks.iter().enumerate() {
        // Note followed by a separate quality word: "F minor", "C_Maj".
        if let Some(next) = toks.get(i + 1) {
            if t.len() <= 2 {
                if let (Some((note, "")), Some(q)) = (parse_note(t), quality(next)) {
                    if t.chars().next().is_some_and(|c| c.is_uppercase()) {
                        return Some(format!("{note}{q}"));
                    }
                }
            }
        }
        if let Some(k) = parse_key_token(t, false) {
            return Some(k);
        }
        // Bare uppercase note letter ("XLNT_Growl_F"): only near the end of
        // the name and never as the first token.
        let tail_pos = toks[i + 1..].iter().all(|x| x.chars().all(|c| c.is_ascii_digit()));
        let near_end = i + 2 >= n || tail_pos;
        if i > 0 && near_end {
            let chars: Vec<char> = t.chars().collect();
            let bare = match chars.as_slice() {
                [c] => c.is_ascii_uppercase(),
                [c, '#'] | [c, '♯'] => c.is_ascii_uppercase(),
                [c, 'b'] => c.is_ascii_uppercase() && *c != 'B',
                _ => false,
            };
            if bare {
                if let Some((note, "")) = parse_note(t) {
                    return Some(note);
                }
            }
        }
    }
    None
}

/// A single token like `F#m`, `Ebmin`, `Cmaj`, or a note with octave `C4`.
fn parse_key_token(t: &str, after_marker: bool) -> Option<String> {
    let (note, rest) = parse_note(t)?;
    if !t.chars().next()?.is_uppercase() && !after_marker {
        return None;
    }
    if rest.is_empty() {
        return after_marker.then_some(note);
    }
    if let Some(q) = quality(rest) {
        return Some(format!("{note}{q}"));
    }
    // Note with octave, e.g. "C4", "F#3" — a pitched one-shot.
    if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_digit()) {
        return Some(note);
    }
    None
}

/// True when the name contains a note+octave token like `C4` — typical of
/// multisampled one-shots.
pub fn has_note_octave(stem: &str) -> bool {
    coarse_tokens(stem).iter().any(|t| {
        let chars: Vec<char> = t.chars().collect();
        matches!(chars.as_slice(),
            [n, o] if ('A'..='G').contains(n) && o.is_ascii_digit())
            || matches!(chars.as_slice(),
            [n, '#' | 'b', o] if ('A'..='G').contains(n) && o.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpm() {
        assert_eq!(bpm_from_name("House_Kick_Loop_128BPM"), Some(128.0));
        assert_eq!(bpm_from_name("Loop 140 bpm"), Some(140.0));
        assert_eq!(bpm_from_name("BPM_174_Break"), Some(174.0));
        assert_eq!(bpm_from_name("Kick_128"), None);
        assert_eq!(bpm_from_name("BOS_TT_Bass_Loop_140_Fmin"), None);
    }

    #[test]
    fn keys() {
        assert_eq!(key_from_name("XLNT_Growl_F").as_deref(), Some("F"));
        assert_eq!(key_from_name("Bass_Loop_140_Fmin").as_deref(), Some("Fm"));
        assert_eq!(key_from_name("Pad_F#m_01").as_deref(), Some("F#m"));
        assert_eq!(key_from_name("Lead_Ebmaj").as_deref(), Some("Eb"));
        assert_eq!(key_from_name("Piano_C4").as_deref(), Some("C"));
        assert_eq!(key_from_name("Chords A minor 120bpm").as_deref(), Some("Am"));
        assert_eq!(key_from_name("Key_Am_Melody").as_deref(), Some("Am"));
        assert_eq!(key_from_name("Bass_Growl_01"), None);
        assert_eq!(key_from_name("Kick_01"), None);
        assert_eq!(key_from_name("Big Room Drop"), None);
        assert_eq!(key_from_name("A Kick 01"), None);
    }

    #[test]
    fn note_octave() {
        assert!(has_note_octave("Piano_C4"));
        assert!(has_note_octave("Pluck F#3"));
        assert!(!has_note_octave("Kick_01"));
    }
}
