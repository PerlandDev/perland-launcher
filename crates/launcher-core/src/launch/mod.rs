pub mod crash;
pub mod jvm_args;
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
    task::JoinHandle,
};

pub use crash::Crash;
pub use redact::Redactor;

use crate::{
    accounts::Session,
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink},
    fsutil::write_atomic,
    install::{classpath, ResolvedVersion},
    instances::ServerAddress,
    meta::{
        mojang::{ArgValue, VersionJson, LEGACY_JVM_ARGS},
        rules::{resolve_args, RuleContext},
    },
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
    /// The player's own Java arguments, already parsed (see [`jvm_args::parse`]).
    pub jvm_args: Vec<String>,
    pub platform: Platform,
    pub launcher_version: &'a str,
    /// Join this server once the game starts: quick play where the version supports it,
    /// `--server`/`--port` elsewhere.
    pub join: Option<&'a ServerAddress>,
    /// Open this world (a folder under `saves/`) once the game starts: quick play only. A
    /// version without it refuses, rather than open the title screen as if nothing was asked.
    pub world: Option<&'a str>,
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

/// Marks a line that was cut at `MAX_LINE_BYTES`.
const TRUNCATED_MARKER: &str = " …[truncated]";
/// Output lines waiting for the sink, across both streams.
const LINE_QUEUE: usize = 1024;
/// How long to wait for output to drain once the game has exited.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExit {
    pub code: Option<i32>,
    /// True when the launcher stopped the game on request.
    pub killed: bool,
    /// The last `TAIL_LINES` redacted output lines.
    pub tail: Vec<String>,
    /// A likely cause, only for exits nobody asked for that weren't clean.
    pub crash: Option<Crash>,
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

/// The feature a version JSON declares when it takes `--quickPlayMultiplayer` (1.20 and later).
pub const QUICK_PLAY_MULTIPLAYER: &str = "is_quick_play_multiplayer";

/// The feature a version JSON declares when it takes `--quickPlaySingleplayer` (1.20 and later).
pub const QUICK_PLAY_SINGLEPLAYER: &str = "is_quick_play_singleplayer";

/// Does the version (with its loader merged in) declare `feature` in a game-argument rule?
pub fn declares_feature(json: &VersionJson, feature: &str) -> bool {
    json.arguments.as_ref().is_some_and(|a| {
        a.game.iter().any(|arg| match arg {
            ArgValue::Conditional { rules, .. } => rules
                .iter()
                .any(|r| r.features.as_ref().is_some_and(|f| f.contains_key(feature))),
            ArgValue::Plain(_) => false,
        })
    })
}

pub fn build_command(spec: &LaunchSpec) -> Result<LaunchCommand> {
    let json = &spec.resolved.json;
    let quick_play = spec.join.is_some() && declares_feature(json, QUICK_PLAY_MULTIPLAYER);
    if spec.world.is_some() && !declares_feature(json, QUICK_PLAY_SINGLEPLAYER) {
        return Err(LauncherError::game(
            ErrorCode::WorldContinueUnsupported,
            "This Minecraft version can't open a world straight away",
        )
        .with_fix("Press Play and pick the world in Singleplayer")
        .with_param("version", &json.id));
    }
    let mut ctx = RuleContext::new(spec.platform);
    if quick_play {
        ctx = ctx.with_feature(QUICK_PLAY_MULTIPLAYER);
    }
    if spec.world.is_some() {
        ctx = ctx.with_feature(QUICK_PLAY_SINGLEPLAYER);
    }
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
    let mut vars: HashMap<&str, String> = HashMap::from([
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
    if let (true, Some(address)) = (quick_play, spec.join) {
        vars.insert("quickPlayMultiplayer", address.display());
    }
    if let Some(world) = spec.world {
        vars.insert("quickPlaySingleplayer", world.to_string());
    }

    // The player's arguments come before the version's, so when a property repeats, the
    // launcher's own (library path, brand, log4j) is the last and wins.
    let mut args = vec![format!("-Xmx{}M", spec.memory_mb)];
    args.extend(spec.jvm_args.iter().cloned());
    match &json.arguments {
        Some(a) if !a.jvm.is_empty() => args.extend(
            resolve_args(&a.jvm, &ctx)
                .iter()
                .map(|t| substitute(t, &vars)),
        ),
        _ => args.extend(LEGACY_JVM_ARGS.iter().map(|t| substitute(t, &vars))),
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
    if let (false, Some(address)) = (quick_play, spec.join) {
        // 1.13–1.19 and legacy `minecraftArguments`: the older flags every version still reads.
        args.extend([
            "--server".to_string(),
            address.host.clone(),
            "--port".to_string(),
            address.port.to_string(),
        ]);
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

/// Longest command line started directly. Windows refuses anything over 32,767 characters; past
/// this, the JVM arguments move into a Java argument file.
pub const ARGFILE_THRESHOLD: usize = 30_000;

/// About how long the OS command line is: the program and every argument, quoted and spaced.
pub fn command_line_len(cmd: &LaunchCommand) -> usize {
    cmd.program.as_os_str().len() + 2 + cmd.args.iter().map(|a| a.len() + 3).sum::<usize>()
}

/// Java reads `@argfile`s from version 9 on; an unknown version gets the plain command line.
pub fn wants_argfile(cmd: &LaunchCommand, java_major: Option<u32>) -> bool {
    command_line_len(cmd) > ARGFILE_THRESHOLD && java_major.is_some_and(|m| m >= 9)
}

/// A Java `@argfile` body: one argument per line, each in double quotes, with `\` and `"`
/// escaped (inside quotes Java treats `\` as an escape; outside them `#` starts a comment).
pub fn argfile_text(args: &[String]) -> String {
    let mut out = String::new();
    for arg in args {
        out.push('"');
        for c in arg.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                _ => out.push(c),
            }
        }
        out.push_str("\"\n");
    }
    out
}

/// The bytes Java reads an argfile as. Java's launcher reads `@argfiles` in the system ANSI code
/// page on Windows, so they are encoded in it (or as UTF-8 when the system code page is UTF-8);
/// elsewhere it is UTF-8. `None` when some character has no form in that code page, or when its
/// form holds a stray `\` byte (a trail byte of a multi-byte character, as in Shift-JIS), which
/// Java would read as an escape.
pub fn encode_argfile(text: &str) -> Option<Vec<u8>> {
    #[cfg(windows)]
    let bytes = ansi_bytes(text)?;
    #[cfg(not(windows))]
    let bytes = text.as_bytes().to_vec();
    backslashes_survive(text, &bytes).then_some(bytes)
}

/// Whether the encoded `bytes` hold exactly the `\` bytes the `text` holds. `argfile_text` escapes
/// every real backslash, so an extra 0x5C can only be a trail byte of a multi-byte character, and
/// Java's parser would take it for an escape and mangle the path.
fn backslashes_survive(text: &str, bytes: &[u8]) -> bool {
    text.matches('\\').count() == bytes.iter().filter(|&&b| b == b'\\').count()
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn ansi_bytes(text: &str) -> Option<Vec<u8>> {
    use windows_sys::Win32::Globalization::{
        GetACP, WideCharToMultiByte, CP_ACP, WC_NO_BEST_FIT_CHARS,
    };
    // SAFETY: GetACP takes no arguments and only reads system state.
    if unsafe { GetACP() } == 65001 {
        return Some(text.as_bytes().to_vec());
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return Some(Vec::new());
    }
    let mut lossy = 0;
    // SAFETY: `wide` is a valid UTF-16 buffer of the given length; a null output buffer with
    // size 0 asks only for the needed size; `lossy` is a valid out pointer.
    let needed = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            WC_NO_BEST_FIT_CHARS,
            wide.as_ptr(),
            wide.len() as i32,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            &mut lossy,
        )
    };
    if needed <= 0 || lossy != 0 {
        return None;
    }
    let mut out = vec![0u8; needed as usize];
    // SAFETY: `out` has exactly `needed` bytes, as measured above.
    let written = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            WC_NO_BEST_FIT_CHARS,
            wide.as_ptr(),
            wide.len() as i32,
            out.as_mut_ptr(),
            needed,
            std::ptr::null(),
            &mut lossy,
        )
    };
    (written == needed && lossy == 0).then_some(out)
}

/// Move the JVM arguments (everything before `main_class`) into the argfile at `path`. The game
/// arguments, which carry the access token, stay on the command line. If any JVM argument holds
/// `secret`, or the system code page can't hold some character of them (see [`encode_argfile`]),
/// nothing is written and the command comes back unchanged.
pub async fn with_argfile(
    cmd: &LaunchCommand,
    main_class: &str,
    path: &Path,
    secret: &str,
) -> Result<LaunchCommand> {
    let Some(split) = cmd.args.iter().position(|a| a == main_class) else {
        return Ok(cmd.clone());
    };
    let jvm = &cmd.args[..split];
    if secret.len() >= 4 && jvm.iter().any(|a| a.contains(secret)) {
        return Ok(cmd.clone());
    }
    // A path the system code page can't hold: keep the plain command line (below Windows'
    // 32k limit it still starts; above it, the spawn fails with the usual start error).
    let Some(bytes) = encode_argfile(&argfile_text(jvm)) else {
        return Ok(cmd.clone());
    };
    write_atomic(path, &bytes).await?;
    let mut args = vec![format!("@{}", path.display())];
    args.extend_from_slice(&cmd.args[split..]);
    Ok(LaunchCommand {
        args,
        ..cmd.clone()
    })
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
    line.push_str(TRUNCATED_MARKER);
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

async fn read_lines(reader: Box<dyn AsyncRead + Unpin + Send>, tx: mpsc::Sender<String>) {
    // Read raw bytes: the game may write in a non-UTF-8 charset, which must not end capture.
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES).await {
            Ok((true, truncated)) => {
                if tx.send(decode_line(&buf, truncated)).await.is_err() {
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
    mut rx: mpsc::Receiver<String>,
    redactor: Redactor,
    sink: EventSink,
    mut log: LogWriter,
    tail: Arc<Mutex<Tail>>,
) {
    let mut log_ok = true;
    while let Some(line) = rx.recv().await {
        let clean = match line.strip_suffix(TRUNCATED_MARKER) {
            // The cut may have gone through a secret, which would leave only its first part.
            Some(body) => format!("{}{TRUNCATED_MARKER}", redactor.redact_cut(body)),
            None => redactor.redact(&line),
        };
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

/// Let the output drain after the game exits. A grandchild holding the pipes open must not hang
/// the exit, and must not leave tasks running that keep logging after it: on timeout the pump and
/// the readers are stopped, not just abandoned.
async fn drain_output(mut pump: JoinHandle<()>, readers: Vec<JoinHandle<()>>, limit: Duration) {
    if tokio::time::timeout(limit, &mut pump).await.is_err() {
        pump.abort();
        let _ = pump.await;
    }
    for reader in readers {
        reader.abort();
        let _ = reader.await;
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
    // Crash reports and logs older than this belong to an earlier run.
    let started = std::time::SystemTime::now();
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
    // Bounded: when the sink can't keep up the readers wait, and the pipe backs up to the game.
    let (line_tx, line_rx) = mpsc::channel::<String>(LINE_QUEUE);
    let streams: [Box<dyn AsyncRead + Unpin + Send>; 2] = [Box::new(stdout), Box::new(stderr)];
    let readers: Vec<JoinHandle<()>> = streams
        .into_iter()
        .map(|stream| tokio::spawn(read_lines(stream, line_tx.clone())))
        .collect();
    drop(line_tx);
    let tail = Arc::new(Mutex::new(Tail::default()));
    let (crash_redactor, game_dir) = (redactor.clone(), cmd.cwd.clone());
    let pump = tokio::spawn(pump_lines(line_rx, redactor, sink, log, tail.clone()));
    let (kill_tx, kill_rx) = watch::channel(false);
    let (done_tx, done_rx) = oneshot::channel();
    tokio::spawn(async move {
        let result = wait_for_exit(child, kill_rx).await;
        drain_output(pump, readers, DRAIN_TIMEOUT).await;
        let tail = tail.lock().unwrap_or_else(|e| e.into_inner()).lines();
        let exit = match result {
            Ok((code, killed)) => {
                let crash = if !killed && code != Some(0) {
                    crash::diagnose(&game_dir, pid, started, &tail, &crash_redactor).await
                } else {
                    None
                };
                Ok(GameExit {
                    code,
                    killed,
                    tail,
                    crash,
                })
            }
            Err(e) => Err(e),
        };
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
        instances::ServerAddress,
        meta::{merge::merge, mojang::ArgValue},
        test_support::{forge_resolved, version, LINUX_X64, WINDOWS_X64},
    };

    fn build(resolved: &ResolvedVersion, platform: Platform) -> LaunchCommand {
        build_joining(resolved, platform, None)
    }

    fn build_joining(
        resolved: &ResolvedVersion,
        platform: Platform,
        join: Option<&ServerAddress>,
    ) -> LaunchCommand {
        build_with(resolved, platform, join, Vec::new())
    }

    fn build_with(
        resolved: &ResolvedVersion,
        platform: Platform,
        join: Option<&ServerAddress>,
        jvm_args: Vec<String>,
    ) -> LaunchCommand {
        let dirs = LauncherDirs::new("/pl");
        let session = offline_account("Steve").unwrap().offline_session();
        build_command(&LaunchSpec {
            dirs: &dirs,
            resolved,
            game_dir: Path::new("/pl/instances/x/minecraft"),
            java: Path::new("/pl/java/x/bin/java"),
            session: &session,
            memory_mb: 4096,
            jvm_args,
            platform,
            launcher_version: "0.1.0",
            join,
            world: None,
        })
        .unwrap()
    }

    fn server() -> ServerAddress {
        ServerAddress::new("play.example.com", 25566).unwrap()
    }

    fn build_world(resolved: &ResolvedVersion, world: &str) -> Result<LaunchCommand> {
        let dirs = LauncherDirs::new("/pl");
        let session = offline_account("Steve").unwrap().offline_session();
        build_command(&LaunchSpec {
            dirs: &dirs,
            resolved,
            game_dir: Path::new("/pl/instances/x/minecraft"),
            java: Path::new("/pl/java/x/bin/java"),
            session: &session,
            memory_mb: 4096,
            jvm_args: Vec::new(),
            platform: WINDOWS_X64,
            launcher_version: "0.1.0",
            join: None,
            world: Some(world),
        })
    }

    #[test]
    fn quick_play_opens_a_world_on_1_20_1() {
        let cmd = build_world(&vanilla("1.20.1.json", "1.20.1"), "Mundo de Ana").unwrap();
        assert_eq!(
            arg_after(&cmd.args, "--quickPlaySingleplayer"),
            "Mundo de Ana"
        );
        assert!(!cmd.args.contains(&"--quickPlayMultiplayer".to_string()));
        assert!(!cmd.args.contains(&"--server".to_string()));
        assert!(!cmd.args.contains(&"--quickPlayPath".to_string()));
    }

    #[test]
    fn a_world_on_a_version_without_quick_play_is_refused() {
        let Err(err) = build_world(&vanilla("1.12.2.json", "1.12.2"), "Mundo") else {
            panic!("1.12.2 can't open a world straight away");
        };
        assert_eq!(err.code, ErrorCode::WorldContinueUnsupported);
    }

    fn vanilla(file: &str, id: &str) -> ResolvedVersion {
        ResolvedVersion {
            vanilla_id: id.into(),
            json: version(file),
            forge: None,
        }
    }

    #[test]
    fn quick_play_joins_on_1_20_1() {
        let cmd = build_joining(
            &vanilla("1.20.1.json", "1.20.1"),
            WINDOWS_X64,
            Some(&server()),
        );
        assert_eq!(
            arg_after(&cmd.args, "--quickPlayMultiplayer"),
            "play.example.com:25566"
        );
        assert!(!cmd.args.contains(&"--server".to_string()));
        assert!(
            !cmd.args.contains(&"--quickPlayPath".to_string()),
            "only the multiplayer feature is on"
        );
        assert!(!cmd.args.iter().any(|a| a.contains("${")), "{:?}", cmd.args);
    }

    #[test]
    fn quick_play_brackets_an_ipv6_address() {
        let v6 = ServerAddress::new("2001:db8::1", 25565).unwrap();
        let cmd = build_joining(&vanilla("1.20.1.json", "1.20.1"), WINDOWS_X64, Some(&v6));
        assert_eq!(
            arg_after(&cmd.args, "--quickPlayMultiplayer"),
            "[2001:db8::1]:25565"
        );
    }

    #[test]
    fn legacy_versions_join_with_server_and_port() {
        let cmd = build_joining(
            &vanilla("1.12.2.json", "1.12.2"),
            LINUX_X64,
            Some(&server()),
        );
        assert_eq!(arg_after(&cmd.args, "--server"), "play.example.com");
        assert_eq!(arg_after(&cmd.args, "--port"), "25566");
        assert!(!cmd.args.iter().any(|a| a.starts_with("--quickPlay")));
        let forge = ResolvedVersion {
            vanilla_id: "1.12.2".into(),
            json: merge(
                version("1.12.2.json"),
                version("forge-1.12.2-14.23.5.2860-version.json"),
            ),
            forge: None,
        };
        let cmd = build_joining(&forge, LINUX_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--server"), "play.example.com");
    }

    #[test]
    fn modern_versions_without_quick_play_join_with_server_and_port() {
        let mut json = version("1.20.1.json");
        json.arguments.as_mut().unwrap().game.retain(|arg| {
            !matches!(arg, ArgValue::Conditional { rules, .. }
                if rules.iter().any(|r| r.features.as_ref().is_some_and(|f| f.keys().any(|k| k.contains("quick_play")))))
        });
        assert!(!declares_feature(&json, QUICK_PLAY_MULTIPLAYER));
        let resolved = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json,
            forge: None,
        };
        let cmd = build_joining(&resolved, WINDOWS_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--server"), "play.example.com");
        assert_eq!(arg_after(&cmd.args, "--port"), "25566");
        assert!(!cmd.args.contains(&"--quickPlayMultiplayer".to_string()));
    }

    #[test]
    fn loaders_inherit_quick_play_through_the_merge() {
        let fabric = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: merge(
                version("1.20.1.json"),
                version("fabric-1.20.1-0.16.14.json"),
            ),
            forge: None,
        };
        let cmd = build_joining(&fabric, WINDOWS_X64, Some(&server()));
        assert_eq!(
            arg_after(&cmd.args, "--quickPlayMultiplayer"),
            "play.example.com:25566"
        );
        let cmd = build_joining(&forge_resolved(), WINDOWS_X64, Some(&server()));
        assert_eq!(
            arg_after(&cmd.args, "--quickPlayMultiplayer"),
            "play.example.com:25566"
        );
    }

    #[test]
    fn without_a_server_nothing_is_added() {
        for resolved in [
            vanilla("1.20.1.json", "1.20.1"),
            vanilla("1.12.2.json", "1.12.2"),
        ] {
            let cmd = build(&resolved, WINDOWS_X64);
            assert!(!cmd.args.contains(&"--server".to_string()));
            assert!(!cmd.args.iter().any(|a| a.starts_with("--quickPlay")));
        }
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
            forge: None,
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
            forge: None,
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
            forge: None,
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
    async fn a_secret_straddling_the_line_cap_does_not_leak() {
        let secret = "tok-0123456789abcdef";
        let mut line = vec![b'x'; MAX_LINE_BYTES - 7];
        line.extend_from_slice(secret.as_bytes()); // the cap cuts after "tok-012"
        line.push(b'\n');
        let (tx, rx) = mpsc::channel(4);
        read_lines(Box::new(std::io::Cursor::new(line)), tx).await;
        let (sink, seen) = collect();
        let tail = Arc::new(Mutex::new(Tail::default()));
        pump_lines(
            rx,
            Redactor::new().with_secret(secret),
            sink,
            Box::new(tokio::io::sink()),
            tail,
        )
        .await;
        let shown = logs(&seen).remove(0);
        assert!(!shown.contains("tok-"), "{shown:?}");
        assert!(shown.ends_with("<redacted> …[truncated]"), "{shown:?}");
    }

    #[tokio::test]
    async fn output_backs_up_instead_of_growing_without_bound() {
        let lines: Vec<u8> = (0..10)
            .flat_map(|i| format!("line {i}\n").into_bytes())
            .collect();
        let (tx, mut rx) = mpsc::channel(2);
        let reader = tokio::spawn(read_lines(Box::new(std::io::Cursor::new(lines)), tx));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!reader.is_finished(), "the reader must wait for room");
        let mut got = Vec::new();
        while let Some(line) = rx.recv().await {
            got.push(line);
        }
        reader.await.unwrap();
        assert_eq!(got.len(), 10);
        assert_eq!(got[9], "line 9");
    }

    #[tokio::test]
    async fn drain_stops_tasks_that_outlive_the_timeout() {
        struct Flag(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Flag {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stuck = |flag: Flag| {
            tokio::spawn(async move {
                let _flag = flag;
                std::future::pending::<()>().await;
            })
        };
        let pump = stuck(Flag(stopped.clone()));
        let other = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let readers = vec![stuck(Flag(other.clone()))];
        drain_output(pump, readers, Duration::from_millis(50)).await;
        assert!(stopped.load(std::sync::atomic::Ordering::SeqCst), "pump");
        assert!(other.load(std::sync::atomic::Ordering::SeqCst), "reader");
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

    #[test]
    fn forge_ignore_list_names_the_jar_on_the_classpath() {
        let cmd = build(&crate::test_support::forge_resolved(), WINDOWS_X64);
        let jar = cmd
            .classpath
            .last()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let ignore = cmd
            .args
            .iter()
            .find(|a| a.starts_with("-DignoreList="))
            .unwrap();
        assert!(
            ignore.ends_with(&format!(",{jar}")),
            "{ignore} must name {jar}"
        );
        assert_eq!(arg_after(&cmd.args, "--launchTarget"), "forgeclient");
        assert_eq!(arg_after(&cmd.args, "--username"), "Steve");
        assert!(!cmd.args.iter().any(|x| x.contains("${")), "{:?}", cmd.args);
    }

    fn long_command() -> LaunchCommand {
        LaunchCommand {
            program: "/j/bin/java".into(),
            args: vec![
                "-Xmx1M".into(),
                "-cp".into(),
                "x".repeat(ARGFILE_THRESHOLD),
                "net.minecraft.client.main.Main".into(),
                "--accessToken".into(),
                "tok-0123456789".into(),
            ],
            cwd: "/pl/instances/x/minecraft".into(),
            classpath: vec![],
        }
    }

    #[test]
    fn player_jvm_args_follow_memory_and_precede_the_version_args() {
        let resolved = vanilla("1.20.1.json", "1.20.1");
        let cmd = build_with(
            &resolved,
            LINUX_X64,
            None,
            vec!["-XX:+UseG1GC".into(), "-Dperland.x=1".into()],
        );
        let a = &cmd.args;
        assert_eq!(a[0], "-Xmx4096M");
        assert_eq!(a[1..3], ["-XX:+UseG1GC", "-Dperland.x=1"]);
        let at = |x: &str| a.iter().position(|v| v == x).unwrap();
        assert!(at("-Dminecraft.launcher.brand=perland-launcher") > 2);
        assert!(at("-cp") > 2);
        assert!(at("-cp") < at("net.minecraft.client.main.Main"));
    }

    #[tokio::test]
    async fn player_jvm_args_go_into_the_argfile() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("jvm-args.txt");
        let resolved = vanilla("1.20.1.json", "1.20.1");
        let mut cmd = build_with(&resolved, LINUX_X64, None, vec![r"-Ddir=C:\A B\c".into()]);
        let at = cmd.args.iter().position(|a| a == "-cp").unwrap();
        cmd.args[at + 1] = "x".repeat(ARGFILE_THRESHOLD);
        let out = with_argfile(
            &cmd,
            "net.minecraft.client.main.Main",
            &file,
            "tok-0123456789",
        )
        .await
        .unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains(r#""-Ddir=C:\\A B\\c""#), "{written}");
        let main = cmd
            .args
            .iter()
            .position(|a| a == "net.minecraft.client.main.Main")
            .unwrap();
        let mut expected = vec![format!("@{}", file.display())];
        expected.extend_from_slice(&cmd.args[main..]);
        assert_eq!(out.args, expected);
    }

    #[test]
    fn argfile_only_for_long_lines_on_java_9_and_newer() {
        let long = long_command();
        assert!(command_line_len(&long) > ARGFILE_THRESHOLD);
        assert!(wants_argfile(&long, Some(17)));
        assert!(!wants_argfile(&long, Some(8)));
        assert!(!wants_argfile(&long, None));
        let mut short = long;
        short.args[2] = "a.jar".into();
        assert!(!wants_argfile(&short, Some(17)));
    }

    #[test]
    fn argfile_text_quotes_every_argument() {
        let text = argfile_text(&[
            r#"C:\Program Files\x y#z"q"#.to_string(),
            "plain".to_string(),
        ]);
        assert_eq!(text, "\"C:\\\\Program Files\\\\x y#z\\\"q\"\n\"plain\"\n");
    }

    #[tokio::test]
    async fn argfile_keeps_game_arguments_on_the_command_line() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(".perland/jvm-args.txt");
        let cmd = long_command();
        let out = with_argfile(
            &cmd,
            "net.minecraft.client.main.Main",
            &file,
            "tok-0123456789",
        )
        .await
        .unwrap();
        assert_eq!(
            out.args,
            vec![
                format!("@{}", file.display()),
                "net.minecraft.client.main.Main".to_string(),
                "--accessToken".to_string(),
                "tok-0123456789".to_string(),
            ]
        );
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.starts_with("\"-Xmx1M\"\n\"-cp\"\n"));
        assert!(
            !written.contains("tok-"),
            "the token never touches the disk"
        );
        assert_eq!(out.cwd, cmd.cwd);
    }

    #[tokio::test]
    async fn argfile_is_skipped_when_a_jvm_argument_holds_the_secret() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("jvm-args.txt");
        let mut cmd = long_command();
        cmd.args[0] = "-Dleak=tok-0123456789".into();
        let out = with_argfile(
            &cmd,
            "net.minecraft.client.main.Main",
            &file,
            "tok-0123456789",
        )
        .await
        .unwrap();
        assert_eq!(out.args, cmd.args);
        assert!(!file.exists());
    }

    #[test]
    fn java_reads_the_argfile_back() {
        // Skipped where no java is installed, or where it is older than Java 9 (no `@argfile`).
        let Ok(probe) = std::process::Command::new("java").arg("-version").output() else {
            return;
        };
        let version_text = String::from_utf8_lossy(&probe.stderr);
        if !crate::java::parse_java_version(&version_text).is_some_and(|m| m >= 9) {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("args.txt");
        let tricky = r#"C:\Program Files\x y#z"q"#;
        std::fs::write(
            &file,
            argfile_text(&[
                format!("-Dperland.tricky={tricky}"),
                "-Dperland.plain=ok".into(),
            ]),
        )
        .unwrap();
        let out = std::process::Command::new("java")
            .arg(format!("@{}", file.display()))
            .args(["-XshowSettings:properties", "-version"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(
            text.contains(&format!("perland.tricky = {tricky}")),
            "{text}"
        );
        assert!(text.contains("perland.plain = ok"), "{text}");
    }

    #[test]
    fn ascii_argfiles_encode_as_is() {
        assert_eq!(
            encode_argfile("\"-Xmx4G\"\n").unwrap(),
            b"\"-Xmx4G\"\n".to_vec()
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_write_utf8() {
        assert_eq!(
            encode_argfile("\"C:/Jugadores/Ñandú\"\n").unwrap(),
            "\"C:/Jugadores/Ñandú\"\n".as_bytes().to_vec()
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_skips_the_argfile_when_the_code_page_cant_hold_a_path() {
        if system_code_page_is_utf8() {
            return; // UTF-8 can hold any path, so there is nothing to reject
        }
        // No Windows ANSI code page holds both CJK and Cyrillic, and none maps emoji.
        assert_eq!(encode_argfile("\"C:/Users/玩家/Игрок/🎮\"\n"), None);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn an_unencodable_argfile_falls_back_to_the_command_line() {
        if system_code_page_is_utf8() {
            return; // UTF-8 can hold any path, so there is nothing to fall back from
        }
        let tmp = tempfile::tempdir().unwrap();
        let cmd = LaunchCommand {
            args: vec![
                "-Djava.library.path=C:/玩家/Игрок/🎮".into(),
                "net.minecraft.client.main.Main".into(),
                "--accessToken".into(),
                "tok-0123456789".into(),
            ],
            ..long_command()
        };
        let file = tmp.path().join("jvm-args.txt");
        let out = with_argfile(
            &cmd,
            "net.minecraft.client.main.Main",
            &file,
            "tok-0123456789",
        )
        .await
        .unwrap();
        assert_eq!(out.args, cmd.args);
        assert!(!file.exists());
    }

    #[test]
    fn backslashes_survive_only_when_no_trail_byte_is_a_backslash() {
        assert!(backslashes_survive("a\\b", b"a\\b"));
        assert!(backslashes_survive("plain", b"plain"));
        // Shift-JIS encodes 表 as 0x95 0x5C: the second byte reads as a backslash to Java.
        assert!(!backslashes_survive("表", &[0x95, 0x5C]));
    }

    /// True when the system code page is UTF-8, which can encode every character.
    #[cfg(windows)]
    fn system_code_page_is_utf8() -> bool {
        // SAFETY: GetACP takes no arguments and only reads system state.
        #[allow(unsafe_code)]
        unsafe {
            windows_sys::Win32::Globalization::GetACP() == 65001
        }
    }
}
