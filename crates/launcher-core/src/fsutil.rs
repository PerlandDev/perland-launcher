use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
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

/// True when the OS refused because something holds the file or folder: a permission refusal,
/// or on Windows `ERROR_ACCESS_DENIED` (5) or `ERROR_SHARING_VIOLATION` (32). An antivirus or
/// indexer often holds a just-written folder for a moment.
pub fn is_in_use(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(5 | 32))
}

/// The waits between tries of [`rename_with_retry`]: about two seconds in all.
const RENAME_RETRY_DELAYS: [Duration; 6] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(800),
    Duration::from_millis(450),
];

/// Rename `from` to `to`, trying again for about two seconds while the OS says something holds
/// it ([`is_in_use`]). Any other error, or the last refusal, comes back as is.
pub async fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_while_in_use(&RENAME_RETRY_DELAYS, || tokio::fs::rename(from, to)).await
}

/// Run `op`, and again after each of `delays` while it fails with [`is_in_use`].
async fn retry_while_in_use<T, F, Fut>(delays: &[Duration], mut op: F) -> std::io::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::io::Result<T>>,
{
    let mut delays = delays.iter();
    loop {
        match op().await {
            Err(e) if is_in_use(&e) => match delays.next() {
                Some(delay) => tokio::time::sleep(*delay).await,
                None => return Err(e),
            },
            result => return result,
        }
    }
}

/// Windows device names. A file whose stem is one of these (`nul.txt`, `CON .jar`) opens the
/// device instead of a file, so such names are never accepted from remote data.
const DEVICE_NAMES: [&str; 32] = [
    "con", "prn", "aux", "nul", "conin$", "conout$", "com0", "com1", "com2", "com3", "com4",
    "com5", "com6", "com7", "com8", "com9", "com¹", "com²", "com³", "lpt0", "lpt1", "lpt2", "lpt3",
    "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9", "lpt¹", "lpt²", "lpt³",
];

/// Longest relative path accepted from remote data, in bytes.
const MAX_REL_LEN: usize = 1024;

/// One path segment that is safe on every platform: not empty, `.` or `..`; no `:` (drive
/// letters, NTFS alternate streams), no `<>"|?*` or control characters; no trailing dot or
/// space (Windows strips them, so two names would alias); and not a Windows device name.
pub fn safe_component(part: &str) -> bool {
    if part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' ']) {
        return false;
    }
    if part.chars().any(|c| {
        c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*' | '/' | '\\')
    }) {
        return false;
    }
    let stem = part
        .split('.')
        .next()
        .unwrap_or(part)
        .trim_end()
        .to_lowercase();
    !DEVICE_NAMES.contains(&stem.as_str())
}

/// A remote-supplied relative path in canonical form (segments joined with `/`), or `None`
/// when any segment is unsafe (see [`safe_component`]). Splits on both `/` and `\`, so the
/// answer is the same on every platform. Leading, trailing and doubled separators are refused.
pub fn normalize_rel(rel: &str) -> Option<String> {
    if rel.is_empty() || rel.len() > MAX_REL_LEN {
        return None;
    }
    let parts: Vec<&str> = rel.split(['/', '\\']).collect();
    if !parts.iter().all(|p| safe_component(p)) {
        return None;
    }
    Some(parts.join("/"))
}

/// The key two relative paths collide under on a case-insensitive file system.
pub fn path_key(rel: &str) -> String {
    rel.to_lowercase()
}

/// Join a remote-supplied relative path onto `base` without letting it escape or alias:
/// `base` joined with [`normalize_rel`]'s canonical form, or `None` when it is unsafe.
pub fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    normalize_rel(rel).map(|n| base.join(n))
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, io::ErrorKind, time::Duration};

    use super::*;

    #[test]
    fn in_use_errors_are_recognised() {
        assert!(is_in_use(&std::io::Error::from_raw_os_error(32)));
        assert!(is_in_use(&std::io::Error::from_raw_os_error(5)));
        assert!(is_in_use(&std::io::Error::from(
            ErrorKind::PermissionDenied
        )));
        assert!(!is_in_use(&std::io::Error::from(ErrorKind::NotFound)));
    }

    #[tokio::test]
    async fn retry_gives_an_in_use_folder_time_to_let_go() {
        let calls = Cell::new(0);
        let result = retry_while_in_use(&[Duration::ZERO; 5], || {
            calls.set(calls.get() + 1);
            let n = calls.get();
            async move {
                if n < 3 {
                    Err(std::io::Error::from_raw_os_error(32))
                } else {
                    Ok(n)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 3);
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test]
    async fn retry_gives_up_after_the_last_delay() {
        let calls = Cell::new(0);
        let err = retry_while_in_use(&[Duration::ZERO; 4], || {
            calls.set(calls.get() + 1);
            async { Err::<(), _>(std::io::Error::from(ErrorKind::PermissionDenied)) }
        })
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::PermissionDenied);
        assert_eq!(calls.get(), 5, "the first try plus one per delay");
    }

    #[tokio::test]
    async fn retry_stops_at_an_error_that_is_not_in_use() {
        let calls = Cell::new(0);
        let err = retry_while_in_use(&[Duration::ZERO; 4], || {
            calls.set(calls.get() + 1);
            async { Err::<(), _>(std::io::Error::from(ErrorKind::NotFound)) }
        })
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn rename_with_retry_moves_a_folder() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("a")).unwrap();
        std::fs::write(tmp.path().join("a/f"), "f").unwrap();
        rename_with_retry(&tmp.path().join("a"), &tmp.path().join("b"))
            .await
            .unwrap();
        assert_eq!(std::fs::read(tmp.path().join("b/f")).unwrap(), b"f");
        assert!(!tmp.path().join("a").exists());
    }

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

    #[test]
    fn safe_join_accepts_relative_paths() {
        let base = Path::new("/base");
        assert_eq!(
            safe_join(base, "bin/javaw.exe"),
            Some(base.join("bin/javaw.exe"))
        );
        assert_eq!(safe_join(base, "a\\b.txt"), Some(base.join("a/b.txt")));
    }

    #[test]
    fn normalize_rel_canonicalizes() {
        assert_eq!(normalize_rel("mods/a.jar").as_deref(), Some("mods/a.jar"));
        assert_eq!(
            normalize_rel("config\\x.toml").as_deref(),
            Some("config/x.toml")
        );
        assert_eq!(normalize_rel("a b/c+d.jar").as_deref(), Some("a b/c+d.jar"));
        // Device names only match the whole stem.
        for ok in [
            "console.jar",
            "nullify.cfg",
            "com10.txt",
            "auxiliary/x",
            "CONFIG/x",
        ] {
            assert!(normalize_rel(ok).is_some(), "{ok:?}");
        }
    }

    #[test]
    fn normalize_rel_rejects_unsafe_parts() {
        for bad in [
            "",
            ".",
            "..",
            "./a",
            "a/.",
            "a/./b",
            "a//b",
            "a/",
            "/a",
            "\\a",
            "../x",
            "a/../../x",
            "C:/x",
            "C:x",
            "a\0b",
            "a\u{1}b",
            "mods/a.jar:evil",
            "a.jar:Zone.Identifier",
            "CON",
            "con.txt",
            "mods/NUL.jar",
            "Com1.log",
            "lpt9",
            "com0",
            "LPT0.txt",
            "conin$",
            "CONOUT$.x",
            "a/aux .txt",
            "com¹",
            "trail.",
            "trail ",
            "mods/x./y",
            "a<b",
            "a>b",
            "a|b",
            "a?b",
            "a*b",
            "a\"b",
        ] {
            assert_eq!(normalize_rel(bad), None, "{bad:?} must be rejected");
            assert_eq!(safe_join(Path::new("/base"), bad), None, "{bad:?}");
        }
        assert!(
            normalize_rel(&"a/".repeat(600)).is_none(),
            "overlong paths are refused"
        );
    }

    #[test]
    fn path_key_folds_case() {
        assert_eq!(path_key("Mods/A.jar"), path_key("mods/a.JAR"));
        assert_ne!(path_key("mods/a.jar"), path_key("mods/b.jar"));
    }
}
