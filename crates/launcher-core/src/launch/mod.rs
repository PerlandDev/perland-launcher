pub mod crash;
pub mod redact;

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    process::Child,
    sync::{mpsc, oneshot, watch},
};

pub use crash::CrashHint;
pub use redact::Redactor;

use crate::{
    accounts::Session,
    error::{ErrorCode, LauncherError, Result},
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

/// Longest output line kept; the rest of a longer line is dropped.
pub const MAX_LINE_BYTES: usize = 8 * 1024;
/// How many of the last output lines a finished game reports.
pub const TAIL_LINES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExit {
    pub code: Option<i32>,
    /// True when the launcher stopped the game on request.
    pub killed: bool,
    /// The last `TAIL_LINES` redacted output lines.
    pub tail: Vec<String>,
    /// A likely cause, only for exits nobody asked for that weren't clean.
    pub crash: Option<CrashHint>,
}

/// Asks a running game to stop. Cheap to clone; calling `kill` twice is harmless.
#[derive(Clone)]
pub struct GameKiller(Arc<watch::Sender<bool>>);

impl GameKiller {
    pub fn kill(&self) {
        // Err only means the game already finished and nobody is listening.
        let _ = self.0.send(true);
    }
}

/// A started game. Dropping this without waiting never kills the game.
pub struct GameProcess {
    pid: Option<u32>,
    killer: GameKiller,
    done: oneshot::Receiver<Result<GameExit>>,
}

impl GameProcess {
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
    pub fn killer(&self) -> GameKiller {
        self.killer.clone()
    }
    pub async fn wait(self) -> Result<GameExit> {
        self.done.await.unwrap_or_else(|_| {
            Err(LauncherError::game(
                ErrorCode::Internal,
                "Lost track of the game process",
            ))
        })
    }
}

#[derive(Default)]
struct Tail(VecDeque<String>);

impl Tail {
    fn push(&mut self, line: String) {
        if self.0.len() == TAIL_LINES {
            self.0.pop_front();
        }
        self.0.push_back(line);
    }
    fn lines(&self) -> Vec<String> {
        self.0.iter().cloned().collect()
    }
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
            return Err(LauncherError::loader(
                ErrorCode::NoGameArguments,
                format!("{} has no game arguments", json.id),
            )
            .with_param("version", &json.id))
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
    // Classpath entries are resolved by Java against its working directory, `cmd.cwd`.
    let classpath_missing = cmd.classpath.iter().filter(|p| !cmd.cwd.join(p).is_file());
    std::iter::once(&cmd.program)
        .filter(|p| !p.is_file())
        .chain(classpath_missing)
        .cloned()
        .collect()
}

/// Decode one raw output line lossily (invalid bytes become U+FFFD) and strip the trailing `\n` / `\r\n`.
/// A truncated line is cut back to its last whole character and marked.
fn decode_line(bytes: &[u8], truncated: bool) -> String {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    if !truncated {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let keep = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        // An incomplete sequence at the very end is the half character the cap cut through.
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => bytes.len(),
    };
    let mut line = String::from_utf8_lossy(&bytes[..keep]).into_owned();
    line.push_str(" …[truncated]");
    line
}

/// Read one line into `buf`, keeping at most `max` bytes of it. The rest of an over-long line is
/// consumed and dropped so a runaway line can't grow memory. Returns `(got_a_line, truncated)`;
/// `got_a_line` is false at end of stream.
async fn read_capped_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<(bool, bool)> {
    let mut got = false;
    let mut truncated = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok((got, truncated));
        }
        got = true;
        let (chunk, found_newline) = match available.iter().position(|&b| b == b'\n') {
            Some(i) => (&available[..=i], true),
            None => (available, false),
        };
        let room = max.saturating_sub(buf.len());
        if chunk.len() > room {
            truncated = true;
        }
        buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
        let used = chunk.len();
        reader.consume(used);
        if found_newline {
            return Ok((true, truncated));
        }
    }
}

async fn read_lines(reader: Box<dyn AsyncRead + Unpin + Send>, tx: mpsc::UnboundedSender<String>) {
    // Read raw bytes: the game may write in a non-UTF-8 charset, which must not end capture.
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES).await {
            Ok((true, truncated)) => {
                if tx.send(decode_line(&buf, truncated)).is_err() {
                    break;
                }
            }
            Ok((false, _)) | Err(_) => break,
        }
    }
}

type LogWriter = Box<dyn AsyncWrite + Send + Unpin>;

/// Redact each line, append it to the log (until writing fails once), keep the tail, and stream it.
async fn pump_lines(
    mut rx: mpsc::UnboundedReceiver<String>,
    redactor: Redactor,
    sink: EventSink,
    mut log: LogWriter,
    tail: Arc<Mutex<Tail>>,
) {
    let mut log_ok = true;
    while let Some(line) = rx.recv().await {
        let clean = redactor.redact(&line);
        if log_ok {
            let wrote = match log.write_all(clean.as_bytes()).await {
                Ok(()) => log.write_all(b"\n").await,
                Err(e) => Err(e),
            };
            if let Err(e) = wrote {
                log_ok = false;
                sink(Event::Warning(
                    LauncherError::disk(
                        ErrorCode::LogWriteStopped,
                        "The game log stopped being saved to disk",
                    )
                    .with_fix("Check free disk space. The game keeps running.")
                    .with_detail(e),
                ));
            }
        }
        tail.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(clean.clone());
        sink(Event::Log { line: clean });
    }
    if log_ok {
        let _ = log.flush().await;
    }
}

/// Wait for the child, killing it when asked. Returns `(exit code, killed)`.
async fn wait_for_exit(
    mut child: Child,
    mut kill_rx: watch::Receiver<bool>,
) -> Result<(Option<i32>, bool)> {
    let mut killed = false;
    let mut listening = true;
    loop {
        tokio::select! {
            status = child.wait() => return Ok((status?.code(), killed)),
            changed = kill_rx.changed(), if listening && !killed => match changed {
                Ok(()) if *kill_rx.borrow_and_update() => {
                    killed = true;
                    let _ = child.start_kill();
                }
                Ok(()) => {}
                // Every handle was dropped: keep waiting, never kill.
                Err(_) => listening = false,
            },
        }
    }
}

/// Start the game. Output lines are redacted, written to `log_file`, kept as a tail and streamed
/// to `sink`. The returned handle can kill the game and wait for its exit.
pub async fn spawn(
    cmd: &LaunchCommand,
    redactor: Redactor,
    sink: EventSink,
    log_file: &Path,
) -> Result<GameProcess> {
    tokio::fs::create_dir_all(&cmd.cwd).await?;
    if let Some(parent) = log_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Create the log file before spawning: if this fails, no game process exists yet to be orphaned.
    let log = tokio::fs::File::create(log_file).await?;
    spawn_with_log(cmd, redactor, sink, Box::new(log))
}

/// Everything after the process starts runs without an `.await` until its tasks own it, so a
/// caller cancelled at its next `.await` can never drop a started game that nobody waits for.
fn spawn_with_log(
    cmd: &LaunchCommand,
    redactor: Redactor,
    sink: EventSink,
    log: LogWriter,
) -> Result<GameProcess> {
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
        LauncherError::java(ErrorCode::JavaStartFailed, "Java couldn't be started")
            .with_fix("Reinstall Java from the instance settings")
            .with_detail(e)
    })?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (line_tx, line_rx) = mpsc::unbounded_channel::<String>();
    let readers: [Box<dyn AsyncRead + Unpin + Send>; 2] = [Box::new(stdout), Box::new(stderr)];
    for reader in readers {
        tokio::spawn(read_lines(reader, line_tx.clone()));
    }
    drop(line_tx);
    let tail = Arc::new(Mutex::new(Tail::default()));
    let pump = tokio::spawn(pump_lines(line_rx, redactor, sink, log, tail.clone()));
    let (kill_tx, kill_rx) = watch::channel(false);
    let (done_tx, done_rx) = oneshot::channel();
    tokio::spawn(async move {
        let result = wait_for_exit(child, kill_rx).await;
        // Let the output drain; a grandchild holding the pipes open must not hang the exit.
        let _ = tokio::time::timeout(Duration::from_secs(5), pump).await;
        let tail = tail.lock().unwrap_or_else(|e| e.into_inner()).lines();
        let exit = result.map(|(code, killed)| {
            let crash = if !killed && code != Some(0) {
                crash::diagnose(&tail)
            } else {
                None
            };
            GameExit {
                code,
                killed,
                tail,
                crash,
            }
        });
        let _ = done_tx.send(exit);
    });
    Ok(GameProcess {
        pid,
        killer: GameKiller(Arc::new(kill_tx)),
        done: done_rx,
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

    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncWrite;

    fn collect() -> (EventSink, Arc<Mutex<Vec<Event>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        (Arc::new(move |e| s2.lock().unwrap().push(e)), seen)
    }

    fn logs(seen: &Arc<Mutex<Vec<Event>>>) -> Vec<String> {
        seen.lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::Log { line } => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    /// Prints "alpha" then "bravo" on every OS. On Windows the `&` must sit inside an argument
    /// without spaces: an argument with spaces gets quoted, and cmd won't split a quoted `&`.
    fn two_lines(cwd: &Path) -> LaunchCommand {
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/C", "echo", "alpha&echo", "bravo"])
        } else {
            ("sh", &["-c", "echo alpha; echo bravo"])
        };
        LaunchCommand {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: cwd.to_path_buf(),
            classpath: vec![],
        }
    }

    #[test]
    fn decode_line_is_lossy_and_strips_line_endings() {
        let out = decode_line(b"caf\xe9 ok\r\n", false);
        assert!(out.contains("caf") && out.ends_with(" ok"), "{out:?}");
        assert!(!out.contains('\r') && !out.contains('\n'), "{out:?}");
        assert_eq!(decode_line(b"plain\n", false), "plain");
        assert_eq!(decode_line(b"\r\n", false), "");
    }

    #[test]
    fn truncated_lines_end_on_a_whole_character() {
        // "ab" then the first byte of a two-byte "ñ": the cut must drop the half character.
        assert_eq!(decode_line(b"ab\xc3", true), "ab …[truncated]");
    }

    #[tokio::test]
    async fn capped_line_drops_the_overflow() {
        let mut input = vec![b'a'; MAX_LINE_BYTES * 3];
        input.extend_from_slice(b"\nnext\n");
        let mut reader = tokio::io::BufReader::new(&input[..]);
        let mut buf = Vec::new();
        let (got, truncated) = read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES)
            .await
            .unwrap();
        assert!(got && truncated);
        assert_eq!(buf.len(), MAX_LINE_BYTES);
        buf.clear();
        let (got, truncated) = read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES)
            .await
            .unwrap();
        assert!(got && !truncated);
        assert_eq!(buf, b"next\n");
        buf.clear();
        let (got, _) = read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES)
            .await
            .unwrap();
        assert!(!got, "end of stream");
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        let mut tail = Tail::default();
        for i in 0..(TAIL_LINES + 5) {
            tail.push(i.to_string());
        }
        let lines = tail.lines();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[0], "5");
    }

    #[tokio::test]
    async fn spawn_streams_redacted_output_and_writes_the_log() {
        let tmp = tempfile::tempdir().unwrap();
        let (sink, seen) = collect();
        let log = tmp.path().join("logs/latest.log");
        let redactor = Redactor::new().with_secret("bravo");
        let game = spawn(&two_lines(tmp.path()), redactor, sink, &log)
            .await
            .unwrap();
        let exit = game.wait().await.unwrap();
        assert_eq!(exit.code, Some(0));
        assert!(!exit.killed);
        assert_eq!(exit.crash, None);
        assert_eq!(logs(&seen), vec!["alpha", "<redacted>"]);
        assert_eq!(exit.tail, vec!["alpha", "<redacted>"]);
        let written = std::fs::read_to_string(&log).unwrap();
        assert!(
            written.contains("alpha") && !written.contains("bravo"),
            "{written}"
        );
    }

    #[tokio::test]
    async fn kill_stops_a_running_process() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, args) = if cfg!(windows) {
            ("ping", vec!["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", vec!["30"])
        };
        let cmd = LaunchCommand {
            program: program.into(),
            args: args.into_iter().map(String::from).collect(),
            cwd: tmp.path().into(),
            classpath: vec![],
        };
        let (sink, _) = collect();
        let game = spawn(&cmd, Redactor::new(), sink, &tmp.path().join("l.log"))
            .await
            .unwrap();
        assert!(game.pid().is_some());
        let killer = game.killer();
        killer.kill();
        killer.kill(); // idempotent
        let exit = tokio::time::timeout(std::time::Duration::from_secs(15), game.wait())
            .await
            .expect("the killed process must exit")
            .unwrap();
        assert!(exit.killed);
        assert_ne!(exit.code, Some(0));
        assert_eq!(exit.crash, None, "a requested stop is never a crash");
    }

    struct FailingWriter;
    impl AsyncWrite for FailingWriter {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::task::Poll::Ready(Err(std::io::Error::other("disk full")))
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn log_write_failure_warns_once_and_keeps_streaming() {
        let tmp = tempfile::tempdir().unwrap();
        let (sink, seen) = collect();
        let game = spawn_with_log(
            &two_lines(tmp.path()),
            Redactor::new(),
            sink,
            Box::new(FailingWriter),
        )
        .unwrap();
        game.wait().await.unwrap();
        let warnings: Vec<ErrorCode> = seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::Warning(err) => Some(err.code),
                _ => None,
            })
            .collect();
        assert_eq!(warnings, vec![ErrorCode::LogWriteStopped]);
        assert_eq!(logs(&seen), vec!["alpha", "bravo"]);
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

    #[test]
    fn dry_run_resolves_relative_entries_from_the_game_dir() {
        // Java resolves relative classpath entries against its working directory (the game dir),
        // not the launcher's; the dry run must judge them the same way.
        let tmp = tempfile::tempdir().unwrap();
        let program = tmp.path().join("java");
        std::fs::write(&program, "").unwrap();
        std::fs::write(tmp.path().join("lib.jar"), "").unwrap();
        let cmd = LaunchCommand {
            program,
            args: vec![],
            cwd: tmp.path().into(),
            classpath: vec![PathBuf::from("lib.jar"), PathBuf::from("absent.jar")],
        };
        assert_eq!(missing_files(&cmd), vec![PathBuf::from("absent.jar")]);
    }
}
