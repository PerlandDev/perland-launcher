use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha512};

/// The checksums a source publishes for one file. Any subset may be present.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hashes {
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
    pub murmur2: Option<u32>,
}

impl Hashes {
    pub fn sha1(hex: impl Into<String>) -> Self {
        Self {
            sha1: Some(hex.into()),
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sha1.is_none()
            && self.sha256.is_none()
            && self.sha512.is_none()
            && self.murmur2.is_none()
    }

    /// True when every hash that is present matches `data`.
    pub fn verify(&self, data: &[u8]) -> bool {
        type Check<'a> = (&'a Option<String>, fn(&[u8]) -> String);
        let checks: [Check; 3] = [
            (&self.sha1, sha1_hex),
            (&self.sha256, sha256_hex),
            (&self.sha512, sha512_hex),
        ];
        for (expected, f) in checks {
            if let Some(expected) = expected {
                if !expected.eq_ignore_ascii_case(&f(data)) {
                    return false;
                }
            }
        }
        match self.murmur2 {
            Some(m) => m == curseforge_murmur2(data),
            None => true,
        }
    }
}

pub fn sha1_hex(data: &[u8]) -> String {
    hex::encode(Sha1::digest(data))
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn sha512_hex(data: &[u8]) -> String {
    hex::encode(Sha512::digest(data))
}

/// CurseForge's file fingerprint: MurmurHash2 (seed 1) over the bytes with tab/LF/CR/space removed.
pub fn curseforge_murmur2(data: &[u8]) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let bytes: Vec<u8> = data
        .iter()
        .copied()
        .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
        .collect();
    let mut h: u32 = 1 ^ (bytes.len() as u32);
    let (chunks, rem) = bytes.as_chunks::<4>();
    for c in chunks {
        let mut k = u32::from_le_bytes(*c);
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    if !rem.is_empty() {
        if rem.len() == 3 {
            h ^= (rem[2] as u32) << 16;
        }
        if rem.len() >= 2 {
            h ^= (rem[1] as u32) << 8;
        }
        h ^= rem[0] as u32;
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_vectors() {
        assert_eq!(
            sha1_hex(b"hello"),
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
        );
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(
            sha512_hex(b"hello"),
            "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043"
        );
    }

    #[test]
    fn murmur2_matches_curseforge_fingerprints() {
        assert_eq!(curseforge_murmur2(b"hello world"), 2824650221);
        // CurseForge strips tab, LF, CR and space before hashing.
        assert_eq!(curseforge_murmur2(b"helloworld"), 2824650221);
        assert_eq!(
            curseforge_murmur2(b"a\tb\r\nc d"),
            curseforge_murmur2(b"abcd")
        );
        assert_eq!(curseforge_murmur2(b"a\tb\r\nc d"), 3376380438);
        assert_eq!(curseforge_murmur2(b""), 1540447798);
    }

    #[test]
    fn verify_requires_every_present_hash() {
        let mut h = Hashes::sha1("AAF4C61DDCC5E8A2DABEDE0F3B482CD9AEA9434D");
        assert!(h.verify(b"hello"), "comparison is case-insensitive");
        assert!(!h.verify(b"jello"));
        h.sha256 = Some("00".into());
        assert!(
            !h.verify(b"hello"),
            "a wrong sha256 fails even when sha1 matches"
        );
        assert!(Hashes::default().is_empty());
        assert!(Hashes::default().verify(b"anything"));
        let cf = Hashes {
            murmur2: Some(2824650221),
            ..Default::default()
        };
        assert!(cf.verify(b"hello world"));
    }
}
