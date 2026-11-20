//! Minimal RIFF/WAVE chunk walker for `acid` and `smpl` chunks.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Default)]
pub struct RiffInfo {
    pub acid_one_shot: Option<bool>,
    pub acid_tempo: Option<f32>,
    pub smpl_loops: Option<u32>,
}

pub fn read_chunks(path: &Path) -> std::io::Result<RiffInfo> {
    let f = File::open(path)?;
    let len = f.metadata()?.len();
    let mut r = BufReader::new(f);
    let mut hdr = [0u8; 12];
    r.read_exact(&mut hdr)?;
    if &hdr[0..4] != b"RIFF" || &hdr[8..12] != b"WAVE" {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not RIFF/WAVE"));
    }
    let mut info = RiffInfo::default();
    let mut pos = 12u64;
    for _ in 0..256 {
        if pos + 8 > len {
            break;
        }
        let mut ch = [0u8; 8];
        if r.read_exact(&mut ch).is_err() {
            break;
        }
        let size = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]) as u64;
        let id = &ch[0..4];
        let body_start = pos + 8;
        match id {
            b"acid" if size >= 24 => {
                let mut b = [0u8; 24];
                r.read_exact(&mut b)?;
                let flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                let tempo = f32::from_le_bytes([b[20], b[21], b[22], b[23]]);
                info.acid_one_shot = Some(flags & 0x01 != 0);
                if tempo.is_finite() && (20.0..=400.0).contains(&tempo) {
                    info.acid_tempo = Some(tempo);
                }
            }
            b"smpl" if size >= 36 => {
                let mut b = [0u8; 36];
                r.read_exact(&mut b)?;
                info.smpl_loops = Some(u32::from_le_bytes([b[28], b[29], b[30], b[31]]));
            }
            _ => {}
        }
        pos = body_start + size + (size & 1);
        r.seek(SeekFrom::Start(pos))?;
    }
    Ok(info)
}
