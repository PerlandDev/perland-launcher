use std::{io::Read, path::Path};

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

    /// Same result as [`Hashes::verify`] on the file's bytes, but streams the file in 64 KiB chunks so large
    /// files are never held in memory. CurseForge's murmur2 needs the whitespace-filtered length up front,
    /// so when it is present the whole file is read instead (CurseForge files only).
    pub fn verify_file(&self, path: &Path) -> std::io::Result<bool> {
        if self.is_empty() {
            return Ok(true);
        }
        if self.murmur2.is_some() {
            return Ok(self.verify(&std::fs::read(path)?));
        }
        let mut file = std::fs::File::open(path)?;
        let mut sha1 = self.sha1.as_ref().map(|_| Sha1::new());
        let mut sha256 = self.sha256.as_ref().map(|_| Sha256::new());
        let mut sha512 = self.sha512.as_ref().map(|_| Sha512::new());
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let chunk = &buf[..n];
            if let Some(h) = sha1.as_mut() {
                h.update(chunk);
            }
            if let Some(h) = sha256.as_mut() {
                h.update(chunk);
            }
            if let Some(h) = sha512.as_mut() {
                h.update(chunk);
            }
        }
        fn matches(expected: &Option<String>, hasher: Option<impl Digest>) -> bool {
            match (expected, hasher) {
                (Some(expected), Some(h)) => {
                    expected.eq_ignore_ascii_case(&hex::encode(h.finalize()))
                }
                _ => true,
            }
        }
        Ok(matches(&self.sha1, sha1)
            && matches(&self.sha256, sha256)
            && matches(&self.sha512, sha512))
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

    fn assert_agrees(h: &Hashes, data: &[u8], expected: bool) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, data).unwrap();
        assert_eq!(h.verify(data), expected);
        assert_eq!(h.verify_file(&path).unwrap(), expected);
    }

    #[test]
    fn verify_file_agrees_with_verify() {
        let sha1 = Hashes::sha1("AAF4C61DDCC5E8A2DABEDE0F3B482CD9AEA9434D");
        assert_agrees(&sha1, b"hello", true);
        assert_agrees(&sha1, b"jello", false);

        let all_sha = Hashes {
            sha1: sha1.sha1.clone(),
            sha256: Some(sha256_hex(b"hello")),
            sha512: Some(sha512_hex(b"hello")),
            murmur2: None,
        };
        assert_agrees(&all_sha, b"hello", true);
        assert_agrees(&all_sha, b"jello", false);
        let wrong_sha256 = Hashes {
            sha256: Some("00".into()),
            ..sha1.clone()
        };
        assert_agrees(&wrong_sha256, b"hello", false);

        let cf = Hashes {
            murmur2: Some(2824650221),
            ..Default::default()
        };
        assert_agrees(&cf, b"hello world", true);
        assert_agrees(&cf, b"hello", false);

        assert_agrees(&Hashes::default(), b"anything", true);
    }

    #[test]
    fn verify_file_streams_data_larger_than_one_chunk() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let h = Hashes {
            sha256: Some(sha256_hex(&data)),
            ..Default::default()
        };
        assert_agrees(&h, &data, true);
        let mut tampered = data.clone();
        tampered[150_000] ^= 1;
        assert_agrees(&h, &tampered, false);
    }

    #[test]
    fn verify_file_reports_missing_file_as_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Hashes::sha1("00")
            .verify_file(&dir.path().join("nope"))
            .is_err());
    }
}
