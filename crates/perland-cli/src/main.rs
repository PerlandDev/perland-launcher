//! Developer harness for launcher-core: create instances, add offline accounts, install and launch.
use std::{path::PathBuf, process::ExitCode, sync::Arc};

use clap::{Parser, Subcommand, ValueEnum};
use launcher_core::{
    accounts::{AccountStore, BuildKind, Provider},
    download::Downloader,
    events::{Event, EventSink},
    install::Installer,
    instances::{GameSpec, InstanceStore, LoaderSpec},
    java,
    launch::{self, LaunchSpec, Redactor},
    meta::{loaders::LoaderKind, MetaClient},
    paths::LauncherDirs,
    platform::Platform,
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
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CliLoader {
    Fabric,
    Quilt,
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
        Cmd::Launch { instance, dry_run } => {
            launch_instance(&dirs, &store, &accounts, &instance, dry_run).await?
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
) -> Result<(), LauncherError> {
    let instance = store.load(id).await?;
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
    let http = launcher_core::http::client(env!("CARGO_PKG_VERSION"))?;
    let platform = Platform::current();
    let installer = Installer {
        dirs: dirs.clone(),
        meta: MetaClient::new(http.clone(), dirs.meta()),
        downloader: Downloader::new(http),
        platform,
    };
    let sink: EventSink = Arc::new(|event| match event {
        Event::Progress {
            task, done, total, ..
        } => eprint!("\r{task}: {done}/{total}    "),
        Event::Log { line } => println!("{line}"),
        Event::StateChanged { .. } => {}
    });

    let loader = instance
        .game
        .loader
        .as_ref()
        .map(|l| (l.kind, l.version.as_str()));
    let resolved = installer.resolve(&instance.game.minecraft, loader).await?;
    installer.install(&resolved, &sink).await?;
    eprintln!();
    let java_path = match &instance.java.path {
        Some(path) => path.clone(),
        None => {
            let component = java::component_for(&resolved.json);
            java::ensure_runtime(
                &installer.meta,
                &installer.downloader,
                &dirs.java(),
                platform,
                &component,
                &sink,
            )
            .await?
        }
    };
    eprintln!();

    let game_dir = store.game_dir(&instance.id);
    let cmd = launch::build_command(&LaunchSpec {
        dirs,
        resolved: &resolved,
        game_dir: &game_dir,
        java: &java_path,
        session: &session,
        memory_mb: instance.java.memory_mb,
        platform,
        launcher_version: env!("CARGO_PKG_VERSION"),
    })?;
    let redactor = Redactor::for_current_user().with_secret(session.access_token.clone());

    if dry_run {
        println!(
            "{}",
            redactor.redact(&format!("{} {}", cmd.program.display(), cmd.args.join(" ")))
        );
        let missing = launch::missing_files(&cmd);
        if missing.is_empty() {
            eprintln!(
                "dry run OK: java and {} classpath entries present",
                cmd.classpath.len()
            );
            return Ok(());
        }
        for path in &missing {
            eprintln!("missing: {}", path.display());
        }
        return Err(LauncherError::loader(
            ErrorCode::GameFilesMissing,
            format!("{} required files are missing", missing.len()),
        ));
    }

    let exit = launch::run(
        &cmd,
        &redactor,
        &sink,
        &dirs.logs().join(format!("{}-latest.log", instance.id)),
    )
    .await?;
    eprintln!("game exited with {:?}", exit.code);
    Ok(())
}
