//! Every word the setup shows, in English and Spanish (neutral Latin American, informal "tú").

use crate::model::{ErrorCode, Offer, Screen, Step};
use crate::options::FolderProblem;
use crate::uninstall::{UScreen, UStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
}

/// The language for a Windows UI language id (`GetUserDefaultUILanguage`): any Spanish is Spanish.
pub fn lang_from_langid(langid: u16) -> Lang {
    const LANG_SPANISH: u16 = 0x0a;
    if langid & 0x3ff == LANG_SPANISH {
        Lang::Es
    } else {
        Lang::En
    }
}

pub struct Strings {
    pub brand: &'static str,
    pub tagline: &'static str,
    pub installing: &'static str,
    pub stop_title: &'static str,
    pub stop_detail: &'static str,
    pub cant_stop: &'static str,
    pub all_set: &'static str,
    pub opening: &'static str,
    pub installed_title: &'static str,
    pub installed_detail: &'static str,
    pub already_installed: &'static str,
    pub running_title: &'static str,
    pub running_detail: &'static str,
    pub error_title: &'static str,
    pub install: &'static str,
    pub update: &'static str,
    pub open: &'static str,
    pub reinstall: &'static str,
    pub try_again: &'static str,
    pub copy_details: &'static str,
    pub stop: &'static str,
    pub keep_installing: &'static str,
    pub close: &'static str,
    pub window_title: &'static str,
}

static EN: Strings = Strings {
    brand: "PERLAND LAUNCHER",
    tagline: "Play Minecraft on Perland servers, in one click.",
    installing: "INSTALLING…",
    stop_title: "STOP THE INSTALL?",
    stop_detail: "Nothing has been installed yet.",
    cant_stop: "Almost there, this can't be stopped now",
    all_set: "ALL SET",
    opening: "Opening Perland Launcher…",
    installed_title: "INSTALLED",
    installed_detail: "Perland Launcher is installed. Open it to start playing.",
    already_installed: "Perland Launcher is installed.",
    running_title: "CLOSE THE LAUNCHER",
    running_detail: "Close Perland Launcher to continue. If a game is running, quit it first.",
    error_title: "SOMETHING WENT WRONG",
    install: "INSTALL",
    update: "UPDATE",
    open: "OPEN",
    reinstall: "Reinstall",
    try_again: "TRY AGAIN",
    copy_details: "Copy details",
    stop: "STOP",
    keep_installing: "Keep installing",
    close: "Close",
    window_title: "Perland Launcher Setup",
};

static ES: Strings = Strings {
    brand: "PERLAND LAUNCHER",
    tagline: "Juega Minecraft en servidores de Perland, con un clic.",
    installing: "INSTALANDO…",
    stop_title: "¿DETENER LA INSTALACIÓN?",
    stop_detail: "Todavía no se instaló nada.",
    cant_stop: "Ya casi, esto no se puede detener ahora",
    all_set: "¡LISTO!",
    opening: "Abriendo Perland Launcher…",
    installed_title: "INSTALADO",
    installed_detail: "Perland Launcher está instalado. Ábrelo para empezar a jugar.",
    already_installed: "Perland Launcher ya está instalado.",
    running_title: "CIERRA EL LAUNCHER",
    running_detail:
        "Cierra Perland Launcher para continuar. Si hay un juego abierto, ciérralo primero.",
    error_title: "ALGO SALIÓ MAL",
    install: "INSTALAR",
    update: "ACTUALIZAR",
    open: "ABRIR",
    reinstall: "Reinstalar",
    try_again: "REINTENTAR",
    copy_details: "Copiar detalles",
    stop: "DETENER",
    keep_installing: "Seguir instalando",
    close: "Cerrar",
    window_title: "Instalador de Perland Launcher",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Es => &ES,
    }
}

pub fn step_text(lang: Lang, step: Step) -> &'static str {
    match (lang, step) {
        (Lang::En, Step::GettingReady) => "Getting ready",
        (Lang::En, Step::Installing) => "Copying files",
        (Lang::En, Step::FinishingUp) => "Finishing up",
        (Lang::Es, Step::GettingReady) => "Preparando",
        (Lang::Es, Step::Installing) => "Copiando archivos",
        (Lang::Es, Step::FinishingUp) => "Terminando",
    }
}

pub fn error_text(lang: Lang, code: &ErrorCode) -> String {
    match (lang, code) {
        (Lang::En, ErrorCode::PayloadMissing) => {
            "This setup has no installer inside. Download it again from perland.online/launcher.".into()
        }
        (Lang::En, ErrorCode::TempUnwritable) => {
            "Couldn't prepare the install. Check there's free space on your drive and try again.".into()
        }
        (Lang::En, ErrorCode::InstallerFailed(n)) => format!("The install didn't finish (code {n}). Try again."),
        (Lang::En, ErrorCode::InstallNotFound) => {
            "The install finished, but Perland Launcher wasn't found. Try again.".into()
        }
        (Lang::En, ErrorCode::Unknown) => "Something went wrong. Copy the details and send them to us.".into(),
        (Lang::Es, ErrorCode::PayloadMissing) => {
            "Este instalador no trae nada dentro. Descárgalo de nuevo desde perland.online/launcher.".into()
        }
        (Lang::Es, ErrorCode::TempUnwritable) => {
            "No se pudo preparar la instalación. Revisa que haya espacio libre en tu disco y vuelve a intentarlo."
                .into()
        }
        (Lang::Es, ErrorCode::InstallerFailed(n)) => {
            format!("La instalación no terminó (código {n}). Vuelve a intentarlo.")
        }
        (Lang::Es, ErrorCode::InstallNotFound) => {
            "La instalación terminó, pero no se encontró Perland Launcher. Vuelve a intentarlo.".into()
        }
        (Lang::Es, ErrorCode::Unknown) => "Algo salió mal. Copia los detalles y envíanoslos.".into(),
    }
}

/// The small line at the bottom of the Ready screens.
pub fn version_line(lang: Lang, offer: &Offer, payload: &str) -> String {
    match (lang, offer) {
        (Lang::En, Offer::Install) => {
            format!("Version {payload} · Installs for you only, no admin needed")
        }
        (Lang::En, Offer::Update { from }) => format!("Updates version {from} to {payload}"),
        (Lang::En, Offer::Reinstall) => format!("Version {payload} is installed"),
        (Lang::En, Offer::Newer) => format!("A version newer than {payload} is installed"),
        (Lang::Es, Offer::Install) => {
            format!("Versión {payload} · Se instala solo para ti, sin permisos de administrador")
        }
        (Lang::Es, Offer::Update { from }) => format!("Actualiza la versión {from} a la {payload}"),
        (Lang::Es, Offer::Reinstall) => format!("La versión {payload} está instalada"),
        (Lang::Es, Offer::Newer) => format!("Hay instalada una versión más nueva que la {payload}"),
    }
}

/// The big line of a screen.
pub fn headline(lang: Lang, screen: &Screen) -> &'static str {
    let s = strings(lang);
    match screen {
        Screen::Ready(_) => s.brand,
        Screen::Options { .. } => options_words(lang).title,
        Screen::Uninstall(u) => {
            let w = uninstall_words(lang);
            match u {
                UScreen::Confirm => w.title,
                UScreen::Removing { .. } => w.uninstalling,
                UScreen::Done { .. } => w.uninstalled,
                UScreen::Failed { .. } => s.error_title,
            }
        }
        Screen::LauncherRunning => s.running_title,
        Screen::Installing {
            confirm_stop: true, ..
        } => s.stop_title,
        Screen::Installing { .. } => s.installing,
        Screen::Opening => s.all_set,
        Screen::Installed => s.installed_title,
        Screen::Error(_) => s.error_title,
    }
}

/// The sentence under the headline (may be wrapped by the drawing).
pub fn detail(lang: Lang, screen: &Screen) -> String {
    let s = strings(lang);
    match screen {
        Screen::Ready(Offer::Install | Offer::Update { .. }) => s.tagline.into(),
        Screen::Ready(Offer::Reinstall | Offer::Newer) => s.already_installed.into(),
        Screen::Options { .. } => options_words(lang).install_to.into(),
        Screen::Uninstall(u) => {
            let w = uninstall_words(lang);
            match u {
                UScreen::Confirm => w.removes_from.into(),
                UScreen::Removing { step: UStep::RemovingApp, .. } => w.removing_app.into(),
                UScreen::Removing { step: UStep::DeletingData, .. } => w.deleting_data.into(),
                UScreen::Done { kept_data: true } => w.done_kept.into(),
                UScreen::Done { kept_data: false } => w.done_deleted.into(),
                UScreen::Failed { not_deleted, .. } if !not_deleted.is_empty() => w.partly.into(),
                UScreen::Failed { code, .. } => match lang {
                    Lang::En => format!("The uninstall didn't finish (code {code}). Try again from Settings > Apps."),
                    Lang::Es => format!(
                        "La desinstalación no terminó (código {code}). Vuelve a intentarlo desde Configuración > Aplicaciones."
                    ),
                },
            }
        }
        Screen::LauncherRunning => s.running_detail.into(),
        Screen::Installing {
            confirm_stop: true, ..
        } => s.stop_detail.into(),
        Screen::Installing { step, .. } => step_text(lang, *step).into(),
        Screen::Opening => s.opening.into(),
        Screen::Installed => s.installed_detail.into(),
        Screen::Error(code) => error_text(lang, code),
    }
}

/// The footer line: the version on the Ready screens, the can't-stop hint while installing.
pub fn footer(lang: Lang, screen: &Screen, payload: &str) -> Option<String> {
    match screen {
        Screen::Ready(offer) => Some(version_line(lang, offer, payload)),
        Screen::Installing {
            step: Step::Installing | Step::FinishingUp,
            ..
        } => Some(strings(lang).cant_stop.into()),
        _ => None,
    }
}

/// (primary, secondary) button labels for a screen.
pub fn button_labels(lang: Lang, screen: &Screen) -> (Option<&'static str>, Option<&'static str>) {
    let s = strings(lang);
    match screen {
        Screen::Ready(Offer::Install) => (Some(s.install), None),
        Screen::Options { offer, .. } => {
            let primary = match offer {
                Offer::Install => s.install,
                Offer::Update { .. } => s.update,
                Offer::Reinstall | Offer::Newer => options_words(lang).reinstall,
            };
            (Some(primary), Some(options_words(lang).back))
        }
        Screen::Uninstall(u) => {
            let w = uninstall_words(lang);
            match u {
                UScreen::Confirm => (Some(w.uninstall), Some(w.cancel)),
                UScreen::Removing { .. } => (None, None),
                UScreen::Done { .. } => (Some(w.close), None),
                UScreen::Failed { .. } => (Some(w.close), Some(s.copy_details)),
            }
        }
        Screen::Ready(Offer::Update { .. }) => (Some(s.update), None),
        Screen::Ready(Offer::Reinstall) => (Some(s.open), Some(s.reinstall)),
        Screen::Ready(Offer::Newer) | Screen::Installed => (Some(s.open), None),
        Screen::LauncherRunning => (Some(s.try_again), None),
        Screen::Installing {
            confirm_stop: true, ..
        } => (Some(s.stop), Some(s.keep_installing)),
        Screen::Installing { .. } | Screen::Opening => (None, None),
        Screen::Error(_) => (Some(s.try_again), Some(s.copy_details)),
    }
}

/// Words of the install options view (spec §4a).
pub struct OptionWords {
    pub link: &'static str,
    pub title: &'static str,
    pub install_to: &'static str,
    pub change: &'static str,
    pub for_you: &'static str,
    pub about: &'static str,
    pub update_folder: &'static str,
    pub data_stays: &'static str,
    pub what: &'static str,
    pub items: [&'static str; 4],
    pub start_menu: &'static str,
    pub desktop: &'static str,
    pub shortcuts_stay: &'static str,
    pub back: &'static str,
    pub reinstall: &'static str,
}

static OPTIONS_EN: OptionWords = OptionWords {
    link: "Options",
    title: "INSTALL OPTIONS",
    install_to: "Install to",
    change: "Change…",
    for_you: "For you only, no admin needed",
    about: "About",
    update_folder: "Updates keep the folder it's in",
    data_stays: "Your games, worlds and settings stay in",
    what: "What gets installed",
    items: [
        "Perland Launcher, and its uninstaller",
        "Microsoft WebView2, only if Windows doesn't have it",
        "perland:// links, so invite pages open the launcher",
        "An entry in Settings > Apps",
    ],
    start_menu: "Start menu shortcut",
    desktop: "Desktop shortcut",
    shortcuts_stay: "Shortcuts stay as they are",
    back: "Back",
    reinstall: "REINSTALL",
};

static OPTIONS_ES: OptionWords = OptionWords {
    link: "Opciones",
    title: "OPCIONES DE INSTALACIÓN",
    install_to: "Instalar en",
    change: "Cambiar…",
    for_you: "Solo para ti, sin permisos de administrador",
    about: "Unos",
    update_folder: "Las actualizaciones usan la carpeta actual",
    data_stays: "Tus juegos, mundos y ajustes se quedan en",
    what: "Qué se instala",
    items: [
        "Perland Launcher y su desinstalador",
        "Microsoft WebView2, solo si Windows no lo tiene",
        "Enlaces perland://, para que las invitaciones abran el launcher",
        "Una entrada en Configuración > Aplicaciones",
    ],
    start_menu: "Acceso en el menú Inicio",
    desktop: "Acceso en el escritorio",
    shortcuts_stay: "Los accesos directos se quedan como están",
    back: "Volver",
    reinstall: "REINSTALAR",
};

pub fn options_words(lang: Lang) -> &'static OptionWords {
    match lang {
        Lang::En => &OPTIONS_EN,
        Lang::Es => &OPTIONS_ES,
    }
}

/// Words of the uninstaller (spec §4b).
pub struct UninstallWords {
    pub window_title: &'static str,
    pub title: &'static str,
    pub removes_from: &'static str,
    pub removes_also: &'static str,
    pub delete_box: &'static str,
    pub uninstall: &'static str,
    pub uninstall_delete: &'static str,
    pub cancel: &'static str,
    pub uninstalling: &'static str,
    pub removing_app: &'static str,
    pub deleting_data: &'static str,
    pub uninstalled: &'static str,
    pub done_kept: &'static str,
    pub done_deleted: &'static str,
    pub partly: &'static str,
    pub close: &'static str,
    pub sign_ins: &'static str,
}

static UNINSTALL_EN: UninstallWords = UninstallWords {
    window_title: "Uninstall Perland Launcher",
    title: "UNINSTALL PERLAND LAUNCHER",
    removes_from: "Removes the app from",
    removes_also: "and its shortcuts, perland:// links and Settings entry.",
    delete_box: "Also delete my games, worlds, settings and sign-ins",
    uninstall: "UNINSTALL",
    uninstall_delete: "UNINSTALL AND DELETE",
    cancel: "Cancel",
    uninstalling: "UNINSTALLING…",
    removing_app: "Removing the app",
    deleting_data: "Deleting your data",
    uninstalled: "UNINSTALLED",
    done_kept: "Perland Launcher is uninstalled. Your games and worlds were kept in:",
    done_deleted:
        "Perland Launcher is uninstalled, and your games, worlds and sign-ins are deleted.",
    partly: "The app is uninstalled, but some of your data couldn't be deleted:",
    close: "CLOSE",
    sign_ins: "Saved sign-ins",
};

static UNINSTALL_ES: UninstallWords = UninstallWords {
    window_title: "Desinstalar Perland Launcher",
    title: "DESINSTALAR PERLAND LAUNCHER",
    removes_from: "Quita la app de",
    removes_also: "y sus accesos directos, enlaces perland:// y su entrada en Configuración.",
    delete_box: "También borrar mis juegos, mundos, ajustes e inicios de sesión",
    uninstall: "DESINSTALAR",
    uninstall_delete: "DESINSTALAR Y BORRAR",
    cancel: "Cancelar",
    uninstalling: "DESINSTALANDO…",
    removing_app: "Quitando la app",
    deleting_data: "Borrando tus datos",
    uninstalled: "DESINSTALADO",
    done_kept: "Perland Launcher está desinstalado. Tus juegos y mundos se conservaron en:",
    done_deleted:
        "Perland Launcher está desinstalado y se borraron tus juegos, mundos e inicios de sesión.",
    partly: "La app está desinstalada, pero no se pudo borrar parte de tus datos:",
    close: "CERRAR",
    sign_ins: "Inicios de sesión guardados",
};

pub fn uninstall_words(lang: Lang) -> &'static UninstallWords {
    match lang {
        Lang::En => &UNINSTALL_EN,
        Lang::Es => &UNINSTALL_ES,
    }
}

/// "850 MB", "3.2 GB" ("3,2 GB" in Spanish): MB under a GB, else GB to one decimal.
pub fn bytes_text(lang: Lang, bytes: u64) -> String {
    if bytes < 1_000_000_000 {
        return format!("{} MB", bytes / 1_000_000);
    }
    let gb = format!("{:.1}", bytes as f64 / 1e9);
    match lang {
        Lang::En => format!("{gb} GB"),
        Lang::Es => format!("{} GB", gb.replace('.', ",")),
    }
}

/// The options view's size line: "About 25 MB · For you only, no admin needed".
pub fn size_line(lang: Lang, installed_bytes: Option<u64>) -> String {
    let w = options_words(lang);
    match installed_bytes {
        Some(bytes) => format!(
            "{} {} · {}",
            w.about,
            crate::options::size_text(bytes),
            w.for_you
        ),
        None => w.for_you.into(),
    }
}

pub fn folder_problem_text(lang: Lang, problem: FolderProblem) -> &'static str {
    match (lang, problem) {
        (Lang::En, FolderProblem::NotWritable) => "You can't write to this folder. Pick another one.",
        (Lang::En, FolderProblem::NeedsAdmin) => {
            "This folder needs administrator rights. Pick one in your user folder or another drive."
        }
        (Lang::En, FolderProblem::NotEmpty) => {
            "There's already a Perland Launcher folder here with other files. Pick another folder."
        }
        (Lang::Es, FolderProblem::NotWritable) => "No puedes escribir en esta carpeta. Elige otra.",
        (Lang::Es, FolderProblem::NeedsAdmin) => {
            "Esta carpeta necesita permisos de administrador. Elige una en tu carpeta de usuario o en otro disco."
        }
        (Lang::Es, FolderProblem::NotEmpty) => {
            "Ya hay una carpeta Perland Launcher aquí con otros archivos. Elige otra carpeta."
        }
    }
}

#[cfg(test)]
pub(crate) fn all_screens() -> Vec<Screen> {
    let installing = |step, confirm_stop| Screen::Installing {
        step,
        progress: 0.5,
        confirm_stop,
    };
    vec![
        Screen::Ready(Offer::Install),
        Screen::Ready(Offer::Update {
            from: "0.1.0".into(),
        }),
        Screen::Ready(Offer::Reinstall),
        Screen::Ready(Offer::Newer),
        Screen::LauncherRunning,
        installing(Step::GettingReady, false),
        installing(Step::GettingReady, true),
        installing(Step::Installing, false),
        installing(Step::FinishingUp, false),
        Screen::Opening,
        Screen::Installed,
        Screen::Error(ErrorCode::PayloadMissing),
        Screen::Error(ErrorCode::TempUnwritable),
        Screen::Error(ErrorCode::InstallerFailed(2)),
        Screen::Error(ErrorCode::InstallNotFound),
        Screen::Error(ErrorCode::Unknown),
        Screen::Options {
            offer: Offer::Install,
            problem: None,
        },
        Screen::Options {
            offer: Offer::Install,
            problem: Some(FolderProblem::NotEmpty),
        },
        Screen::Options {
            offer: Offer::Install,
            problem: Some(FolderProblem::NeedsAdmin),
        },
        Screen::Options {
            offer: Offer::Install,
            problem: Some(FolderProblem::NotWritable),
        },
        Screen::Options {
            offer: Offer::Update {
                from: "0.1.0".into(),
            },
            problem: None,
        },
        Screen::Options {
            offer: Offer::Reinstall,
            problem: None,
        },
        Screen::Uninstall(UScreen::Confirm),
        Screen::Uninstall(UScreen::Removing {
            step: UStep::RemovingApp,
            progress: 0.3,
        }),
        Screen::Uninstall(UScreen::Removing {
            step: UStep::DeletingData,
            progress: 0.7,
        }),
        Screen::Uninstall(UScreen::Done { kept_data: true }),
        Screen::Uninstall(UScreen::Done { kept_data: false }),
        Screen::Uninstall(UScreen::Failed {
            code: 2,
            not_deleted: vec![],
        }),
        Screen::Uninstall(UScreen::Failed {
            code: 0,
            not_deleted: vec![
                "C:\\Users\\ana\\AppData\\Roaming\\PerlandLauncher".into(),
                "Saved sign-ins".into(),
            ],
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn langid_spanish_variants() {
        for id in [0x0c0a, 0x080a, 0x2c0a, 0x540a] {
            assert_eq!(lang_from_langid(id), Lang::Es, "{id:#x}");
        }
        for id in [0x0409, 0x0809, 0x0403, 0x0416, 0] {
            assert_eq!(lang_from_langid(id), Lang::En, "{id:#x}");
        }
    }

    #[test]
    fn every_screen_has_words_in_both_languages() {
        for lang in [Lang::En, Lang::Es] {
            for screen in all_screens() {
                assert!(!headline(lang, &screen).is_empty(), "{lang:?} {screen:?}");
                assert!(!detail(lang, &screen).is_empty(), "{lang:?} {screen:?}");
                let (primary, secondary) = button_labels(lang, &screen);
                for label in [primary, secondary].into_iter().flatten() {
                    assert!(!label.is_empty());
                }
            }
        }
    }

    #[test]
    fn buttons_per_screen() {
        let en = Lang::En;
        assert_eq!(
            button_labels(en, &Screen::Ready(Offer::Install)),
            (Some("INSTALL"), None)
        );
        assert_eq!(
            button_labels(
                en,
                &Screen::Ready(Offer::Update {
                    from: "0.1.0".into()
                })
            ),
            (Some("UPDATE"), None)
        );
        assert_eq!(
            button_labels(en, &Screen::Ready(Offer::Reinstall)),
            (Some("OPEN"), Some("Reinstall"))
        );
        assert_eq!(
            button_labels(en, &Screen::Ready(Offer::Newer)),
            (Some("OPEN"), None)
        );
        let installing = Screen::Installing {
            step: Step::Installing,
            progress: 0.4,
            confirm_stop: false,
        };
        assert_eq!(button_labels(en, &installing), (None, None));
        let stop = Screen::Installing {
            step: Step::GettingReady,
            progress: 0.0,
            confirm_stop: true,
        };
        assert_eq!(
            button_labels(en, &stop),
            (Some("STOP"), Some("Keep installing"))
        );
        assert_eq!(
            button_labels(Lang::Es, &Screen::Ready(Offer::Install)),
            (Some("INSTALAR"), None)
        );
        assert_eq!(
            button_labels(en, &Screen::Error(ErrorCode::Unknown)),
            (Some("TRY AGAIN"), Some("Copy details"))
        );
    }

    #[test]
    fn error_text_names_code() {
        assert!(error_text(Lang::En, &ErrorCode::InstallerFailed(2)).contains("(code 2)"));
        assert!(error_text(Lang::Es, &ErrorCode::InstallerFailed(1603)).contains("1603"));
    }

    #[test]
    fn version_lines() {
        assert_eq!(
            version_line(Lang::En, &Offer::Install, "0.2.0"),
            "Version 0.2.0 · Installs for you only, no admin needed"
        );
        let update = version_line(
            Lang::En,
            &Offer::Update {
                from: "0.1.0".into(),
            },
            "0.2.0",
        );
        assert!(
            update.contains("0.1.0") && update.contains("0.2.0"),
            "{update}"
        );
        assert!(version_line(Lang::Es, &Offer::Reinstall, "0.2.0").contains("0.2.0"));
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(bytes_text(Lang::En, 850_000_000), "850 MB");
        assert_eq!(bytes_text(Lang::En, 3_240_000_000), "3.2 GB");
        assert_eq!(bytes_text(Lang::Es, 3_240_000_000), "3,2 GB");
        assert_eq!(bytes_text(Lang::En, 0), "0 MB");
        assert_eq!(
            size_line(Lang::En, Some(25_000_000)),
            "About 25 MB · For you only, no admin needed"
        );
        assert_eq!(size_line(Lang::En, None), "For you only, no admin needed");
    }

    #[test]
    fn folder_problems_have_words() {
        for lang in [Lang::En, Lang::Es] {
            for p in [
                FolderProblem::NotWritable,
                FolderProblem::NeedsAdmin,
                FolderProblem::NotEmpty,
            ] {
                assert!(!folder_problem_text(lang, p).is_empty());
            }
        }
        assert!(folder_problem_text(Lang::En, FolderProblem::NeedsAdmin).contains("administrator"));
    }

    #[test]
    fn options_words_complete() {
        for lang in [Lang::En, Lang::Es] {
            let w = options_words(lang);
            assert_eq!(w.items.len(), 4);
            assert!(w.items.iter().all(|i| !i.is_empty()));
        }
    }

    #[test]
    fn uninstall_buttons() {
        let en = Lang::En;
        assert_eq!(
            button_labels(en, &Screen::Uninstall(UScreen::Confirm)),
            (Some("UNINSTALL"), Some("Cancel"))
        );
        assert_eq!(uninstall_words(en).uninstall_delete, "UNINSTALL AND DELETE");
        assert_eq!(
            button_labels(en, &Screen::Uninstall(UScreen::Done { kept_data: true })),
            (Some("CLOSE"), None)
        );
        assert_eq!(
            button_labels(
                en,
                &Screen::Uninstall(UScreen::Failed {
                    code: 2,
                    not_deleted: vec![]
                })
            ),
            (Some("CLOSE"), Some("Copy details"))
        );
        let removing = Screen::Uninstall(UScreen::Removing {
            step: UStep::RemovingApp,
            progress: 0.1,
        });
        assert_eq!(button_labels(en, &removing), (None, None));
        assert_eq!(
            button_labels(Lang::Es, &Screen::Uninstall(UScreen::Confirm)),
            (Some("DESINSTALAR"), Some("Cancelar"))
        );
    }

    #[test]
    fn options_buttons_follow_the_offer() {
        let opt = |offer| Screen::Options {
            offer,
            problem: None,
        };
        assert_eq!(
            button_labels(Lang::En, &opt(Offer::Install)),
            (Some("INSTALL"), Some("Back"))
        );
        assert_eq!(
            button_labels(
                Lang::En,
                &opt(Offer::Update {
                    from: "0.1.0".into()
                })
            ),
            (Some("UPDATE"), Some("Back"))
        );
        assert_eq!(
            button_labels(Lang::En, &opt(Offer::Reinstall)),
            (Some("REINSTALL"), Some("Back"))
        );
    }

    #[test]
    fn uninstall_failure_names_the_code() {
        let failed = Screen::Uninstall(UScreen::Failed {
            code: 1603,
            not_deleted: vec![],
        });
        assert!(detail(Lang::En, &failed).contains("1603"));
        let partial = Screen::Uninstall(UScreen::Failed {
            code: 0,
            not_deleted: vec!["X".into()],
        });
        assert!(!detail(Lang::En, &partial).contains("code"));
    }

    #[test]
    fn footer_while_installing_says_it_cant_stop() {
        let screen = Screen::Installing {
            step: Step::Installing,
            progress: 0.4,
            confirm_stop: false,
        };
        assert_eq!(
            footer(Lang::En, &screen, "0.2.0").as_deref(),
            Some("Almost there, this can't be stopped now")
        );
        let ready = Screen::Installing {
            step: Step::GettingReady,
            progress: 0.0,
            confirm_stop: false,
        };
        assert_eq!(footer(Lang::En, &ready, "0.2.0"), None);
    }
}
