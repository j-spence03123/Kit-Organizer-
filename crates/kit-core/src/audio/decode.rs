//! Full decoding for previews. Every supported format is transcoded to a
//! 16-bit PCM WAV in memory so the webview can play it regardless of its
//! own codec support (WebKit has no OGG, Chromium has no AIFF).

use crate::error::{CoreError, CoreResult};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decoded interleaved f32 samples.
pub struct Decoded {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

pub fn decode(path: &Path, max_secs: f64) -> CoreResult<Decoded> {
    let file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(e) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(e);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| CoreError::Audio(format!("cannot read {}: {e}", path.display())))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or_else(|| CoreError::Audio("no audio track".into()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| CoreError::Audio(format!("unsupported codec: {e}")))?;
    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let mut channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
    let mut samples: Vec<f32> = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymErr::IoError(_)) | Err(SymErr::ResetRequired) => break,
            Err(e) => return Err(CoreError::Audio(e.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio) => {
                let spec = *audio.spec();
                sample_rate = spec.rate;
                channels = spec.channels.count() as u16;
                let need = audio.capacity() as u64;
                if buf.as_ref().is_none_or(|b| b.capacity() < need as usize * spec.channels.count()) {
                    buf = Some(SampleBuffer::<f32>::new(need, spec));
                }
                let b = buf.as_mut().expect("buffer allocated");
                b.copy_interleaved_ref(audio);
                samples.extend_from_slice(b.samples());
                if samples.len() as f64 >= max_secs * sample_rate as f64 * channels as f64 {
                    break;
                }
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(CoreError::Audio(e.to_string())),
        }
    }
    Ok(Decoded { sample_rate, channels: channels.max(1), samples })
}

/// Decode `path` and return a complete 16-bit PCM WAV file.
pub fn decode_to_wav(path: &Path, max_secs: f64) -> CoreResult<Vec<u8>> {
    let d = decode(path, max_secs)?;
    let data_len = (d.samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    let ch = d.channels;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&ch.to_le_bytes());
    out.extend_from_slice(&d.sample_rate.to_le_bytes());
    out.extend_from_slice(&(d.sample_rate * ch as u32 * 2).to_le_bytes());
    out.extend_from_slice(&(ch * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in d.samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    Ok(out)
}

/// Peak envelope with `buckets` points (max |sample| per bucket, 0..1).
pub fn peaks(path: &Path, buckets: usize) -> CoreResult<Vec<f32>> {
    let d = decode(path, 600.0)?;
    let frames = d.samples.len() / d.channels as usize;
    if frames == 0 || buckets == 0 {
        return Ok(vec![]);
    }
    let per = frames.div_ceil(buckets).max(1);
    let ch = d.channels as usize;
    Ok(d.samples
        .chunks(per * ch)
        .map(|c| c.iter().fold(0.0f32, |m, s| m.max(s.abs())))
        .collect())
}
