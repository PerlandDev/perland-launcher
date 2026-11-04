//! Every word the setup shows, in English and Spanish (neutral Latin American, informal "tú").

use crate::model::{ErrorCode, Offer, Screen, Step};

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
        Screen::Options { .. } => s.brand,
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
        Screen::Options { .. } => s.tagline.into(),
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
        Screen::Options { .. } => (Some(s.install), None),
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
