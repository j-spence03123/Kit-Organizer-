//! Token-aware text normalisation.
//!
//! Sample names use every convention imaginable: `XLNT_Kick_01`,
//! `ClosedHat-03`, `HH OPEN 2`, `kick01`, `808s`. We normalise them in two
//! steps:
//!
//! 1. **Coarse tokens** split on separators only (space, `_`, `-`, `.`,
//!    brackets…). These keep `F#m` intact for key detection.
//! 2. **Fine tokens** additionally split camelCase (`ClosedHat` → `closed
//!    hat`) and letter/digit boundaries (`Kick01` → `kick 01`, `128bpm` →
//!    `128 bpm`), then lowercase.
//!
//! Matching always happens on whole tokens, so the abbreviation `bd` never
//! matches inside `abdominal`.

/// Split on anything that is not a letter, digit or `#` (kept for keys).
pub fn coarse_tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '#' || c == '♯' || c == '♭'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Upper,
    Lower,
    Digit,
}

fn class(c: char) -> Class {
    if c.is_ascii_digit() || c.is_numeric() {
        Class::Digit
    } else if c.is_uppercase() {
        Class::Upper
    } else {
        Class::Lower
    }
}

/// Split one coarse token at case and digit boundaries.
fn split_word(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().filter(|c| c.is_alphanumeric()).collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if !cur.is_empty() {
            let prev = class(chars[i - 1]);
            let now = class(c);
            let next = chars.get(i + 1).map(|&n| class(n));
            let boundary = match (prev, now) {
                (Class::Digit, Class::Digit) => false,
                (Class::Digit, _) | (_, Class::Digit) => true,
                (Class::Lower, Class::Upper) => true,
                // "HHOpen" -> "HH" + "Open"
                (Class::Upper, Class::Upper) => next == Some(Class::Lower) && cur.chars().count() > 1,
                _ => false,
            };
            if boundary {
                out.push(std::mem::take(&mut cur));
            }
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // Re-attach a plural "s" to numbers: "808s" stays "808s".
    let mut merged: Vec<String> = Vec::with_capacity(out.len());
    for t in out {
        if t.eq_ignore_ascii_case("s") {
            if let Some(last) = merged.last_mut() {
                if last.chars().all(|c| c.is_ascii_digit()) {
                    last.push('s');
                    continue;
                }
            }
        }
        merged.push(t);
    }
    merged
}

/// Lowercased fine tokens.
pub fn fine_tokens(s: &str) -> Vec<String> {
    coarse_tokens(s).iter().flat_map(|w| split_word(w)).map(|t| t.to_lowercase()).collect()
}

/// Strip the extension from a filename.
pub fn stem(file_name: &str) -> &str {
    match file_name.rfind('.') {
        Some(i) if i > 0 => &file_name[..i],
        _ => file_name,
    }
}

/// Simple singular form used as a fallback when an exact lookup fails.
pub fn singular(tok: &str) -> Option<String> {
    if tok.len() > 3 && tok.ends_with("es") && (tok.ends_with("shes") || tok.ends_with("ches") || tok.ends_with("xes")) {
        return Some(tok[..tok.len() - 2].to_string());
    }
    if tok.len() > 2 && tok.ends_with('s') && !tok.ends_with("ss") {
        return Some(tok[..tok.len() - 1].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_conventions() {
        assert_eq!(fine_tokens("XLNT_Kick_01"), ["xlnt", "kick", "01"]);
        assert_eq!(fine_tokens("ClosedHat-03"), ["closed", "hat", "03"]);
        assert_eq!(fine_tokens("HHOpen 2"), ["hh", "open", "2"]);
        assert_eq!(fine_tokens("kick01"), ["kick", "01"]);
        assert_eq!(fine_tokens("128bpm"), ["128", "bpm"]);
        assert_eq!(fine_tokens("808s & Subs"), ["808s", "subs"]);
        assert_eq!(fine_tokens("Hi-Hat (Open) [Wet]"), ["hi", "hat", "open", "wet"]);
        assert_eq!(fine_tokens("BOS_TT_Bass_Loop_140_Fmin"), ["bos", "tt", "bass", "loop", "140", "fmin"]);
    }

    #[test]
    fn coarse_keeps_sharp() {
        assert_eq!(coarse_tokens("Growl_F#m_01.wav"), ["Growl", "F#m", "01", "wav"]);
    }

    #[test]
    fn singulars() {
        assert_eq!(singular("kicks").as_deref(), Some("kick"));
        assert_eq!(singular("crashes").as_deref(), Some("crash"));
        assert_eq!(singular("bass"), None);
    }
}
