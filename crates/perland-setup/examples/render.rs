//! Writes every setup screen in both languages to PNG, for review and the PR's screenshots:
//! `cargo run -p perland-setup --example render` → `target/setup-renders/`.

use perland_setup::draw::{render, Paths, Ui};
use perland_setup::model::{Action, ErrorCode, Offer, Screen, Step};
use perland_setup::options::FolderProblem;
use perland_setup::text::{uninstall_words, Lang};
use perland_setup::uninstall::{UScreen, UStep};

fn main() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/setup-renders");
    std::fs::create_dir_all(&out).expect("create the output folder");
    let installing = |step, progress, confirm_stop| Screen::Installing {
        step,
        progress,
        confirm_stop,
    };
    let screens: Vec<(&str, Screen, Option<Action>)> = vec![
        ("01-install", Screen::Ready(Offer::Install), None),
        (
            "02-install-hover",
            Screen::Ready(Offer::Install),
            Some(Action::Primary),
        ),
        (
            "03-update",
            Screen::Ready(Offer::Update {
                from: "0.1.4".into(),
            }),
            None,
        ),
        ("04-reinstall", Screen::Ready(Offer::Reinstall), None),
        ("05-newer", Screen::Ready(Offer::Newer), None),
        ("06-launcher-running", Screen::LauncherRunning, None),
        (
            "07-getting-ready",
            installing(Step::GettingReady, 0.05, false),
            None,
        ),
        (
            "08-stop-prompt",
            installing(Step::GettingReady, 0.05, true),
            None,
        ),
        (
            "09-installing",
            installing(Step::Installing, 0.55, false),
            None,
        ),
        (
            "10-finishing-up",
            installing(Step::FinishingUp, 1.0, false),
            None,
        ),
        ("11-opening", Screen::Opening, None),
        ("12-installed", Screen::Installed, None),
        (
            "13-error",
            Screen::Error(ErrorCode::InstallerFailed(2)),
            None,
        ),
        (
            "14-options",
            Screen::Options {
                offer: Offer::Install,
                problem: None,
            },
            None,
        ),
        (
            "15-options-folder-problem",
            Screen::Options {
                offer: Offer::Install,
                problem: Some(FolderProblem::NeedsAdmin),
            },
            None,
        ),
        (
            "16-options-update",
            Screen::Options {
                offer: Offer::Update {
                    from: "0.1.4".into(),
                },
                problem: None,
            },
            None,
        ),
        (
            "17-uninstall-confirm",
            Screen::Uninstall(UScreen::Confirm),
            None,
        ),
        (
            "18-uninstall-delete-ticked",
            Screen::Uninstall(UScreen::Confirm),
            None,
        ),
        (
            "19-uninstalling",
            Screen::Uninstall(UScreen::Removing {
                step: UStep::RemovingApp,
                progress: 0.35,
            }),
            None,
        ),
        (
            "20-uninstalled-kept",
            Screen::Uninstall(UScreen::Done { kept_data: true }),
            None,
        ),
        (
            "21-uninstall-failed",
            Screen::Uninstall(UScreen::Failed {
                code: 0,
                // Replaced per language below, as the app does.
                not_deleted: vec![String::new()],
            }),
            None,
        ),
    ];
    for lang in [Lang::En, Lang::Es] {
        for (name, screen, hover) in &screens {
            let localized;
            let screen = match screen {
                Screen::Uninstall(UScreen::Failed { code, .. }) => {
                    localized = Screen::Uninstall(UScreen::Failed {
                        code: *code,
                        not_deleted: vec![uninstall_words(lang).sign_ins.into()],
                    });
                    &localized
                }
                other => other,
            };
            let ui = Ui {
                lang,
                payload_version: perland_setup::VERSION.into(),
                hover: *hover,
                focus: None,
                fade: 1.0,
                paths: Paths {
                    delete_data: name.contains("delete-ticked"),
                    ..Paths::preview()
                },
            };
            for scale in [1.0, 2.0] {
                let lang_name = if lang == Lang::En { "en" } else { "es" };
                let path = out.join(format!("{name}-{lang_name}@{scale}x.png"));
                render(screen, &ui, scale)
                    .save_png(&path)
                    .expect("write png");
            }
        }
    }
    println!("{}", out.canonicalize().expect("output exists").display());
}
