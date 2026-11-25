//! Audio metadata extraction and preview decoding.
//!
//! Metadata is read from container headers only (no full decode), so
//! probing thousands of files stays cheap. For WAV files we additionally
//! walk the RIFF chunk list (seeking past audio data) to read the `acid`
//! chunk (loop/one-shot flag and tempo, written by many sample vendors) and
//! the `smpl` chunk (sampler loop points).

pub mod analysis;
mod decode;
mod riff;

pub use analysis::{analyse_file, AudioFeatures};
pub use decode::{decode_to_wav, peaks};

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioMeta {
    pub duration_secs: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub bit_depth: Option<u16>,
    pub codec: Option<String>,
    /// From the WAV `acid` chunk, when present.
    pub acid_one_shot: Option<bool>,
    pub acid_tempo: Option<f32>,
    /// Number of sampler loops in the WAV `smpl` chunk.
    pub smpl_loop_count: Option<u32>,
    /// MIDI unity note from the WAV `smpl` chunk (60 = C4, also the common
    /// default, so it is only shown when it differs from 60).
    pub root_note: Option<u32>,
    /// Embedded tags (WAV INFO, ID3, Vorbis comments).
    pub title: Option<String>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    pub tag_bpm: Option<f32>,
    pub tag_key: Option<String>,
    /// Signal analysis, present when audio analysis is enabled.
    pub features: Option<AudioFeatures>,
    /// Set when the header could not be read; the file is still organised.
    pub error: Option<String>,
}

/// Read header-level metadata. Never fails: problems are recorded in
/// `error` so a damaged file can still be classified by name.
pub fn probe(path: &Path) -> AudioMeta {
    let mut meta = AudioMeta::default();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "wav" {
        if let Ok(r) = riff::read_chunks(path) {
            meta.acid_one_shot = r.acid_one_shot;
            meta.acid_tempo = r.acid_tempo;
            meta.smpl_loop_count = r.smpl_loops;
            meta.root_note = r.root_note;
        }
    }
    match probe_symphonia(path, &ext, &mut meta) {
        Ok(()) => {}
        Err(e) => meta.error = Some(e),
    }
    meta
}

fn probe_symphonia(path: &Path, ext: &str, meta: &mut AudioMeta) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .map_err(|e| format!("unrecognised audio header: {e}"))?;
    let mut probed = probed;
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current().cloned()) {
        read_tags(rev.tags(), meta);
    }
    if let Some(rev) = probed.format.metadata().current() {
        read_tags(rev.tags(), meta);
    }
    let track = probed.format.default_track().ok_or("no audio track")?;
    let p = &track.codec_params;
    meta.sample_rate = p.sample_rate;
    meta.channels = p.channels.map(|c| c.count() as u16);
    meta.bit_depth = p.bits_per_sample.map(|b| b as u16);
    meta.codec = symphonia::default::get_codecs().get_codec(p.codec).map(|d| d.short_name.to_string());
    if let (Some(n), Some(sr)) = (p.n_frames, p.sample_rate) {
        if sr > 0 {
            meta.duration_secs = Some(n as f64 / sr as f64);
        }
    } else if let (Some(n), Some(tb)) = (p.n_frames, p.time_base) {
        let t = tb.calc_time(n);
        meta.duration_secs = Some(t.seconds as f64 + t.frac);
    }
    Ok(())
}

fn read_tags(tags: &[symphonia::core::meta::Tag], meta: &mut AudioMeta) {
    use symphonia::core::meta::StandardTagKey as K;
    let clean = |v: &str| {
        let t = v.trim().trim_matches(char::from(0)).trim().to_string();
        (!t.is_empty() && t.len() < 500).then_some(t)
    };
    for tag in tags {
        let value = tag.value.to_string();
        let Some(v) = clean(&value) else { continue };
        let raw_key = tag.key.to_ascii_uppercase();
        match tag.std_key {
            Some(K::TrackTitle) => meta.title = meta.title.take().or(Some(v)),
            Some(K::Genre) => meta.genre = meta.genre.take().or(Some(v)),
            Some(K::Comment) | Some(K::Description) | Some(K::PodcastKeywords) => {
                meta.comment = Some(match meta.comment.take() {
                    Some(c) if !c.contains(&v) => format!("{c} {v}"),
                    Some(c) => c,
                    None => v,
                })
            }
            Some(K::Bpm) => {
                if let Ok(b) = v.split_whitespace().next().unwrap_or("").parse::<f32>() {
                    if (40.0..=300.0).contains(&b) {
                        meta.tag_bpm = Some(b);
                    }
                }
            }
            _ if matches!(raw_key.as_str(), "TKEY" | "INITIALKEY" | "KEY" | "INITIAL KEY") => {
                meta.tag_key = crate::classifier::musical::normalise_key(&v);
            }
            _ if raw_key == "IKEY" => {
                // RIFF INFO "keywords"
                meta.comment = Some(match meta.comment.take() {
                    Some(c) => format!("{c} {v}"),
                    None => v,
                })
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 16-bit mono WAV with an optional ACID chunk.
    pub fn make_wav(secs: f32, acid: Option<(bool, f32)>) -> Vec<u8> {
        let sr = 22050u32;
        let n = (sr as f32 * secs) as usize;
        let data: Vec<u8> = (0..n)
            .flat_map(|i| (((i as f32 * 0.05).sin() * 8000.0) as i16).to_le_bytes())
            .collect();
        let mut chunks = Vec::new();
        chunks.extend_from_slice(b"fmt ");
        chunks.extend_from_slice(&16u32.to_le_bytes());
        chunks.extend_from_slice(&1u16.to_le_bytes());
        chunks.extend_from_slice(&1u16.to_le_bytes());
        chunks.extend_from_slice(&sr.to_le_bytes());
        chunks.extend_from_slice(&(sr * 2).to_le_bytes());
        chunks.extend_from_slice(&2u16.to_le_bytes());
        chunks.extend_from_slice(&16u16.to_le_bytes());
        if let Some((one_shot, tempo)) = acid {
            chunks.extend_from_slice(b"acid");
            chunks.extend_from_slice(&24u32.to_le_bytes());
            let mut b = [0u8; 24];
            b[0..4].copy_from_slice(&(one_shot as u32).to_le_bytes());
            b[20..24].copy_from_slice(&tempo.to_le_bytes());
            chunks.extend_from_slice(&b);
        }
        chunks.extend_from_slice(b"data");
        chunks.extend_from_slice(&(data.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&data);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(4 + chunks.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(&chunks);
        out
    }

    #[test]
    fn probes_header_and_acid() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("loop.wav");
        std::fs::write(&p, make_wav(2.0, Some((false, 140.0)))).unwrap();
        let m = probe(&p);
        assert_eq!(m.error, None);
        assert_eq!(m.sample_rate, Some(22050));
        assert_eq!(m.channels, Some(1));
        assert_eq!(m.bit_depth, Some(16));
        assert!((m.duration_secs.unwrap() - 2.0).abs() < 0.01);
        assert_eq!(m.acid_one_shot, Some(false));
        assert_eq!(m.acid_tempo, Some(140.0));
    }

    #[test]
    fn broken_file_reports_error_instead_of_failing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("bad.wav");
        std::fs::write(&p, b"not audio at all").unwrap();
        let m = probe(&p);
        assert!(m.error.is_some());
        assert_eq!(m.duration_secs, None);
    }

    #[test]
    fn decodes_to_wav_and_peaks() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.wav");
        std::fs::write(&p, make_wav(0.5, None)).unwrap();
        let wav = decode_to_wav(&p, 60.0).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 11025 * 2);
        let pk = peaks(&p, 100).unwrap();
        assert!(pk.len() >= 99 && pk.len() <= 100);
        assert!(pk.iter().any(|&v| v > 0.2));
    }
}
