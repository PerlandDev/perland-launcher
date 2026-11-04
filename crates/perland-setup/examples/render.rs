//! Writes every setup screen in both languages to PNG, for review and the PR's screenshots:
//! `cargo run -p perland-setup --example render` → `target/setup-renders/`.

use perland_setup::draw::{render, Ui};
use perland_setup::model::{Action, ErrorCode, Offer, Screen, Step};
use perland_setup::text::Lang;

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
    ];
    for lang in [Lang::En, Lang::Es] {
        for (name, screen, hover) in &screens {
            let ui = Ui {
                lang,
                payload_version: perland_setup::VERSION.into(),
                hover: *hover,
                focus: None,
                fade: 1.0,
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
