use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::error::Result;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write a file so readers never see a half-written version: write a temp file next to it, then
/// rename over `path`. The temp name (`<path>.<pid>.<n>.tmp`) is unique per call, so concurrent
/// writers to the same path never share one; the last rename wins.
pub async fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.{n}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let written = match tokio::fs::write(&tmp, data).await {
        Ok(()) => tokio::fs::rename(&tmp, path).await,
        Err(e) => Err(e),
    };
    if let Err(e) = written {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e.into());
    }
    Ok(())
}

/// Join a remote-supplied relative path onto `base` without letting it escape.
///
/// Returns `None` for empty paths, NUL bytes, absolute or rooted paths (`/x`, `\x`), Windows drive
/// or prefix paths (`C:x`) and any `..` component, splitting on both `/` and `\` so the result
/// is the same on every platform.
pub fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\0') || rel.starts_with(['/', '\\']) {
        return None;
    }
    let bytes = rel.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return None;
    }
    if rel.split(['/', '\\']).any(|part| part == "..") {
        return None;
    }
    let path = Path::new(rel);
    if path.is_absolute()
        || path.has_root()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::Prefix(_)))
    {
        return None;
    }
    Some(base.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_atomic_writes_to_one_path_all_succeed() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("shared.json");
        let contents: Vec<String> = (0..16).map(|i| format!("writer {i}")).collect();
        let results = futures_util::future::join_all(
            contents.iter().map(|c| write_atomic(&path, c.as_bytes())),
        )
        .await;
        for result in results {
            result.unwrap();
        }
        let last = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains(&last), "{last:?}");
        let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn safe_join_accepts_relative_paths() {
        let base = Path::new("/base");
        assert_eq!(
            safe_join(base, "bin/javaw.exe"),
            Some(base.join("bin/javaw.exe"))
        );
        assert_eq!(safe_join(base, "a/./b"), Some(base.join("a/./b")));
    }

    #[test]
    fn safe_join_rejects_escaping_paths() {
        let base = Path::new("/base");
        for bad in [
            "",
            "../x",
            "a/../../x",
            "a\\..\\x",
            "/etc/passwd",
            "\\x",
            "C:/x",
            "C:x",
            "a\0b",
        ] {
            assert_eq!(safe_join(base, bad), None, "{bad:?} must be rejected");
        }
    }
}
