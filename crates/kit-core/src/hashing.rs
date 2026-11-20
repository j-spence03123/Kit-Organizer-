//! Content hashing for duplicate detection and copy verification (BLAKE3).

use crate::error::{CoreResult, IoContext};
use std::fs::File;
use std::path::Path;

/// Hash a file by streaming it (constant memory).
pub fn hash_file(path: &Path) -> CoreResult<String> {
    let f = File::open(path).at(path)?;
    let mut h = blake3::Hasher::new();
    h.update_reader(f).at(path)?;
    Ok(h.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_same_hash() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.wav");
        let b = d.path().join("b.wav");
        let c = d.path().join("c.wav");
        std::fs::write(&a, b"same").unwrap();
        std::fs::write(&b, b"same").unwrap();
        std::fs::write(&c, b"diff").unwrap();
        assert_eq!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
        assert_ne!(hash_file(&a).unwrap(), hash_file(&c).unwrap());
    }
}
