pub mod redact;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};

pub use redact::Redactor;

use crate::{
    accounts::Session,
    error::{LauncherError, Result},
    events::{Event, EventSink},
    install::{classpath, ResolvedVersion},
    meta::rules::{resolve_args, RuleContext},
    paths::LauncherDirs,
    platform::Platform,
};

pub const LAUNCHER_NAME: &str = "perland-launcher";

pub struct LaunchSpec<'a> {
    pub dirs: &'a LauncherDirs,
    pub resolved: &'a ResolvedVersion,
    pub game_dir: &'a Path,
    pub java: &'a Path,
    pub session: &'a Session,
    pub memory_mb: u32,
    pub platform: Platform,
    pub launcher_version: &'a str,
}

#[derive(Clone)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub classpath: Vec<PathBuf>,
}

// Manual Debug: `args` carries the access token, so it must never be printed.
impl std::fmt::Debug for LaunchCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchCommand")
            .field("program", &self.program)
            .field("args", &format_args!("<{} args>", self.args.len()))
            .field("cwd", &self.cwd)
            .field("classpath", &self.classpath.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExit {
    pub code: Option<i32>,
}

/// Replace `${name}` with values from `vars`; unknown names are left as written.
pub fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn build_command(spec: &LaunchSpec) -> Result<LaunchCommand> {
    let ctx = RuleContext::new(spec.platform);
    let json = &spec.resolved.json;
    let classpath = classpath(spec.resolved, spec.dirs, &ctx)?;
    let cp_string = classpath
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(spec.platform.classpath_separator());
    let assets_index = json
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .or_else(|| json.assets.clone())
        .unwrap_or_default();
    let s = spec.session;
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", s.username.clone()),
        ("version_name", json.id.clone()),
        ("game_directory", path_str(spec.game_dir)),
        ("assets_root", path_str(&spec.dirs.assets())),
        ("game_assets", path_str(&spec.dirs.assets())),
        ("assets_index_name", assets_index),
        ("auth_uuid", s.uuid.clone()),
        ("auth_access_token", s.access_token.clone()),
        ("auth_session", s.access_token.clone()),
        ("clientid", String::new()),
        (
            "auth_xuid",
            s.xuid.clone().unwrap_or_else(|| "0".to_string()),
        ),
        ("user_type", s.user_type.clone()),
        ("user_properties", "{}".to_string()),
        (
            "version_type",
            json.kind.clone().unwrap_or_else(|| "release".to_string()),
        ),
        (
            "natives_directory",
            path_str(&spec.dirs.natives_dir(&json.id)),
        ),
        ("launcher_name", LAUNCHER_NAME.to_string()),
        ("launcher_version", spec.launcher_version.to_string()),
        ("classpath", cp_string),
        (
            "classpath_separator",
            spec.platform.classpath_separator().to_string(),
        ),
        ("library_directory", path_str(&spec.dirs.libraries())),
    ]);

    let mut args = vec![format!("-Xmx{}M", spec.memory_mb)];
    match &json.arguments {
        Some(a) if !a.jvm.is_empty() => args.extend(
            resolve_args(&a.jvm, &ctx)
                .iter()
                .map(|t| substitute(t, &vars)),
        ),
        _ => args.extend(
            [
                "-Djava.library.path=${natives_directory}",
                "-cp",
                "${classpath}",
            ]
            .iter()
            .map(|t| substitute(t, &vars)),
        ),
    }
    if let Some(client) = json.logging.as_ref().and_then(|l| l.client.as_ref()) {
        let path_var = HashMap::from([("path", path_str(&spec.dirs.log_config(&client.file.id)))]);
        args.push(substitute(&client.argument, &path_var));
    }
    args.push(json.main_class.clone());
    match (&json.arguments, &json.minecraft_arguments) {
        (Some(a), _) if !a.game.is_empty() => args.extend(
            resolve_args(&a.game, &ctx)
                .iter()
                .map(|t| substitute(t, &vars)),
        ),
        (_, Some(legacy)) => args.extend(legacy.split_whitespace().map(|t| substitute(t, &vars))),
        _ => {
            return Err(LauncherError::loader(format!(
                "{} has no game arguments",
                json.id
            )))
        }
    }
    Ok(LaunchCommand {
        program: spec.java.to_path_buf(),
        args,
        cwd: spec.game_dir.to_path_buf(),
        classpath,
    })
}

/// Files the command needs that aren't on disk. Empty means the launch can proceed.
pub fn missing_files(cmd: &LaunchCommand) -> Vec<PathBuf> {
    std::iter::once(&cmd.program)
        .chain(cmd.classpath.iter())
        .filter(|p| !p.is_file())
        .cloned()
        .collect()
}

/// Decode one raw output line lossily (invalid bytes become U+FFFD) and strip the trailing `\n` / `\r\n`.
fn decode_line(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Start the game, stream every output line (redacted) to the sink and the log file, and wait for it to exit.
pub async fn run(
    cmd: &LaunchCommand,
    redactor: &Redactor,
    sink: &EventSink,
    log_file: &Path,
) -> Result<GameExit> {
    tokio::fs::create_dir_all(&cmd.cwd).await?;
    if let Some(parent) = log_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Create the log file before spawning: if this fails, no game process exists yet to be orphaned.
    let mut log = Some(tokio::fs::File::create(log_file).await?);
    let mut command = tokio::process::Command::new(&cmd.program);
    command
        .args(&cmd.args)
        .current_dir(&cmd.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn().map_err(|e| {
        LauncherError::java("Java couldn't be started")
            .with_fix("Reinstall Java from the instance settings")
            .with_detail(e)
    })?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let readers: [Box<dyn AsyncRead + Unpin + Send>; 2] = [Box::new(stdout), Box::new(stderr)];
    for reader in readers {
        let tx = tx.clone();
        tokio::spawn(async move {
            // Read raw bytes: the game may write in a non-UTF-8 charset, which must not end capture.
            let mut reader = BufReader::new(reader);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx.send(decode_line(&buf)).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
    drop(tx);
    while let Some(line) = rx.recv().await {
        let clean = redactor.redact(&line);
        // A log write failure is non-fatal: stop writing the file, keep streaming to the sink.
        if let Some(file) = log.as_mut() {
            let wrote = match file.write_all(clean.as_bytes()).await {
                Ok(()) => file.write_all(b"\n").await,
                Err(e) => Err(e),
            };
            if wrote.is_err() {
                log = None;
            }
        }
        sink(Event::Log { line: clean });
    }
    if let Some(file) = log.as_mut() {
        let _ = file.flush().await;
    }
    // Always reached once the child has been spawned, so the game is never left unowned.
    let status = child.wait().await?;
    Ok(GameExit {
        code: status.code(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts::offline_account,
        meta::merge::merge,
        test_support::{version, LINUX_X64, WINDOWS_X64},
    };

    fn build(resolved: &ResolvedVersion, platform: Platform) -> LaunchCommand {
        let dirs = LauncherDirs::new("/pl");
        let session = offline_account("Steve").unwrap().offline_session();
        build_command(&LaunchSpec {
            dirs: &dirs,
            resolved,
            game_dir: Path::new("/pl/instances/x/minecraft"),
            java: Path::new("/pl/java/x/bin/java"),
            session: &session,
            memory_mb: 4096,
            platform,
            launcher_version: "0.1.0",
        })
        .unwrap()
    }

    fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let i = args
            .iter()
            .position(|a| a == flag)
            .unwrap_or_else(|| panic!("{flag} missing"));
        &args[i + 1]
    }

    #[test]
    fn substitute_replaces_known_and_keeps_unknown() {
        let vars = HashMap::from([("a", "1".to_string())]);
        assert_eq!(substitute("x${a}y${b}z", &vars), "x1y${b}z");
        assert_eq!(substitute("open ${a", &vars), "open ${a");
    }

    #[test]
    fn modern_command_on_windows() {
        let resolved = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: version("1.20.1.json"),
        };
        let cmd = build(&resolved, WINDOWS_X64);
        let a = &cmd.args;
        assert_eq!(a[0], "-Xmx4096M");
        let cp = arg_after(a, "-cp");
        assert!(cp.contains(';') && cp.ends_with("1.20.1.jar"), "{cp}");
        assert!(
            a.iter()
                .any(|x| x.starts_with("-Dlog4j.configurationFile=")
                    && x.ends_with("client-1.12.xml"))
        );
        assert!(a
            .iter()
            .any(|x| x == "-Dminecraft.launcher.brand=perland-launcher"));
        assert!(a.contains(&"net.minecraft.client.main.Main".to_string()));
        assert_eq!(arg_after(a, "--username"), "Steve");
        assert_eq!(arg_after(a, "--uuid"), "5627dd98e6be3c21b8a8e92344183641");
        assert_eq!(arg_after(a, "--assetIndex"), "5");
        assert_eq!(arg_after(a, "--version"), "1.20.1");
        assert!(!a.contains(&"--demo".to_string()));
        assert!(!a.contains(&"-XstartOnFirstThread".to_string()));
        assert!(
            !a.iter().any(|x| x.contains("${")),
            "no unresolved templates: {a:?}"
        );
        let main = a
            .iter()
            .position(|x| x == "net.minecraft.client.main.Main")
            .unwrap();
        assert!(
            a.iter().position(|x| x == "-cp").unwrap() < main,
            "jvm args come before the main class"
        );
        assert_eq!(cmd.cwd, Path::new("/pl/instances/x/minecraft"));
        assert_eq!(cmd.classpath.len(), 51);
    }

    #[test]
    fn legacy_command_on_linux() {
        let resolved = ResolvedVersion {
            vanilla_id: "1.12.2".into(),
            json: version("1.12.2.json"),
        };
        let cmd = build(&resolved, LINUX_X64);
        assert!(cmd
            .args
            .iter()
            .any(|x| x.starts_with("-Djava.library.path=") && x.ends_with("natives")));
        assert!(arg_after(&cmd.args, "-cp").contains(':'));
        assert_eq!(arg_after(&cmd.args, "--username"), "Steve");
        assert_eq!(arg_after(&cmd.args, "--assetIndex"), "1.12");
    }

    #[test]
    fn fabric_command_uses_knot() {
        let resolved = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: merge(
                version("1.20.1.json"),
                version("fabric-1.20.1-0.16.14.json"),
            ),
        };
        let cmd = build(&resolved, WINDOWS_X64);
        assert!(cmd
            .args
            .contains(&"net.fabricmc.loader.impl.launch.knot.KnotClient".to_string()));
        assert!(cmd
            .args
            .contains(&"-DFabricMcEmu= net.minecraft.client.main.Main ".to_string()));
        assert_eq!(
            arg_after(&cmd.args, "--version"),
            "fabric-loader-0.16.14-1.20.1"
        );
    }

    #[test]
    fn decode_line_is_lossy_and_strips_line_endings() {
        let out = decode_line(b"caf\xe9 ok\r\n");
        assert!(out.contains("caf") && out.ends_with(" ok"), "{out:?}");
        assert!(!out.contains('\r') && !out.contains('\n'), "{out:?}");
        assert_eq!(decode_line(b"plain\n"), "plain");
        assert_eq!(decode_line(b"no newline"), "no newline");
        assert_eq!(decode_line(b"\r\n"), "");
    }

    #[test]
    fn dry_run_reports_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let program = tmp.path().join("java");
        let jar = tmp.path().join("a.jar");
        let cmd = LaunchCommand {
            program: program.clone(),
            args: vec![],
            cwd: tmp.path().into(),
            classpath: vec![jar.clone()],
        };
        assert_eq!(missing_files(&cmd), vec![program.clone(), jar.clone()]);
        std::fs::write(&program, "").unwrap();
        std::fs::write(&jar, "").unwrap();
        assert!(missing_files(&cmd).is_empty());
    }

    #[tokio::test]
    async fn run_streams_redacted_output() {
        // Uses the java already on PATH (present on CI runners and dev machines); skipped if absent.
        if std::process::Command::new("java")
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let l2 = lines.clone();
        let sink: EventSink = std::sync::Arc::new(move |e| {
            if let Event::Log { line } = e {
                l2.lock().unwrap().push(line);
            }
        });
        let cmd = LaunchCommand {
            program: "java".into(),
            args: vec!["-version".into()],
            cwd: tmp.path().join("game"),
            classpath: vec![],
        };
        let redactor = Redactor::new().with_secret("version");
        let log = tmp.path().join("logs/latest.log");
        let exit = run(&cmd, &redactor, &sink, &log).await.unwrap();
        assert_eq!(exit.code, Some(0));
        let all = lines.lock().unwrap().join("\n");
        assert!(all.contains("<redacted>"), "{all}");
        assert!(!all.contains("version"), "{all}");
        assert!(std::fs::read_to_string(&log)
            .unwrap()
            .contains("<redacted>"));
    }
}
