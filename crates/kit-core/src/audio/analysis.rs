//! Lightweight signal analysis used as *supporting* classification
//! evidence. This is deliberately simple, explainable DSP — band energy,
//! envelope shape, onset rhythm — not sound recognition. It cannot tell a
//! snare from a clap; it can tell a short bass-heavy hit from a bright
//! noisy tick, a steadily rising sweep from a decaying hit, and a rhythmic
//! pattern from a single event.

use super::decode::decode;
use crate::error::CoreResult;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Seconds of audio analysed per file (keeps scans fast).
pub const ANALYSIS_SECS: f64 = 10.0;
const FRAME_SECS: f32 = 0.01;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioFeatures {
    /// Seconds actually analysed.
    pub analysed_secs: f32,
    /// Share of energy below ~150 Hz (0..1).
    pub low_ratio: f32,
    /// Share of energy above ~5 kHz (0..1).
    pub high_ratio: f32,
    /// Zero-crossing rate as a fraction of the sample rate (0..0.5).
    pub zcr: f32,
    /// Time from start to the loudest 10 ms frame.
    pub attack_ms: f32,
    /// Time from the peak until the level falls 30 dB below it, if it does.
    pub decay_ms: Option<f32>,
    /// Number of distinct onsets (hits).
    pub onsets: u32,
    /// 0..1 strength of a regular repeating pulse in the onset envelope.
    pub periodicity: f32,
    /// Level of the last quarter minus the first quarter, in dB.
    pub rise_db: f32,
    /// The analysed audio ends in near-silence.
    pub tail_silent: bool,
}

/// Decode up to [`ANALYSIS_SECS`] of `path` and measure it.
pub fn analyse_file(path: &Path) -> CoreResult<AudioFeatures> {
    let d = decode(path, ANALYSIS_SECS)?;
    let ch = d.channels as usize;
    let mono: Vec<f32> = d.samples.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    Ok(analyse(&mono, d.sample_rate))
}

fn one_pole(fc: f32, sr: f32) -> f32 {
    1.0 - (-2.0 * std::f32::consts::PI * fc / sr).exp()
}

pub fn analyse(x: &[f32], sample_rate: u32) -> AudioFeatures {
    let sr = sample_rate.max(1) as f32;
    let mut f = AudioFeatures { analysed_secs: x.len() as f32 / sr, ..Default::default() };
    if x.len() < (sr * 0.02) as usize {
        return f;
    }
    // Band energies with simple one-pole filters (two cascaded for the low band).
    let (a_low, a_high) = (one_pole(150.0, sr), one_pole(5000.0, sr));
    let (mut l1, mut l2, mut h) = (0.0f32, 0.0f32, 0.0f32);
    let (mut e_total, mut e_low, mut e_high) = (0.0f64, 0.0f64, 0.0f64);
    let mut crossings = 0u64;
    for (i, &s) in x.iter().enumerate() {
        l1 += a_low * (s - l1);
        l2 += a_low * (l1 - l2);
        h += a_high * (s - h);
        let hp = s - h;
        e_total += (s * s) as f64;
        e_low += (l2 * l2) as f64;
        e_high += (hp * hp) as f64;
        if i > 0 && (x[i - 1] >= 0.0) != (s >= 0.0) {
            crossings += 1;
        }
    }
    if e_total <= 1e-12 {
        f.tail_silent = true;
        return f;
    }
    f.low_ratio = (e_low / e_total).min(1.0) as f32;
    f.high_ratio = (e_high / e_total).min(1.0) as f32;
    f.zcr = crossings as f32 / x.len() as f32;

    // 10 ms RMS envelope.
    let hop = ((sr * FRAME_SECS) as usize).max(1);
    let rms: Vec<f32> = x.chunks(hop).map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let (peak_i, peak) = rms.iter().copied().enumerate().fold((0, 0.0f32), |a, (i, v)| if v > a.1 { (i, v) } else { a });
    f.attack_ms = peak_i as f32 * FRAME_SECS * 1000.0;
    let floor = peak * 0.0316; // -30 dB
    f.decay_ms = rms[peak_i..].iter().position(|&v| v < floor).map(|p| p as f32 * FRAME_SECS * 1000.0);
    let tail_n = (rms.len() / 10).max(1);
    let tail = rms[rms.len() - tail_n..].iter().sum::<f32>() / tail_n as f32;
    f.tail_silent = tail < peak * 0.0056; // -45 dB

    let db = |v: f32| 20.0 * (v.max(1e-6)).log10();
    if rms.len() >= 8 {
        let q = rms.len() / 4;
        let mean = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
        f.rise_db = db(mean(&rms[rms.len() - q..])) - db(mean(&rms[..q]));
    }

    // Onsets: peaks in the positive level difference (spectral-flux proxy).
    let flux: Vec<f32> = std::iter::once(rms[0]).chain(rms.windows(2).map(|w| (w[1] - w[0]).max(0.0))).collect();
    let max_flux = flux.iter().copied().fold(0.0f32, f32::max);
    if max_flux > 0.0 {
        let thresh = max_flux * 0.3;
        let mut last: Option<usize> = None;
        for i in 0..flux.len() {
            let local_max = (i == 0 || flux[i] >= flux[i - 1]) && (i + 1 >= flux.len() || flux[i] >= flux[i + 1]);
            if flux[i] > thresh && local_max && last.is_none_or(|l| i - l >= 6) && rms[i.min(rms.len() - 1)] > peak * 0.05 {
                f.onsets += 1;
                last = Some(i);
            }
        }
        // Periodicity: normalised autocorrelation of the flux at beat-ish lags.
        if f.analysed_secs >= 2.0 {
            let mean = flux.iter().sum::<f32>() / flux.len() as f32;
            let c: Vec<f32> = flux.iter().map(|v| v - mean).collect();
            let r0: f32 = c.iter().map(|v| v * v).sum();
            if r0 > 0.0 {
                let max_lag = (150).min(c.len() / 2);
                f.periodicity = (10..max_lag)
                    .map(|lag| c[..c.len() - lag].iter().zip(&c[lag..]).map(|(a, b)| a * b).sum::<f32>() / r0)
                    .fold(0.0f32, f32::max)
                    .clamp(0.0, 1.0);
            }
        }
    }
    f
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const SR: u32 = 22050;

    pub fn kick() -> Vec<f32> {
        (0..(SR as f32 * 0.5) as usize)
            .map(|i| {
                let t = i as f32 / SR as f32;
                (2.0 * std::f32::consts::PI * (55.0 + 120.0 * (-t * 40.0).exp()) * t).sin() * (-t * 9.0).exp()
            })
            .collect()
    }

    pub fn noise(n: usize, seed: u32) -> impl FnMut() -> f32 {
        let mut s = seed.wrapping_add(n as u32) | 1;
        move || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }

    pub fn hat() -> Vec<f32> {
        let mut r = noise(1, 7);
        let mut prev = 0.0f32;
        (0..(SR as f32 * 0.15) as usize)
            .map(|i| {
                let n = r();
                let hp = n - prev; // crude high-pass
                prev = n;
                hp * (-(i as f32) / SR as f32 * 40.0).exp()
            })
            .collect()
    }

    pub fn riser() -> Vec<f32> {
        let mut r = noise(2, 3);
        let n = (SR as f32 * 4.0) as usize;
        (0..n).map(|i| r() * (i as f32 / n as f32).powi(2)).collect()
    }

    pub fn loop_of_kicks() -> Vec<f32> {
        let k = kick();
        let beat = (SR as f32 * 0.5) as usize; // 120 BPM
        let mut out = vec![0.0; beat * 8];
        for b in 0..8 {
            for (i, v) in k.iter().enumerate() {
                if b * beat + i < out.len() {
                    out[b * beat + i] += v;
                }
            }
        }
        out
    }

    #[test]
    fn kick_is_low_and_punchy() {
        let f = analyse(&kick(), SR);
        assert!(f.low_ratio > 0.5, "{f:?}");
        assert!(f.attack_ms < 30.0 && f.onsets <= 2 && f.decay_ms.is_some(), "{f:?}");
    }

    #[test]
    fn hat_is_bright() {
        let f = analyse(&hat(), SR);
        assert!(f.zcr > 0.25 && f.low_ratio < 0.05 && f.high_ratio > 0.08, "{f:?}");
    }

    #[test]
    fn riser_rises() {
        let f = analyse(&riser(), SR);
        assert!(f.rise_db > 12.0, "{f:?}");
    }

    #[test]
    fn loop_is_periodic() {
        let f = analyse(&loop_of_kicks(), SR);
        assert!(f.onsets >= 6 && f.periodicity > 0.45, "{f:?}");
        let single = analyse(&kick(), SR);
        assert_eq!(single.periodicity, 0.0);
    }

    #[test]
    fn silence_is_safe() {
        let f = analyse(&vec![0.0; 22050], SR);
        assert!(f.tail_silent);
        assert_eq!(f.onsets, 0);
    }
}
