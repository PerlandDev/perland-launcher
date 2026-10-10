//! Running a Forge or NeoForge installer's client-side processors: the Java tools that turn the
//! vanilla jar into the patched game Forge loads. Written from the installer's published
//! `install_profile.json` format. Every output with a published sha1 is checked.
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use crate::{
    download::Hashes,
    error::{ErrorCode, LauncherError, Result},
    fsutil::safe_join,
    meta::{
        forge::{
            invalid, parse_data_value, read_entry, replace_tokens, unsafe_value, DataValue,
            InstallProfile, Processor,
        },
        maven::MavenCoord,
    },
};

/// One processor may take minutes on a slow disk; past this it's stuck.
pub const PROCESSOR_TIMEOUT: Duration = Duration::from_secs(600);
/// Lines of a failed processor's output kept for the error details.
const OUTPUT_TAIL: usize = 30;

/// The paths the profile's built-in data keys stand for.
pub struct DataContext<'a> {
    pub libraries: &'a Path,
    pub installer: &'a Path,
    /// Where installer files named by data values (e.g. `data/client.lzma`) are extracted.
    pub extract_dir: &'a Path,
    pub minecraft_jar: &'a Path,
    pub minecraft: &'a str,
    pub root: &'a Path,
}

#[derive(Debug, Default)]
pub struct ProcessorData {
    pub values: HashMap<String, String>,
    /// Values that name a library file, so a plan can tell which arguments are files.
    pub artifacts: HashSet<String>,
    /// Installer entries to extract before the processors run: (entry, destination), sorted.
    pub extract: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessorCall {
    /// The processor jar first, then its classpath.
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    /// Files the processor must produce, with their expected sha1 (lowercase).
    pub outputs: Vec<(PathBuf, String)>,
    /// Every library file the call reads or writes; all of them exist after a good install.
    pub files: Vec<PathBuf>,
}

fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn coord_path(libraries: &Path, name: &str) -> Result<PathBuf> {
    let coord = MavenCoord::parse(name).ok_or_else(|| unsafe_value(name))?;
    safe_join(libraries, &coord.path()).ok_or_else(|| unsafe_value(name))
}

/// The profile's data with every value resolved for the client side, plus the built-in keys.
pub fn resolve_data(profile: &InstallProfile, ctx: &DataContext) -> Result<ProcessorData> {
    let mut data = ProcessorData::default();
    for (key, sided) in &profile.data {
        let value = match parse_data_value(&sided.client)? {
            DataValue::Artifact(coord) => {
                let path = safe_join(ctx.libraries, &coord.path())
                    .ok_or_else(|| unsafe_value(&sided.client))?;
                let path = path_string(&path);
                data.artifacts.insert(path.clone());
                path
            }
            DataValue::Literal(text) => text,
            DataValue::InstallerFile(entry) => {
                let dest =
                    safe_join(ctx.extract_dir, &entry).ok_or_else(|| unsafe_value(&entry))?;
                data.extract.push((entry, dest.clone()));
                path_string(&dest)
            }
        };
        data.values.insert(key.clone(), value);
    }
    for (key, value) in [
        ("SIDE", "client".to_string()),
        ("MINECRAFT_JAR", path_string(ctx.minecraft_jar)),
        ("MINECRAFT_VERSION", ctx.minecraft.to_string()),
        ("ROOT", path_string(ctx.root)),
        ("INSTALLER", path_string(ctx.installer)),
        ("LIBRARY_DIR", path_string(ctx.libraries)),
    ] {
        data.values.insert(key.to_string(), value);
    }
    data.extract.sort();
    Ok(data)
}

/// The client-side processors, in profile order, with every argument resolved.
pub fn plan_processors(
    profile: &InstallProfile,
    data: &ProcessorData,
    libraries: &Path,
) -> Result<Vec<ProcessorCall>> {
    profile
        .processors
        .iter()
        .filter(|p| p.runs_on_client())
        .map(|p| plan_one(p, data, libraries))
        .collect()
}

fn plan_one(p: &Processor, data: &ProcessorData, libraries: &Path) -> Result<ProcessorCall> {
    let mut classpath = vec![coord_path(libraries, &p.jar)?];
    for name in &p.classpath {
        classpath.push(coord_path(libraries, name)?);
    }
    let mut files = classpath.clone();
    let mut args = Vec::with_capacity(p.args.len());
    for arg in &p.args {
        let value = match arg.strip_prefix('[').and_then(|a| a.strip_suffix(']')) {
            Some(name) => {
                let path = coord_path(libraries, name)?;
                files.push(path.clone());
                path_string(&path)
            }
            None => {
                let value = replace_tokens(arg, &data.values)?;
                if data.artifacts.contains(&value) {
                    files.push(PathBuf::from(&value));
                }
                value
            }
        };
        args.push(value);
    }
    let mut outputs = Vec::with_capacity(p.outputs.len());
    for (key, sha) in &p.outputs {
        let path = output_path(key, data, libraries)?;
        let sha1 = replace_tokens(sha, &data.values)?.to_ascii_lowercase();
        if sha1.len() != 40 || !sha1.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(format!("bad output checksum {sha}")));
        }
        outputs.push((path, sha1));
    }
    Ok(ProcessorCall {
        classpath,
        args,
        outputs,
        files,
    })
}

/// Where an output key points. Outputs are checked and, when damaged, deleted, so the path must be a
/// plain file path inside the libraries folder: `..` and the like are refused after token substitution.
fn output_path(key: &str, data: &ProcessorData, libraries: &Path) -> Result<PathBuf> {
    if let Some(name) = key.strip_prefix('[').and_then(|k| k.strip_suffix(']')) {
        return coord_path(libraries, name);
    }
    let path = PathBuf::from(replace_tokens(key, &data.values)?);
    let inside = path.strip_prefix(libraries).is_ok_and(|rest| {
        rest.components().next().is_some()
            && rest.components().all(|c| matches!(c, Component::Normal(_)))
    });
    if inside {
        Ok(path)
    } else {
        Err(unsafe_value(key))
    }
}

/// `Main-Class` from a jar's manifest. Blocking.
pub fn main_class(jar: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)
        .map_err(|e| invalid(format!("{}: {e}", jar.display())))?;
    let manifest = read_entry(&mut archive, "META-INF/MANIFEST.MF", 1024 * 1024)?;
    String::from_utf8_lossy(&manifest)
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:"))
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| invalid(format!("{} has no Main-Class", jar.display())))
}

pub fn java_args(call: &ProcessorCall, main_class: &str, separator: &str) -> Vec<String> {
    let cp = call
        .classpath
        .iter()
        .map(|p| path_string(p))
        .collect::<Vec<_>>()
        .join(separator);
    let mut args = vec!["-cp".to_string(), cp, main_class.to_string()];
    args.extend(call.args.iter().cloned());
    args
}

async fn file_matches(path: &Path, sha1: &str) -> bool {
    let (path, hashes) = (path.to_path_buf(), Hashes::sha1(sha1));
    tokio::task::spawn_blocking(move || {
        path.is_file() && hashes.verify_file(&path).unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// True when the call has outputs and every one is on disk with its expected sha1, so the call
/// can be skipped. A call without outputs always runs.
pub async fn outputs_current(call: &ProcessorCall) -> bool {
    if call.outputs.is_empty() {
        return false;
    }
    for (path, sha1) in &call.outputs {
        if !file_matches(path, sha1).await {
            return false;
        }
    }
    true
}

pub async fn verify_outputs(call: &ProcessorCall) -> Result<()> {
    for (path, sha1) in &call.outputs {
        if !file_matches(path, sha1).await {
            let _ = tokio::fs::remove_file(path).await;
            return Err(LauncherError::integrity(
                ErrorCode::LoaderOutputCorrupt,
                "A file the mod loader built came out damaged",
            )
            .with_fix("Try again")
            .with_detail(format!("{} (expected sha1 {sha1})", path.display())));
        }
    }
    Ok(())
}

fn step_name(call: &ProcessorCall) -> String {
    call.classpath
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn step_failed(step: &str, detail: impl Display) -> LauncherError {
    LauncherError::loader(
        ErrorCode::LoaderProcessorFailed,
        "Setting up the mod loader failed",
    )
    .with_param("step", step)
    .with_fix("Try again. If it keeps happening, copy the details and ask for help.")
    .with_detail(detail)
}

/// Run one processor with `java` (`-cp <jar;classpath> <Main-Class> <args>`), then check its outputs.
pub async fn run_processor(
    java: &Path,
    call: &ProcessorCall,
    separator: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<()> {
    let step = step_name(call);
    let jar = call.classpath[0].clone();
    let main = tokio::task::spawn_blocking(move || main_class(&jar))
        .await
        .map_err(|e| step_failed(&step, e))?
        .map_err(|e| step_failed(&step, e.detail.as_deref().unwrap_or(&e.message)))?;
    let mut command = tokio::process::Command::new(java);
    command
        .args(java_args(call, &main, separator))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = match tokio::time::timeout(timeout, command.output()).await {
        Err(_) => {
            return Err(step_failed(
                &step,
                format!("still running after {} s", timeout.as_secs()),
            ))
        }
        Ok(Err(e)) => return Err(step_failed(&step, format!("couldn't start Java: {e}"))),
        Ok(Ok(output)) => output,
    };
    if !output.status.success() {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let lines: Vec<&str> = text.lines().collect();
        let tail = lines[lines.len().saturating_sub(OUTPUT_TAIL)..].join("\n");
        return Err(step_failed(
            &step,
            format!("exit code {:?}\n{tail}", output.status.code()),
        ));
    }
    verify_outputs(call).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        meta::forge::parse_profile,
        paths::LauncherDirs,
        test_support::{fixture, write_zip},
        ErrorCode,
    };

    const HELLO_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

    fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let i = args
            .iter()
            .position(|a| a == flag)
            .unwrap_or_else(|| panic!("{flag}"));
        &args[i + 1]
    }

    struct Paths {
        libs: PathBuf,
        installer: PathBuf,
        extract: PathBuf,
        mc_jar: PathBuf,
        root: PathBuf,
    }

    fn paths() -> Paths {
        let dirs = LauncherDirs::new("/pl");
        let libs = dirs.libraries();
        Paths {
            installer: libs
                .join("net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar"),
            extract: dirs.version_dir("1.20.1-forge-47.4.22").join(".installer"),
            mc_jar: dirs.client_jar("1.20.1"),
            root: dirs.root().to_path_buf(),
            libs,
        }
    }

    fn plan(profile_fixture: &str, minecraft: &str) -> (Paths, ProcessorData, Vec<ProcessorCall>) {
        let p = paths();
        let profile = parse_profile(fixture(profile_fixture).as_bytes()).unwrap();
        let data = resolve_data(
            &profile,
            &DataContext {
                libraries: &p.libs,
                installer: &p.installer,
                extract_dir: &p.extract,
                minecraft_jar: &p.mc_jar,
                minecraft,
                root: &p.root,
            },
        )
        .unwrap();
        let calls = plan_processors(&profile, &data, &p.libs).unwrap();
        (p, data, calls)
    }

    #[test]
    fn forge_client_processors_are_planned_with_real_paths() {
        let (p, data, calls) = plan("forge-1.20.1-47.4.22-install_profile.json", "1.20.1");
        let lib = |rel: &str| path_string(&p.libs.join(rel));
        assert_eq!(
            data.extract,
            vec![(
                "data/client.lzma".to_string(),
                p.extract.join("data/client.lzma")
            )]
        );
        assert_eq!(data.values["SIDE"], "client");
        assert_eq!(calls.len(), 6);
        // MCP_DATA reads the mcp_config zip named inline as [coordinate].
        assert_eq!(
            arg_after(&calls[0].args, "--input"),
            lib("de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip")
        );
        assert_eq!(arg_after(&calls[1].args, "--side"), "client");
        let split = &calls[3];
        assert_eq!(split.classpath.len(), 3);
        assert_eq!(arg_after(&split.args, "--input"), path_string(&p.mc_jar));
        assert_eq!(
            split.outputs,
            vec![
                (
                    p.libs.join("net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-extra.jar"),
                    "8c5a95cbce940cfdb304376ae9fea47968d02587".to_string()
                ),
                (
                    p.libs.join("net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-slim.jar"),
                    "de86b035d2da0f78940796bb95c39a932ed84834".to_string()
                ),
            ]
        );
        let patch = &calls[5];
        let patched = "net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-client.jar";
        assert_eq!(
            arg_after(&patch.args, "--apply"),
            path_string(&p.extract.join("data/client.lzma"))
        );
        assert_eq!(arg_after(&patch.args, "--output"), lib(patched));
        assert!(patch.files.contains(&p.libs.join(patched)));
        assert!(
            !calls.iter().flat_map(|c| &c.args).any(|a| a.contains('{')),
            "every token is resolved"
        );
    }

    #[test]
    fn neoforge_processors_have_no_output_checks() {
        let (_, _, calls) = plan("neoforge-21.1.257-install_profile.json", "1.21.1");
        assert_eq!(calls.len(), 6);
        // The processor jar plus its classpath entries.
        let lens: Vec<usize> = calls.iter().map(|c| c.classpath.len()).collect();
        assert_eq!(lens, [20, 20, 20, 5, 2, 2]);
        assert!(calls.iter().all(|c| c.outputs.is_empty()));
        assert!(calls[5].classpath[0].ends_with("binarypatcher-2.1.2-fatjar.jar"));
    }

    #[test]
    fn processor_paths_cannot_escape_the_libraries_folder() {
        let p = paths();
        let mut profile =
            parse_profile(fixture("forge-1.20.1-47.4.22-install_profile.json").as_bytes()).unwrap();
        let data = ProcessorData::default();
        profile.processors = vec![Processor {
            jar: "a:b:../../evil".into(),
            classpath: vec![],
            args: vec![],
            outputs: Default::default(),
            sides: None,
        }];
        assert_eq!(
            plan_processors(&profile, &data, &p.libs).unwrap_err().code,
            ErrorCode::UnsafeGameData
        );
        profile.processors[0].jar = "a:b:1".into();
        profile.processors[0].outputs =
            [("/etc/passwd".to_string(), format!("'{HELLO_SHA1}'"))].into();
        assert_eq!(
            plan_processors(&profile, &data, &p.libs).unwrap_err().code,
            ErrorCode::UnsafeGameData
        );
        // `..` after token substitution must not slip past a prefix check.
        let mut with_lib = ProcessorData::default();
        with_lib
            .values
            .insert("LIBRARY_DIR".into(), path_string(&p.libs));
        for key in [
            "{LIBRARY_DIR}/../evil",
            "{LIBRARY_DIR}/a/../../evil",
            "{LIBRARY_DIR}",
        ] {
            profile.processors[0].outputs = [(key.to_string(), format!("'{HELLO_SHA1}'"))].into();
            assert_eq!(
                plan_processors(&profile, &with_lib, &p.libs)
                    .unwrap_err()
                    .code,
                ErrorCode::UnsafeGameData,
                "{key}"
            );
        }
        // A bracketed coordinate is an allowed output key; one that escapes is not.
        profile.processors[0].outputs =
            [("[a.b:c:1:extra@jar]".to_string(), format!("'{HELLO_SHA1}'"))].into();
        let calls = plan_processors(&profile, &with_lib, &p.libs).unwrap();
        assert_eq!(
            calls[0].outputs,
            vec![(p.libs.join("a/b/c/1/c-1-extra.jar"), HELLO_SHA1.to_string())]
        );
        profile.processors[0].outputs =
            [("[a:b:../../evil]".to_string(), format!("'{HELLO_SHA1}'"))].into();
        assert_eq!(
            plan_processors(&profile, &with_lib, &p.libs)
                .unwrap_err()
                .code,
            ErrorCode::UnsafeGameData
        );
    }

    #[test]
    fn main_class_comes_from_the_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("tool.jar");
        write_zip(
            &jar,
            &[(
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.jarsplitter.ConsoleTool\r\n\r\n",
            )],
        );
        assert_eq!(
            main_class(&jar).unwrap(),
            "net.minecraftforge.jarsplitter.ConsoleTool"
        );
        let bare = tmp.path().join("bare.jar");
        write_zip(
            &bare,
            &[("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\r\n")],
        );
        assert_eq!(
            main_class(&bare).unwrap_err().code,
            ErrorCode::LoaderInstallerInvalid
        );
    }

    #[tokio::test]
    async fn outputs_are_checked_and_bad_ones_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("slim.jar");
        std::fs::write(&out, "hello").unwrap();
        let mut call = ProcessorCall {
            classpath: vec![tmp.path().join("tool.jar")],
            args: vec![],
            outputs: vec![(out.clone(), HELLO_SHA1.to_string())],
            files: vec![],
        };
        assert!(outputs_current(&call).await);
        verify_outputs(&call).await.unwrap();
        call.outputs[0].1 = "0".repeat(40);
        assert!(!outputs_current(&call).await);
        assert_eq!(
            verify_outputs(&call).await.unwrap_err().code,
            ErrorCode::LoaderOutputCorrupt
        );
        assert!(!out.exists(), "a damaged output is removed");
        call.outputs.clear();
        assert!(
            !outputs_current(&call).await,
            "no outputs means it must run"
        );
    }

    #[tokio::test]
    async fn a_failing_processor_reports_its_output() {
        // Skipped where no java is installed.
        if std::process::Command::new("java")
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("tool.jar");
        write_zip(
            &jar,
            &[(
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\nMain-Class: perland.Nope\n",
            )],
        );
        let call = ProcessorCall {
            classpath: vec![jar],
            args: vec!["--task".into(), "X".into()],
            outputs: vec![],
            files: vec![],
        };
        let sep = if cfg!(windows) { ";" } else { ":" };
        let err = run_processor(
            Path::new("java"),
            &call,
            sep,
            tmp.path(),
            Duration::from_secs(60),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::LoaderProcessorFailed);
        assert_eq!(err.params["step"], "tool.jar");
        assert!(err.detail.unwrap().contains("perland.Nope"));
    }

    #[tokio::test]
    async fn a_missing_processor_jar_is_a_step_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let call = ProcessorCall {
            classpath: vec![tmp.path().join("gone.jar")],
            args: vec![],
            outputs: vec![],
            files: vec![],
        };
        let err = run_processor(
            Path::new("java"),
            &call,
            ";",
            tmp.path(),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::LoaderProcessorFailed);
        assert_eq!(err.params["step"], "gone.jar");
        assert!(err.detail.is_some());
    }
}
