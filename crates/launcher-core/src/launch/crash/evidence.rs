//! What a crashed game left behind, read capped and redacted. Files older than the launch belong
//! to an earlier run and are skipped; a file that can't be read is skipped too.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use super::{Evidence, Source};
use crate::launch::Redactor;

/// The first part of a crash report read.
pub(crate) const CRASH_REPORT_BYTES: u64 = 256 << 10;
/// The last part of `latest.log` read.
pub(crate) const LATEST_LOG_BYTES: u64 = 512 << 10;
/// The first part of a JVM fatal error log read: the problematic frame is near its top.
pub(crate) const HS_ERR_BYTES: u64 = 128 << 10;
/// File times can be coarse (FAT keeps 2 s), so "written during the run" allows this much.
const CLOCK_SLACK: Duration = Duration::from_secs(2);

/// The tail first, then the crash report, `latest.log` and the JVM's fatal error log.
pub async fn gather(
    game_dir: &Path,
    pid: Option<u32>,
    started: SystemTime,
    tail: &[String],
    redactor: &Redactor,
) -> Vec<Evidence> {
    let (dir, redactor) = (game_dir.to_path_buf(), redactor.clone());
    let mut evidence = vec![Evidence {
        source: Source::Tail,
        text: tail.join("\n"),
    }];
    let files = tokio::task::spawn_blocking(move || read_files(&dir, pid, started, &redactor))
        .await
        .unwrap_or_default();
    evidence.extend(files);
    evidence
}

fn read_files(
    dir: &Path,
    pid: Option<u32>,
    started: SystemTime,
    redactor: &Redactor,
) -> Vec<Evidence> {
    let since = started.checked_sub(CLOCK_SLACK).unwrap_or(started);
    let fresh = |path: &Path| modified(path).is_some_and(|t| t >= since);
    let mut out = Vec::new();
    let mut add = |source, text: Option<String>| {
        if let Some(text) = text {
            out.push(Evidence {
                source,
                text: redactor.redact_cut(&text.replace('\r', "")),
            });
        }
    };
    if let Some(report) = newest_crash_report(&dir.join("crash-reports"), since) {
        add(Source::CrashReport, read_head(&report, CRASH_REPORT_BYTES));
    }
    let log = dir.join("logs").join("latest.log");
    if fresh(&log) {
        add(Source::LatestLog, read_tail(&log, LATEST_LOG_BYTES));
    }
    if let Some(pid) = pid {
        let hs_err = dir.join(format!("hs_err_pid{pid}.log"));
        if fresh(&hs_err) {
            add(Source::HsErr, read_head(&hs_err, HS_ERR_BYTES));
        }
    }
    out
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn newest_crash_report(dir: &Path, since: SystemTime) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("crash-") && name.ends_with(".txt")
        })
        .filter_map(|e| Some((modified(&e.path())?, e.path())))
        .filter(|(t, _)| *t >= since)
        .max_by_key(|(t, _)| *t)
        .map(|(_, path)| path)
}

fn read_head(path: &Path, limit: u64) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The last `limit` bytes, from the first whole line in them.
fn read_tail(path: &Path, limit: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(limit);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(match (start > 0, text.find('\n')) {
        (true, Some(i)) => text[i + 1..].to_string(),
        _ => text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: impl AsRef<[u8]>) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn age(path: &Path, by: Duration) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - by).unwrap();
    }

    async fn sources(dir: &Path, pid: Option<u32>, started: SystemTime) -> Vec<Evidence> {
        gather(dir, pid, started, &["tail line".into()], &Redactor::new()).await
    }

    #[tokio::test]
    async fn newest_fresh_crash_report_is_read() {
        let tmp = tempfile::tempdir().unwrap();
        let reports = tmp.path().join("crash-reports");
        write(
            &reports.join("crash-2026-10-21_10.00.00-client.txt"),
            "older",
        );
        age(
            &reports.join("crash-2026-10-21_10.00.00-client.txt"),
            Duration::from_secs(1),
        );
        write(
            &reports.join("crash-2026-10-21_10.00.01-client.txt"),
            "newest",
        );
        write(&reports.join("notes.txt"), "not a report");
        let started = SystemTime::now() - Duration::from_secs(60);
        let ev = sources(tmp.path(), None, started).await;
        assert_eq!(ev[0].source, Source::Tail);
        assert_eq!(ev[0].text, "tail line");
        assert_eq!(ev[1].source, Source::CrashReport);
        assert_eq!(ev[1].text, "newest");
    }

    #[tokio::test]
    async fn stale_crash_report_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let report = tmp.path().join("crash-reports/crash-old-client.txt");
        write(&report, "java.lang.OutOfMemoryError");
        age(&report, Duration::from_secs(3600));
        let log = tmp.path().join("logs/latest.log");
        write(&log, "old log");
        age(&log, Duration::from_secs(3600));
        let ev = sources(tmp.path(), None, SystemTime::now()).await;
        assert_eq!(ev.len(), 1, "{ev:?}");
    }

    #[tokio::test]
    async fn latest_log_reads_the_last_part() {
        let tmp = tempfile::tempdir().unwrap();
        let filler = "x".repeat(LATEST_LOG_BYTES as usize);
        write(
            &tmp.path().join("logs/latest.log"),
            format!("first line\n{filler}\nlast line\r\n"),
        );
        let ev = sources(tmp.path(), None, SystemTime::now()).await;
        let log = ev.iter().find(|e| e.source == Source::LatestLog).unwrap();
        assert!(log.text.ends_with("last line\n"));
        assert!(!log.text.contains("first line"));
        assert!(!log.text.contains('\r'));
        assert!(log.text.len() as u64 <= LATEST_LOG_BYTES);
    }

    #[tokio::test]
    async fn hs_err_matches_the_pid() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("hs_err_pid41.log"),
            "# C  [nvoglv64.dll+0x1]",
        );
        write(&tmp.path().join("hs_err_pid99.log"), "another game");
        let ev = sources(tmp.path(), Some(41), SystemTime::now()).await;
        let hs = ev
            .iter()
            .filter(|e| e.source == Source::HsErr)
            .collect::<Vec<_>>();
        assert_eq!(hs.len(), 1);
        assert!(hs[0].text.contains("nvoglv64"));
        assert!(sources(tmp.path(), None, SystemTime::now())
            .await
            .iter()
            .all(|e| e.source != Source::HsErr));
    }

    #[tokio::test]
    async fn evidence_is_redacted() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("logs/latest.log"),
            "--accessToken s3cr3t-token at /home/steve/.minecraft",
        );
        let redactor = Redactor::new()
            .with_secret("s3cr3t-token")
            .with_home("/home/steve");
        let ev = gather(tmp.path(), None, SystemTime::now(), &[], &redactor).await;
        let log = ev.iter().find(|e| e.source == Source::LatestLog).unwrap();
        assert_eq!(log.text, "--accessToken <redacted> at ~/.minecraft");
    }

    #[tokio::test]
    async fn invalid_utf8_is_lossy() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("logs/latest.log"), b"ok \xff\xfe end");
        let ev = sources(tmp.path(), None, SystemTime::now()).await;
        let log = ev.iter().find(|e| e.source == Source::LatestLog).unwrap();
        assert!(log.text.starts_with("ok ") && log.text.ends_with(" end"));
    }
}
