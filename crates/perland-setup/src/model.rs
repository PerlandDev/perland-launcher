//! What the setup window shows and what it does next. Pure: the Windows shell feeds it events and
//! runs the effects it returns.

use crate::options::{Choice, FolderProblem};
use std::path::PathBuf;
use std::time::Duration;

/// What the first screen offers, from the installed version and the one inside this setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// Nothing installed.
    Install,
    /// An older (or unreadable) version is installed.
    Update { from: String },
    /// This version is installed: Open, or Reinstall as a repair.
    Reinstall,
    /// A newer version is installed: Open only. The setup never downgrades.
    Newer,
}

pub fn offer(installed: Option<&str>, payload: &str) -> Offer {
    let Some(installed) = installed else {
        return Offer::Install;
    };
    let older = || Offer::Update {
        from: installed.to_string(),
    };
    let (Ok(have), Ok(ours)) = (
        semver::Version::parse(installed.trim()),
        semver::Version::parse(payload),
    ) else {
        return older();
    };
    match have.cmp(&ours) {
        std::cmp::Ordering::Less => older(),
        std::cmp::Ordering::Equal => Offer::Reinstall,
        std::cmp::Ordering::Greater => Offer::Newer,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    GettingReady,
    Installing,
    FinishingUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    PayloadMissing,
    TempUnwritable,
    InstallerFailed(i32),
    InstallNotFound,
    Unknown,
}

impl ErrorCode {
    pub fn code(&self) -> &'static str {
        match self {
            ErrorCode::PayloadMissing => "payload_missing",
            ErrorCode::TempUnwritable => "temp_unwritable",
            ErrorCode::InstallerFailed(_) => "installer_failed",
            ErrorCode::InstallNotFound => "install_not_found",
            ErrorCode::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Ready(Offer),
    /// The install options view, opened from a Ready screen (`offer` is that screen's).
    Options {
        offer: Offer,
        problem: Option<FolderProblem>,
    },
    LauncherRunning,
    Installing {
        step: Step,
        progress: f32,
        confirm_stop: bool,
    },
    Opening,
    /// Installed, but the launcher couldn't be opened: offers OPEN.
    Installed,
    /// The uninstaller's own screens (it shares `LauncherRunning` with the setup).
    Uninstall(crate::uninstall::UScreen),
    Error(ErrorCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Primary,
    Secondary,
    Close,
    CopyDetails,
    Options,
    ChangeFolder,
    ToggleStartMenu,
    ToggleDesktop,
    Back,
    ToggleDeleteData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Pressed(Action),
    RunningChecked(bool),
    PayloadWritten,
    PayloadFailed(ErrorCode),
    InstallerExited(i32),
    /// After the installer exits 0: whether the registry shows the launcher installed.
    Confirmed(bool),
    Launched(bool),
    Tick(Duration),
    /// The folder picker closed: the folder picked, or `None` when cancelled.
    FolderPicked(Option<PathBuf>),
    FolderResolved(Result<PathBuf, FolderProblem>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    CheckRunning,
    WritePayload,
    RunInstaller,
    ConfirmInstall,
    Launch,
    DeletePayload,
    CopyDetails,
    Exit,
    PickFolder,
    ResolveFolder(PathBuf),
    /// After a fresh install: delete the one shortcut the player unticked.
    RemoveShortcuts,
}

pub struct Machine {
    pub screen: Screen,
    payload_present: bool,
    /// Set while a running-launcher check is in flight, so a second click starts nothing.
    checking: bool,
    /// Stop was confirmed while the payload was being written: exit once the writer reports.
    stopping: bool,
    elapsed: Duration,
    fresh: bool,
    pub choice: Choice,
}

impl Machine {
    /// A fresh install (nothing installed): only then can the folder and shortcuts be chosen.
    pub fn fresh(&self) -> bool {
        self.fresh
    }

    pub fn new(offer: Offer, payload_present: bool) -> Machine {
        Machine {
            screen: Screen::Ready(offer.clone()),
            payload_present,
            checking: false,
            stopping: false,
            elapsed: Duration::ZERO,
            fresh: offer == Offer::Install,
            choice: Choice::default(),
        }
    }

    fn check(&mut self) -> Vec<Effect> {
        if self.checking {
            return vec![];
        }
        self.checking = true;
        vec![Effect::CheckRunning]
    }

    fn launch(&mut self) -> Vec<Effect> {
        self.screen = Screen::Opening;
        vec![Effect::Launch]
    }

    fn installing(&mut self, step: Step, progress: f32) {
        self.screen = Screen::Installing {
            step,
            progress,
            confirm_stop: false,
        };
    }

    pub fn on(&mut self, event: Event) -> Vec<Effect> {
        use Action::*;
        if self.checking && matches!(event, Event::Pressed(Primary | Secondary)) {
            // One answer at a time: an Open pressed while the running check is out could start
            // the launcher just before the installer force-closes it.
            return vec![];
        }
        match (&self.screen, event) {
            (Screen::Ready(offer), Event::Pressed(Options)) if *offer != Offer::Newer => {
                self.screen = Screen::Options {
                    offer: offer.clone(),
                    problem: None,
                };
                vec![]
            }
            (Screen::Options { offer, .. }, Event::Pressed(Back | Close)) => {
                self.screen = Screen::Ready(offer.clone());
                vec![]
            }
            (Screen::Options { .. }, Event::Pressed(ChangeFolder)) if self.fresh => {
                vec![Effect::PickFolder]
            }
            (Screen::Options { .. }, Event::Pressed(ToggleStartMenu)) if self.fresh => {
                self.choice.start_menu = !self.choice.start_menu;
                vec![]
            }
            (Screen::Options { .. }, Event::Pressed(ToggleDesktop)) if self.fresh => {
                self.choice.desktop = !self.choice.desktop;
                vec![]
            }
            (Screen::Options { .. }, Event::FolderPicked(Some(path))) => {
                vec![Effect::ResolveFolder(path)]
            }
            (Screen::Options { offer, .. }, Event::FolderResolved(result)) => {
                let offer = offer.clone();
                let problem = match result {
                    Ok(folder) => {
                        self.choice.folder = Some(folder);
                        None
                    }
                    Err(problem) => Some(problem),
                };
                self.screen = Screen::Options { offer, problem };
                vec![]
            }
            (Screen::Options { problem: None, .. }, Event::Pressed(Primary)) => self.check(),
            (Screen::Ready(Offer::Install | Offer::Update { .. }), Event::Pressed(Primary))
            | (Screen::Ready(Offer::Reinstall), Event::Pressed(Secondary))
            | (Screen::LauncherRunning | Screen::Error(_), Event::Pressed(Primary)) => self.check(),
            (
                Screen::Ready(Offer::Reinstall | Offer::Newer) | Screen::Installed,
                Event::Pressed(Primary),
            ) => self.launch(),
            (
                Screen::Ready(_) | Screen::LauncherRunning | Screen::Error(_) | Screen::Installed,
                Event::Pressed(Close),
            ) => {
                vec![Effect::Exit]
            }
            (Screen::Error(_), Event::Pressed(CopyDetails)) => vec![Effect::CopyDetails],
            (_, Event::RunningChecked(running)) if self.checking => {
                self.checking = false;
                if running {
                    self.screen = Screen::LauncherRunning;
                    vec![]
                } else if !self.payload_present {
                    self.screen = Screen::Error(ErrorCode::PayloadMissing);
                    vec![]
                } else {
                    self.installing(Step::GettingReady, 0.0);
                    vec![Effect::WritePayload]
                }
            }
            (
                Screen::Installing {
                    step: Step::GettingReady,
                    progress,
                    confirm_stop,
                },
                Event::Pressed(action),
            ) => {
                let (progress, confirm_stop) = (*progress, *confirm_stop);
                match (action, confirm_stop) {
                    (Close, false) => {
                        self.screen = Screen::Installing {
                            step: Step::GettingReady,
                            progress,
                            confirm_stop: true,
                        };
                        vec![]
                    }
                    (Primary, true) => {
                        self.stopping = true;
                        vec![]
                    }
                    (Secondary, true) => {
                        self.installing(Step::GettingReady, progress);
                        vec![]
                    }
                    _ => vec![],
                }
            }
            (
                Screen::Installing {
                    step: Step::GettingReady,
                    ..
                },
                Event::PayloadWritten | Event::PayloadFailed(_),
            ) if self.stopping => {
                vec![Effect::DeletePayload, Effect::Exit]
            }
            (
                Screen::Installing {
                    step: Step::GettingReady,
                    ..
                },
                Event::PayloadWritten,
            ) => {
                self.elapsed = Duration::ZERO;
                self.installing(Step::Installing, installing_progress(Duration::ZERO));
                vec![Effect::RunInstaller]
            }
            (
                Screen::Installing {
                    step: Step::GettingReady,
                    ..
                },
                Event::PayloadFailed(code),
            ) => {
                self.screen = Screen::Error(code);
                vec![Effect::DeletePayload]
            }
            (
                Screen::Installing {
                    step: Step::Installing,
                    ..
                },
                Event::Tick(dt),
            ) => {
                self.elapsed += dt;
                self.installing(Step::Installing, installing_progress(self.elapsed));
                vec![]
            }
            (
                Screen::Installing {
                    step: Step::Installing,
                    ..
                },
                Event::InstallerExited(code),
            ) => {
                if code == 0 {
                    self.installing(Step::FinishingUp, 1.0);
                    let confirm = vec![Effect::ConfirmInstall, Effect::DeletePayload];
                    if self.fresh {
                        [vec![Effect::RemoveShortcuts], confirm].concat()
                    } else {
                        confirm
                    }
                } else {
                    self.screen = Screen::Error(ErrorCode::InstallerFailed(code));
                    vec![Effect::DeletePayload]
                }
            }
            (
                Screen::Installing {
                    step: Step::FinishingUp,
                    ..
                },
                Event::Confirmed(found),
            ) => {
                if found {
                    self.launch()
                } else {
                    self.screen = Screen::Error(ErrorCode::InstallNotFound);
                    vec![]
                }
            }
            (Screen::Opening, Event::Launched(true)) => vec![Effect::Exit],
            (Screen::Opening, Event::Launched(false)) => {
                self.screen = Screen::Installed;
                vec![]
            }
            _ => vec![],
        }
    }
}

/// Progress while the NSIS installer runs: eases from 15% toward 90% over about 20 s and never
/// passes 90% until the installer exits.
pub fn installing_progress(elapsed: Duration) -> f32 {
    let t = elapsed.as_secs_f32();
    (0.15 + 0.75 * (1.0 - (-3.0 * t / 20.0).exp())).min(0.90)
}

/// Paces animation frames: at most one per `every`, whatever wakes the event loop in between.
#[derive(Debug, Default)]
pub struct Pacer {
    last: Option<std::time::Instant>,
}

impl Pacer {
    /// `Ok(time since the last frame)` when a frame is due, else `Err(when the next one is)`.
    pub fn frame(
        &mut self,
        now: std::time::Instant,
        every: Duration,
    ) -> Result<Duration, std::time::Instant> {
        match self.last {
            Some(last) if now < last + every => Err(last + every),
            last => {
                self.last = Some(now);
                Ok(last.map_or(Duration::ZERO, |l| now - l))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilentPlan {
    Run,
    Exit(i32),
}

/// What `/S` does: never close a running launcher (exit 2), never downgrade (exit 0), else run.
pub fn silent_plan(installed: Option<&str>, payload: &str, running: bool) -> SilentPlan {
    if running {
        SilentPlan::Exit(2)
    } else if offer(installed, payload) == Offer::Newer {
        SilentPlan::Exit(0)
    } else {
        SilentPlan::Run
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{Choice, FolderProblem};
    use std::path::PathBuf;
    use Effect::*;
    use Event::*;

    fn press(m: &mut Machine, a: Action) -> Vec<Effect> {
        m.on(Pressed(a))
    }

    fn installing(m: &Machine) -> Option<Step> {
        match m.screen {
            Screen::Installing { step, .. } => Some(step),
            _ => None,
        }
    }

    #[test]
    fn offer_none_is_install() {
        assert_eq!(offer(None, "0.2.0"), Offer::Install);
    }

    #[test]
    fn offer_older_is_update() {
        assert_eq!(
            offer(Some("0.1.0"), "0.2.0"),
            Offer::Update {
                from: "0.1.0".into()
            }
        );
        assert_eq!(
            offer(Some("0.2.0-beta.3"), "0.2.0"),
            Offer::Update {
                from: "0.2.0-beta.3".into()
            }
        );
    }

    #[test]
    fn offer_same_is_reinstall() {
        assert_eq!(offer(Some("0.2.0"), "0.2.0"), Offer::Reinstall);
    }

    #[test]
    fn offer_newer_is_newer() {
        assert_eq!(offer(Some("0.3.0"), "0.2.0"), Offer::Newer);
        assert_eq!(offer(Some("0.2.0"), "0.2.0-beta.3"), Offer::Newer);
    }

    #[test]
    fn offer_garbage_is_update() {
        assert_eq!(
            offer(Some("dev"), "0.2.0"),
            Offer::Update { from: "dev".into() }
        );
        assert_eq!(offer(Some(""), "0.2.0"), Offer::Update { from: "".into() });
    }

    #[test]
    fn install_happy_path() {
        let mut m = Machine::new(Offer::Install, true);
        assert_eq!(m.screen, Screen::Ready(Offer::Install));
        assert_eq!(press(&mut m, Action::Primary), vec![CheckRunning]);
        assert_eq!(m.on(RunningChecked(false)), vec![WritePayload]);
        assert_eq!(installing(&m), Some(Step::GettingReady));
        assert_eq!(m.on(PayloadWritten), vec![RunInstaller]);
        assert_eq!(installing(&m), Some(Step::Installing));
        assert_eq!(
            m.on(InstallerExited(0)),
            vec![RemoveShortcuts, ConfirmInstall, DeletePayload]
        );
        assert_eq!(installing(&m), Some(Step::FinishingUp));
        assert_eq!(m.on(Confirmed(true)), vec![Launch]);
        assert_eq!(m.screen, Screen::Opening);
        assert_eq!(m.on(Launched(true)), vec![Exit]);
    }

    #[test]
    fn double_primary_starts_one_install() {
        let mut m = Machine::new(Offer::Install, true);
        assert_eq!(press(&mut m, Action::Primary), vec![CheckRunning]);
        assert_eq!(press(&mut m, Action::Primary), vec![]);
        m.on(RunningChecked(false));
        assert_eq!(press(&mut m, Action::Primary), vec![]);
        m.on(PayloadWritten);
        assert_eq!(press(&mut m, Action::Primary), vec![]);
        m.on(InstallerExited(0));
        m.on(Confirmed(true));
        assert_eq!(press(&mut m, Action::Primary), vec![]);
    }

    #[test]
    fn running_launcher_blocks() {
        let mut m = Machine::new(
            Offer::Update {
                from: "0.1.0".into(),
            },
            true,
        );
        press(&mut m, Action::Primary);
        assert_eq!(m.on(RunningChecked(true)), vec![]);
        assert_eq!(m.screen, Screen::LauncherRunning);
        assert_eq!(press(&mut m, Action::Primary), vec![CheckRunning]);
        assert_eq!(m.on(RunningChecked(false)), vec![WritePayload]);
    }

    #[test]
    fn installer_failure_is_error_with_code() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        m.on(PayloadWritten);
        assert_eq!(m.on(InstallerExited(2)), vec![DeletePayload]);
        assert_eq!(m.screen, Screen::Error(ErrorCode::InstallerFailed(2)));
        assert_eq!(press(&mut m, Action::CopyDetails), vec![CopyDetails]);
        assert_eq!(press(&mut m, Action::Primary), vec![CheckRunning]);
    }

    #[test]
    fn payload_failure_is_error() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        assert_eq!(
            m.on(PayloadFailed(ErrorCode::TempUnwritable)),
            vec![DeletePayload]
        );
        assert_eq!(m.screen, Screen::Error(ErrorCode::TempUnwritable));
    }

    #[test]
    fn install_not_found_is_error() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        m.on(PayloadWritten);
        m.on(InstallerExited(0));
        assert_eq!(m.on(Confirmed(false)), vec![]);
        assert_eq!(m.screen, Screen::Error(ErrorCode::InstallNotFound));
    }

    #[test]
    fn close_during_getting_ready_asks_then_stops() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        assert_eq!(press(&mut m, Action::Close), vec![]);
        assert!(matches!(
            m.screen,
            Screen::Installing {
                confirm_stop: true,
                ..
            }
        ));
        assert_eq!(press(&mut m, Action::Secondary), vec![]);
        assert!(matches!(
            m.screen,
            Screen::Installing {
                confirm_stop: false,
                ..
            }
        ));
        press(&mut m, Action::Close);
        // The writer is still running: wait for it, so its folder is removed before exiting.
        assert_eq!(press(&mut m, Action::Primary), vec![]);
        assert_eq!(m.on(PayloadWritten), vec![DeletePayload, Exit]);
    }

    #[test]
    fn stop_waits_for_a_failed_write_too() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        press(&mut m, Action::Close);
        press(&mut m, Action::Primary);
        assert_eq!(
            m.on(PayloadFailed(ErrorCode::TempUnwritable)),
            vec![DeletePayload, Exit]
        );
    }

    #[test]
    fn presses_while_checking_start_nothing() {
        // Reinstall, then OPEN before the running check answers: the launcher must not open,
        // or the installer would force-close it.
        let mut m = Machine::new(Offer::Reinstall, true);
        assert_eq!(press(&mut m, Action::Secondary), vec![CheckRunning]);
        assert_eq!(press(&mut m, Action::Primary), vec![]);
        assert_eq!(m.on(RunningChecked(false)), vec![WritePayload]);
    }

    #[test]
    fn stop_confirmed_after_payload_written_does_not_stop() {
        // The prompt was open when the installer started: the installer is never stopped.
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        press(&mut m, Action::Close);
        assert_eq!(m.on(PayloadWritten), vec![RunInstaller]);
        assert!(matches!(
            m.screen,
            Screen::Installing {
                step: Step::Installing,
                confirm_stop: false,
                ..
            }
        ));
        assert_eq!(press(&mut m, Action::Primary), vec![]);
    }

    #[test]
    fn close_while_installing_does_nothing() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        m.on(PayloadWritten);
        assert_eq!(press(&mut m, Action::Close), vec![]);
        assert!(matches!(
            m.screen,
            Screen::Installing {
                step: Step::Installing,
                confirm_stop: false,
                ..
            }
        ));
        m.on(InstallerExited(0));
        assert_eq!(press(&mut m, Action::Close), vec![]);
    }

    #[test]
    fn close_on_idle_screens_exits() {
        let mut m = Machine::new(Offer::Install, true);
        assert_eq!(press(&mut m, Action::Close), vec![Exit]);
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(true));
        assert_eq!(press(&mut m, Action::Close), vec![Exit]);
    }

    #[test]
    fn open_on_newer_launches() {
        let mut m = Machine::new(Offer::Newer, true);
        assert_eq!(press(&mut m, Action::Secondary), vec![]);
        assert_eq!(press(&mut m, Action::Primary), vec![Launch]);
        assert_eq!(m.screen, Screen::Opening);
    }

    #[test]
    fn reinstall_runs_the_install() {
        let mut m = Machine::new(Offer::Reinstall, true);
        assert_eq!(press(&mut m, Action::Secondary), vec![CheckRunning]);
        assert_eq!(m.on(RunningChecked(false)), vec![WritePayload]);
    }

    #[test]
    fn launch_failure_shows_installed() {
        let mut m = Machine::new(Offer::Reinstall, true);
        press(&mut m, Action::Primary);
        assert_eq!(m.on(Launched(false)), vec![]);
        assert_eq!(m.screen, Screen::Installed);
        assert_eq!(press(&mut m, Action::Primary), vec![Launch]);
    }

    #[test]
    fn payload_missing_errors() {
        let mut m = Machine::new(Offer::Install, false);
        press(&mut m, Action::Primary);
        assert_eq!(m.on(RunningChecked(false)), vec![]);
        assert_eq!(m.screen, Screen::Error(ErrorCode::PayloadMissing));
    }

    #[test]
    fn progress_bounds() {
        assert!((installing_progress(Duration::ZERO) - 0.15).abs() < 1e-6);
        let at20 = installing_progress(Duration::from_secs(20));
        assert!((at20 - 0.8627).abs() < 0.01, "{at20}");
        assert!(installing_progress(Duration::from_secs(600)) <= 0.90);
        assert!(
            installing_progress(Duration::from_secs(5))
                < installing_progress(Duration::from_secs(6))
        );
    }

    #[test]
    fn tick_advances_only_while_installing() {
        let mut m = Machine::new(Offer::Install, true);
        assert_eq!(m.on(Tick(Duration::from_secs(1))), vec![]);
        assert_eq!(m.screen, Screen::Ready(Offer::Install));
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        m.on(Tick(Duration::from_secs(5)));
        assert!(matches!(m.screen, Screen::Installing { progress, .. } if progress == 0.0));
        m.on(PayloadWritten);
        m.on(Tick(Duration::from_secs(10)));
        let Screen::Installing { progress, .. } = m.screen else {
            panic!()
        };
        assert!((progress - installing_progress(Duration::from_secs(10))).abs() < 1e-6);
        m.on(InstallerExited(0));
        assert!(matches!(m.screen, Screen::Installing { progress, .. } if progress == 1.0));
    }

    #[test]
    fn pacer_spaces_frames() {
        let every = Duration::from_millis(16);
        let t0 = std::time::Instant::now();
        let mut pacer = Pacer::default();
        assert_eq!(pacer.frame(t0, every), Ok(Duration::ZERO));
        assert_eq!(
            pacer.frame(t0 + Duration::from_millis(3), every),
            Err(t0 + every)
        );
        assert_eq!(
            pacer.frame(t0 + Duration::from_millis(20), every),
            Ok(Duration::from_millis(20))
        );
        assert_eq!(
            pacer.frame(t0 + Duration::from_millis(21), every),
            Err(t0 + Duration::from_millis(36))
        );
    }

    fn options_of(m: &Machine) -> Option<Option<FolderProblem>> {
        match &m.screen {
            Screen::Options { problem, .. } => Some(*problem),
            _ => None,
        }
    }

    #[test]
    fn options_opens_and_back_returns() {
        let mut m = Machine::new(Offer::Install, true);
        assert_eq!(press(&mut m, Action::Options), vec![]);
        assert_eq!(options_of(&m), Some(None));
        assert_eq!(press(&mut m, Action::Back), vec![]);
        assert_eq!(m.screen, Screen::Ready(Offer::Install));
        press(&mut m, Action::Options);
        assert_eq!(press(&mut m, Action::Close), vec![]);
        assert_eq!(m.screen, Screen::Ready(Offer::Install));
    }

    #[test]
    fn change_folder_picks_then_resolves() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Options);
        assert_eq!(press(&mut m, Action::ChangeFolder), vec![PickFolder]);
        let picked = PathBuf::from("D:\\Games");
        assert_eq!(
            m.on(FolderPicked(Some(picked.clone()))),
            vec![ResolveFolder(picked)]
        );
        let resolved = PathBuf::from("D:\\Games\\Perland Launcher");
        assert_eq!(m.on(FolderResolved(Ok(resolved.clone()))), vec![]);
        assert_eq!(m.choice.folder, Some(resolved));
        assert_eq!(options_of(&m), Some(None));
    }

    #[test]
    fn cancelled_picker_changes_nothing() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Options);
        press(&mut m, Action::ChangeFolder);
        assert_eq!(m.on(FolderPicked(None)), vec![]);
        assert_eq!(m.choice, Choice::default());
    }

    #[test]
    fn refused_folder_blocks_install() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Options);
        m.on(FolderResolved(Err(FolderProblem::NotEmpty)));
        assert_eq!(options_of(&m), Some(Some(FolderProblem::NotEmpty)));
        assert_eq!(m.choice.folder, None);
        assert_eq!(press(&mut m, Action::Primary), vec![]);
        m.on(FolderResolved(Ok(PathBuf::from("E:\\PL"))));
        assert_eq!(press(&mut m, Action::Primary), vec![CheckRunning]);
        assert_eq!(m.on(RunningChecked(false)), vec![WritePayload]);
    }

    #[test]
    fn update_cannot_change_folder_or_shortcuts() {
        let mut m = Machine::new(
            Offer::Update {
                from: "0.1.0".into(),
            },
            true,
        );
        assert!(!m.fresh());
        press(&mut m, Action::Options);
        assert_eq!(options_of(&m), Some(None));
        assert_eq!(press(&mut m, Action::ChangeFolder), vec![]);
        press(&mut m, Action::ToggleDesktop);
        assert!(m.choice.desktop);
        assert_eq!(press(&mut m, Action::Primary), vec![CheckRunning]);
    }

    #[test]
    fn newer_has_no_options() {
        let mut m = Machine::new(Offer::Newer, true);
        assert_eq!(press(&mut m, Action::Options), vec![]);
        assert_eq!(m.screen, Screen::Ready(Offer::Newer));
    }

    #[test]
    fn toggles_flip_on_fresh_install() {
        let mut m = Machine::new(Offer::Install, true);
        assert!(m.fresh());
        press(&mut m, Action::Options);
        press(&mut m, Action::ToggleDesktop);
        press(&mut m, Action::ToggleStartMenu);
        assert!(!m.choice.desktop && !m.choice.start_menu);
        press(&mut m, Action::ToggleDesktop);
        assert!(m.choice.desktop);
    }

    #[test]
    fn fresh_install_removes_shortcuts_after_success() {
        let mut m = Machine::new(Offer::Install, true);
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        m.on(PayloadWritten);
        assert_eq!(
            m.on(InstallerExited(0)),
            vec![RemoveShortcuts, ConfirmInstall, DeletePayload]
        );
    }

    #[test]
    fn update_does_not_remove_shortcuts() {
        let mut m = Machine::new(
            Offer::Update {
                from: "0.1.0".into(),
            },
            true,
        );
        press(&mut m, Action::Primary);
        m.on(RunningChecked(false));
        m.on(PayloadWritten);
        assert_eq!(
            m.on(InstallerExited(0)),
            vec![ConfirmInstall, DeletePayload]
        );
    }

    #[test]
    fn silent_plan_cases() {
        assert_eq!(
            silent_plan(Some("0.1.0"), "0.2.0", true),
            SilentPlan::Exit(2)
        );
        assert_eq!(
            silent_plan(Some("0.3.0"), "0.2.0", false),
            SilentPlan::Exit(0)
        );
        assert_eq!(silent_plan(None, "0.2.0", false), SilentPlan::Run);
        assert_eq!(silent_plan(Some("0.1.0"), "0.2.0", false), SilentPlan::Run);
        assert_eq!(silent_plan(Some("0.2.0"), "0.2.0", false), SilentPlan::Run);
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(ErrorCode::PayloadMissing.code(), "payload_missing");
        assert_eq!(ErrorCode::TempUnwritable.code(), "temp_unwritable");
        assert_eq!(ErrorCode::InstallerFailed(3).code(), "installer_failed");
        assert_eq!(ErrorCode::InstallNotFound.code(), "install_not_found");
        assert_eq!(ErrorCode::Unknown.code(), "unknown");
    }
}
