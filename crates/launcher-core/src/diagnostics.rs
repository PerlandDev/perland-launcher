//! Export diagnostics (spec §5.2): one zip with what someone helping a player needs (logs,
//! instance files, settings, system info), with the home path and every invite token masked.
//! Never `accounts.json`, `jvm-args.txt`, caches or game files.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{
    download::redact_url,
    error::{ErrorCode, LauncherError, Result},
    invite::InviteLink,
    launch::Redactor,
    paths::LauncherDirs,
};

pub const LOG_TAIL_BYTES: u64 = 5 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub launcher_version: String,
    pub build: String,
    pub update_channel: String,
    pub os: String,
    pub arch: String,
    pub total_memory_mb: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Each log is cut to its last this-many bytes.
    pub log_tail: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            log_tail: LOG_TAIL_BYTES,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemJson<'a> {
    #[serde(flatten)]
    info: &'a SystemInfo,
    java_runtimes: Vec<String>,
    instances: usize,
}

pub async fn export(dirs: &LauncherDirs, info: &SystemInfo, out: &Path) -> Result<()> {
    export_with(
        dirs,
        info,
        out,
        Redactor::for_current_user(),
        Limits::default(),
    )
    .await
}

pub async fn export_with(
    dirs: &LauncherDirs,
    info: &SystemInfo,
    out: &Path,
    redactor: Redactor,
    limits: Limits,
) -> Result<()> {
    let (dirs, info, out) = (dirs.clone(), info.clone(), out.to_path_buf());
    tokio::task::spawn_blocking(move || write_zip(&dirs, &info, &out, redactor, limits))
        .await
        .map_err(|e| LauncherError::from(std::io::Error::other(e)))?
}

fn write_failed(e: impl std::fmt::Display) -> LauncherError {
    LauncherError::disk(
        ErrorCode::DiskWriteFailed,
        "Couldn't save the diagnostics file",
    )
    .with_param("item", "diagnostics")
    .with_fix("Pick another folder, and check there's free disk space")
    .with_detail(e)
}

/// Directory entries (name, path) sorted by name; a missing folder is empty.
fn listing(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    out.sort();
    out
}

/// `instance.json` with its invite masked and any pack URL stripped of query and user info,
/// plus the tokens found so they're masked everywhere else. A file that doesn't parse is
/// replaced by a note: its raw contents could hold anything.
fn sanitize_instance(raw: &str) -> (String, Vec<String>) {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return ("\"instance.json couldn't be read\"\n".into(), Vec::new());
    };
    let mut tokens = Vec::new();
    if let Some(source) = value.get_mut("source").and_then(|s| s.as_object_mut()) {
        if let Some(invite) = source
            .get("invite")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
        {
            if let Ok(link) = InviteLink::parse(&invite) {
                tokens.push(link.token().to_string());
            }
            source.insert("invite".into(), "<redacted>".into());
        }
        if let Some(url) = source
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
        {
            source.insert("url".into(), redact_url(&url).into());
        }
    }
    (
        serde_json::to_string_pretty(&value).unwrap_or_default() + "\n",
        tokens,
    )
}

/// The last `limit` bytes, starting at a whole line.
fn tail(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut bytes = Vec::new();
    if len <= limit {
        file.read_to_end(&mut bytes)?;
        return Ok(bytes);
    }
    file.seek(SeekFrom::Start(len - limit))?;
    file.take(limit).read_to_end(&mut bytes)?;
    let start = bytes
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |i| i + 1);
    let mut out = b"[earlier lines cut]\n".to_vec();
    out.extend_from_slice(&bytes[start..]);
    Ok(out)
}

fn write_zip(
    dirs: &LauncherDirs,
    info: &SystemInfo,
    out: &Path,
    mut redactor: Redactor,
    limits: Limits,
) -> Result<()> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let instances: Vec<_> = listing(&dirs.instances())
        .into_iter()
        .filter(|(_, p)| p.is_dir())
        .collect();
    for (id, dir) in &instances {
        if let Ok(raw) = std::fs::read_to_string(dir.join("instance.json")) {
            let (text, tokens) = sanitize_instance(&raw);
            for token in tokens {
                redactor = redactor.with_secret(token);
            }
            entries.push((format!("instances/{id}/instance.json"), text.into_bytes()));
        }
        if let Ok(bytes) = std::fs::read(dir.join(".perland").join("managed.json")) {
            entries.push((format!("instances/{id}/managed.json"), bytes));
        }
    }
    if let Ok(bytes) = std::fs::read(dirs.root().join("settings.json")) {
        entries.push(("settings.json".into(), bytes));
    }
    for (name, path) in listing(&crate::logging::dir(dirs)) {
        if name.ends_with(".log") {
            if let Ok(bytes) = tail(&path, limits.log_tail) {
                entries.push((format!("logs/launcher/{name}"), bytes));
            }
        }
    }
    for (name, path) in listing(&dirs.logs()) {
        if name.ends_with("-latest.log") && path.is_file() {
            if let Ok(bytes) = tail(&path, limits.log_tail) {
                entries.push((format!("logs/{name}"), bytes));
            }
        }
    }
    let system = SystemJson {
        info,
        java_runtimes: listing(&dirs.java())
            .into_iter()
            .filter(|(_, p)| p.is_dir())
            .map(|(n, _)| n)
            .collect(),
        instances: instances.len(),
    };
    entries.push((
        "system.json".into(),
        serde_json::to_vec_pretty(&system).unwrap_or_default(),
    ));

    let part = out.with_extension("zip.part");
    let written = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut zip = ZipWriter::new(File::create(&part)?);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(name, options)?;
            zip.write_all(redactor.redact(&String::from_utf8_lossy(&bytes)).as_bytes())?;
        }
        zip.finish()?;
        std::fs::rename(&part, out)?;
        Ok(())
    })();
    written.map_err(|e| {
        let _ = std::fs::remove_file(&part);
        write_failed(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    const HOME: &str = "C:\\Users\\steve";

    fn token() -> String {
        "A".repeat(43)
    }

    fn info() -> SystemInfo {
        SystemInfo {
            launcher_version: "0.2.0-beta.1".into(),
            build: "beta".into(),
            update_channel: "beta".into(),
            os: "Windows 11 Pro".into(),
            arch: "x86_64".into(),
            total_memory_mb: 16384,
        }
    }

    /// A data folder with one Perland server instance, logs, settings and accounts.
    fn seed() -> (tempfile::TempDir, LauncherDirs) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path().join("data"));
        let inst = dirs.instances().join("asalith");
        std::fs::create_dir_all(inst.join(".perland")).unwrap();
        std::fs::write(
            inst.join("instance.json"),
            format!(r#"{{"id":"asalith","name":"Asalith","source":{{"kind":"perland","invite":"https://play.example.com/{}","engine":"https://engine.example.com"}}}}"#, token()),
        )
        .unwrap();
        std::fs::write(inst.join(".perland/managed.json"), r#"{"files":[]}"#).unwrap();
        let broken = dirs.instances().join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(
            broken.join("instance.json"),
            format!("{{ not json {HOME} {}", token()),
        )
        .unwrap();
        std::fs::create_dir_all(crate::logging::dir(&dirs)).unwrap();
        std::fs::write(
            crate::logging::dir(&dirs).join("launcher.2026-10-14.log"),
            format!(
                "opened {HOME}\\AppData\njoin https://play.example.com/{}\n",
                token()
            ),
        )
        .unwrap();
        std::fs::write(
            dirs.logs().join("asalith-latest.log"),
            format!("[main] game dir {HOME}\\x\n[main] saved C:/Users/steve/AppData/x\n"),
        )
        .unwrap();
        std::fs::write(dirs.root().join("settings.json"), r#"{"language":"es"}"#).unwrap();
        std::fs::write(dirs.root().join("accounts.json"), r#"{"accounts":[]}"#).unwrap();
        std::fs::create_dir_all(dirs.java().join("java-runtime-delta")).unwrap();
        (tmp, dirs)
    }

    fn entries(zip: &Path) -> Vec<(String, String)> {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut f = archive.by_index(i).unwrap();
                let mut text = String::new();
                f.read_to_string(&mut text).unwrap();
                (f.name().to_string(), text)
            })
            .collect()
    }

    async fn run(dirs: &LauncherDirs, out: &Path, limits: Limits) -> Result<()> {
        export_with(dirs, &info(), out, Redactor::new().with_home(HOME), limits).await
    }

    #[tokio::test]
    async fn the_zip_holds_what_support_needs_and_never_accounts() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("perland-diagnostics.zip");
        run(&dirs, &out, Limits::default()).await.unwrap();
        let mut names: Vec<_> = entries(&out).into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "instances/asalith/instance.json",
                "instances/asalith/managed.json",
                "instances/broken/instance.json",
                "logs/asalith-latest.log",
                "logs/launcher/launcher.2026-10-14.log",
                "settings.json",
                "system.json",
            ]
        );
        let system = entries(&out)
            .into_iter()
            .find(|(n, _)| n == "system.json")
            .unwrap()
            .1;
        assert!(system.contains("\"build\": \"beta\""), "{system}");
        assert!(system.contains("java-runtime-delta"), "{system}");
        assert!(system.contains("\"instances\": 2"), "{system}");
    }

    #[tokio::test]
    async fn tokens_and_the_home_path_are_redacted_everywhere() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("d.zip");
        run(&dirs, &out, Limits::default()).await.unwrap();
        let forward = HOME.replace('\\', "/");
        for (name, text) in entries(&out) {
            assert!(
                !text.contains(&token()),
                "{name} leaks the invite token:\n{text}"
            );
            assert!(!text.contains(HOME), "{name} leaks the home path:\n{text}");
            assert!(
                !text.contains(&forward),
                "{name} leaks the forward-slash home path:\n{text}"
            );
        }
    }

    #[tokio::test]
    async fn a_damaged_instance_file_is_left_out() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("d.zip");
        run(&dirs, &out, Limits::default()).await.unwrap();
        let broken = entries(&out)
            .into_iter()
            .find(|(n, _)| n == "instances/broken/instance.json")
            .unwrap()
            .1;
        assert_eq!(broken.trim(), "\"instance.json couldn't be read\"");
    }

    #[tokio::test]
    async fn long_logs_keep_their_tail() {
        let (tmp, dirs) = seed();
        let lines: String = (0..20).map(|i| format!("line {i:02}\n")).collect();
        std::fs::write(dirs.logs().join("asalith-latest.log"), &lines).unwrap();
        let out = tmp.path().join("d.zip");
        run(&dirs, &out, Limits { log_tail: 40 }).await.unwrap();
        let log = entries(&out)
            .into_iter()
            .find(|(n, _)| n == "logs/asalith-latest.log")
            .unwrap()
            .1;
        assert!(log.starts_with("[earlier lines cut]\n"), "{log}");
        assert!(log.ends_with("line 19\n"), "{log}");
        assert!(!log.contains("line 00"), "{log}");
    }

    #[tokio::test]
    async fn missing_folder_fails_cleanly() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("nope").join("d.zip");
        let err = run(&dirs, &out, Limits::default()).await.unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::DiskWriteFailed);
        assert!(!tmp.path().join("nope").exists());
    }

    #[tokio::test]
    async fn an_existing_file_is_replaced_and_no_part_file_is_left() {
        let (tmp, dirs) = seed();
        let out = tmp.path().join("d.zip");
        std::fs::write(&out, "old").unwrap();
        run(&dirs, &out, Limits::default()).await.unwrap();
        assert!(!entries(&out).is_empty());
        assert!(!tmp.path().join("d.zip.part").exists());
    }
}
