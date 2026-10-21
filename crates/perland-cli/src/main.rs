//! Developer harness for launcher-core: create instances, add offline accounts, install and launch.
use std::{path::PathBuf, process::ExitCode, sync::Arc};

use clap::{Parser, Subcommand, ValueEnum};
use launcher_core::{
    accounts::{AccountStore, BuildKind, Provider},
    download::Downloader,
    events::{Event, EventSink},
    install::Installer,
    instances::{GameSpec, InstanceStore, LoaderSpec, SourceRef},
    launch::Redactor,
    locks::InstanceLock,
    meta::{loaders::LoaderKind, MetaClient},
    paths::LauncherDirs,
    platform::Platform,
    play::{self, StartSpec},
    sources::{PackInput, UpdateCheck},
    sync::{
        lockfile::{additions, Managed},
        ManualFile, SyncOutcome, Syncer,
    },
    ErrorCode, LauncherError,
};

#[derive(Parser)]
#[command(
    name = "perland-cli",
    about = "Developer harness for the Perland launcher core"
)]
struct Cli {
    /// Data folder (defaults to the launcher's normal data folder)
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an instance and print its id
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        mc: String,
        #[arg(long, value_enum)]
        loader: Option<CliLoader>,
        #[arg(long)]
        loader_version: Option<String>,
    },
    /// List instances
    List,
    /// Add an offline account and make it active
    Offline { name: String },
    /// Install (if needed) and launch an instance with the active account
    Launch {
        #[arg(long)]
        instance: String,
        /// Build the command and check every file exists, without starting the game
        #[arg(long)]
        dry_run: bool,
        /// Don't update the pack first
        #[arg(long)]
        no_sync: bool,
    },
    /// Packs: add one as a new instance, check for updates, list the player's own mods
    Pack {
        #[command(subcommand)]
        cmd: PackCmd,
    },
    /// Bring a pack instance up to date (downloads only what changed)
    Sync {
        #[arg(long)]
        instance: String,
        /// Where to look for files that must be downloaded by hand (default: your Downloads folder)
        #[arg(long)]
        manual_dir: Option<PathBuf>,
    },
    /// Java installations
    Java {
        #[command(subcommand)]
        cmd: JavaCmd,
    },
}

#[derive(Subcommand)]
enum PackCmd {
    /// Create an instance from a pack link, a .mrpack file, or modrinth:<slug>[@version]; prints its id
    Add { input: String },
    /// Say whether an update is available
    Check {
        #[arg(long)]
        instance: String,
    },
    /// List the jars in mods/ that the pack doesn't manage
    Additions {
        #[arg(long)]
        instance: String,
    },
}

#[derive(Subcommand)]
enum JavaCmd {
    /// List the Java installations found on this computer
    List,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliLoader {
    Fabric,
    Quilt,
    Forge,
    #[value(name = "neoforge")]
    NeoForge,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error [{:?}]: {}", e.category, e.message);
            if let Some(fix) = &e.fix {
                eprintln!("  fix: {fix}");
            }
            if let Some(detail) = &e.detail {
                eprintln!("  detail: {detail}");
            }
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), LauncherError> {
    let root = cli
        .root
        .or_else(LauncherDirs::default_root)
        .ok_or_else(|| {
            LauncherError::disk(ErrorCode::DiskIo, "Couldn't find a data folder")
                .with_fix("Pass --root <folder>")
        })?;
    let dirs = LauncherDirs::new(root);
    let store = InstanceStore::new(dirs.instances());
    let accounts = AccountStore::new(dirs.root().join("accounts.json"));
    match cli.cmd {
        Cmd::Create {
            name,
            mc,
            loader,
            loader_version,
        } => {
            let loader = match (loader, loader_version) {
                (Some(kind), Some(version)) => Some(LoaderSpec {
                    kind: match kind {
                        CliLoader::Fabric => LoaderKind::Fabric,
                        CliLoader::Quilt => LoaderKind::Quilt,
                        CliLoader::Forge => LoaderKind::Forge,
                        CliLoader::NeoForge => LoaderKind::NeoForge,
                    },
                    version,
                }),
                (None, None) => None,
                _ => {
                    return Err(LauncherError::loader(
                        ErrorCode::VersionInvalid,
                        "Pass both --loader and --loader-version",
                    ))
                }
            };
            let instance = store
                .create(
                    &name,
                    GameSpec {
                        minecraft: mc,
                        loader,
                    },
                )
                .await?;
            println!("{}", instance.id);
        }
        Cmd::List => {
            for i in store.list().await? {
                println!("{}\t{}\t{}", i.id, i.name, i.game.minecraft);
            }
        }
        Cmd::Offline { name } => {
            let account = accounts.add_offline(&name, BuildKind::current()).await?;
            println!("{} {}", account.username, account.uuid);
        }
        Cmd::Pack {
            cmd: PackCmd::Add { input },
        } => {
            let syncer = syncer(&dirs)?;
            let (instance, desired) = syncer.add_pack(&store, &PackInput::parse(&input)?).await?;
            // If the sync fails, the player can still find the half-synced instance.
            eprintln!("created instance {}", instance.id);
            // A server's pack is resolved by its first sync (perland::open reads only the invite).
            let outcome = if matches!(instance.source, SourceRef::Perland { .. }) {
                syncer
                    .ensure(&store, &instance.id, &progress_sink())
                    .await?
                    .outcome
            } else {
                syncer
                    .sync(&store, &instance.id, &desired, &progress_sink())
                    .await?
            };
            // stdout carries only the id (scripts capture it), so the changelog goes to stderr.
            if report(&outcome, true).is_some() {
                eprintln!("then run: perland-cli sync --instance {}", instance.id);
            }
            println!("{}", instance.id);
        }
        Cmd::Pack {
            cmd: PackCmd::Check { instance },
        } => {
            let instance = store.load(&instance).await?;
            match syncer(&dirs)?.check(&store, &instance).await {
                UpdateCheck::UpToDate => println!("up to date"),
                UpdateCheck::Available { version, .. } => println!("update available: {version}"),
                UpdateCheck::Unavailable(e) => println!("couldn't check: {}", e.message),
            }
        }
        Cmd::Pack {
            cmd: PackCmd::Additions { instance },
        } => {
            store.load(&instance).await?;
            let managed = Managed::load(&store.perland_dir(&instance)).await;
            for name in additions(&store.game_dir(&instance), managed.as_ref()).await? {
                println!("{name}");
            }
        }
        Cmd::Sync {
            instance,
            manual_dir,
        } => {
            store.load(&instance).await?;
            sync_instance(&syncer(&dirs)?, &store, &instance, manual_dir).await?
        }
        Cmd::Launch {
            instance,
            dry_run,
            no_sync,
        } => launch_instance(&dirs, &store, &accounts, &instance, dry_run, no_sync).await?,
        Cmd::Java { cmd: JavaCmd::List } => {
            for java in launcher_core::java::detect_installed(Platform::current()).await {
                println!("{}\t{}", java.major, java.path.display());
            }
        }
    }
    Ok(())
}

async fn launch_instance(
    dirs: &LauncherDirs,
    store: &InstanceStore,
    accounts: &AccountStore,
    id: &str,
    dry_run: bool,
    no_sync: bool,
) -> Result<(), LauncherError> {
    let account = accounts.active().await?.ok_or_else(|| {
        LauncherError::auth(ErrorCode::NoAccount, "No account selected")
            .with_fix("Run `perland-cli offline <name>` first")
    })?;
    if account.provider != Provider::Offline {
        return Err(LauncherError::auth(
            ErrorCode::MicrosoftUnavailable,
            "Microsoft accounts arrive in a later build",
        ));
    }
    let session = account.offline_session();
    let mut instance = store.load(id).await?;
    if instance.source != SourceRef::None && !no_sync {
        sync_instance(&syncer(dirs)?, store, id, None).await?;
        instance = store.load(id).await?; // a pack update can change the game version
    }
    // Held until the game exits: a sync from the app or another CLI must wait for it.
    let _lock = InstanceLock::try_acquire(&store.perland_dir(id))?;
    let (installer, _) = installer(dirs)?;
    let platform = installer.platform;
    let sink = progress_sink();

    let prepared = play::prepare(&installer, &instance, false, &sink).await?;
    eprintln!();
    let game_dir = store.game_dir(&instance.id);
    let spec = StartSpec {
        dirs,
        game_dir: &game_dir,
        instance: &instance,
        prepared: &prepared,
        session: &session,
        platform,
        launcher_version: env!("CARGO_PKG_VERSION"),
        join: None,
    };

    if dry_run {
        let cmd = play::command(&spec)?;
        let redactor = Redactor::for_current_user().with_secret(session.access_token.clone());
        println!(
            "{}",
            redactor.redact(&format!("{} {}", cmd.program.display(), cmd.args.join(" ")))
        );
        play::check_files(&cmd)?;
        let len = launcher_core::launch::command_line_len(&cmd);
        eprintln!(
            "command line: {len} characters{}",
            if len > launcher_core::launch::ARGFILE_THRESHOLD {
                " (a real launch puts the JVM arguments in an argfile)"
            } else {
                ""
            }
        );
        eprintln!(
            "dry run OK: java and {} classpath entries present",
            cmd.classpath.len()
        );
        return Ok(());
    }

    let game = play::start(&spec, sink).await?;
    let exit = game.wait().await?;
    eprintln!("game exited with {:?}", exit.code);
    Ok(())
}

fn installer(dirs: &LauncherDirs) -> Result<(Arc<Installer>, reqwest::Client), LauncherError> {
    let http = launcher_core::http::client(env!("CARGO_PKG_VERSION"))?;
    let installer = Installer {
        dirs: dirs.clone(),
        meta: MetaClient::new(http.clone(), dirs.meta()),
        downloader: Downloader::new(http.clone()),
        platform: Platform::current(),
    };
    Ok((Arc::new(installer), http))
}

fn syncer(dirs: &LauncherDirs) -> Result<Syncer, LauncherError> {
    let (installer, http) = installer(dirs)?;
    Ok(Syncer::for_installer(installer, http))
}

fn progress_sink() -> EventSink {
    Arc::new(|event| match event {
        Event::Progress {
            task, done, total, ..
        } => eprint!("\r{}: {done}/{total}    ", task.key),
        Event::Log { line } => println!("{line}"),
        Event::Warning(e) => eprintln!("\nwarning: {}", e.message),
        Event::StateChanged { .. } => {}
    })
}

/// Print what a sync did: to stdout, or to stderr when stdout is reserved for something else.
/// Returns the files still to download by hand, if any.
fn report(outcome: &SyncOutcome, to_stderr: bool) -> Option<&[ManualFile]> {
    eprintln!();
    let say = |line: String| {
        if to_stderr {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    };
    match outcome {
        SyncOutcome::Done(r) if r.up_to_date => say("up to date".to_string()),
        SyncOutcome::Done(r) => {
            let c = &r.changes;
            say(format!(
                "{} → {}: +{} added · {} updated · {} removed{}",
                c.from_version.as_deref().unwrap_or("new"),
                c.to_version,
                c.added.len(),
                c.updated.len(),
                c.removed.len(),
                if r.game_changed {
                    " (new game version)"
                } else {
                    ""
                }
            ));
            for path in &r.kept_modified {
                say(format!("kept your version of {path}"));
            }
        }
        SyncOutcome::NeedsManual(files) => {
            eprintln!("{} files must be downloaded by hand:", files.len());
            for f in files {
                eprintln!("  {}: download {} from {}", f.name, f.file_name, f.page_url);
            }
            return Some(files);
        }
    }
    None
}

/// Sync `id` by its policy, picking up hand-downloaded files from `manual_dir` once.
async fn sync_instance(
    syncer: &Syncer,
    store: &InstanceStore,
    id: &str,
    manual_dir: Option<PathBuf>,
) -> Result<(), LauncherError> {
    let sink = progress_sink();
    let ensured = syncer.ensure(store, id, &sink).await?;
    if let Some(w) = &ensured.warning {
        eprintln!("warning: {}", w.message);
    }
    let Some(mut files) = report(&ensured.outcome, false).map(<[ManualFile]>::to_vec) else {
        return Ok(());
    };
    if let Some(dir) = manual_dir.or_else(launcher_core::sync::manual::downloads_dir) {
        if syncer.import_manual(&dir, &files).await? > 0 {
            let again = syncer.ensure(store, id, &sink).await?;
            match report(&again.outcome, false) {
                None => return Ok(()),
                Some(remaining) => files = remaining.to_vec(),
            }
        }
    }
    Err(LauncherError::pack(
        ErrorCode::ManualDownloadsNeeded,
        format!("{} files have to be downloaded by hand", files.len()),
    )
    .with_param("count", files.len())
    .with_fix("Download them from CurseForge, then run sync again"))
}
