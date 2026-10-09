use std::path::{Path, PathBuf};

use crate::error::Result;

/// Write a file so readers never see a half-written version: write `<path>.tmp`, then rename over `path`.
pub async fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, data).await?;
    tokio::fs::rename(&tmp, path).await?;
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
