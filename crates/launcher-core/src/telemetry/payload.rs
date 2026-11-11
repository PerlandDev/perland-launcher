//! What goes over the wire, exactly as the Engine reads it, and nothing else. Every setter
//! cuts to the contract's limits, so a report that exists is a report the Engine accepts.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::scrub::{is_public_host, Scrubber};
use crate::LauncherError;

pub const MESSAGE_MAX: usize = 500;
pub const CODE_MAX: usize = 64;
pub const STACK_MAX: usize = 8192;
pub const CONTEXT_KEYS_MAX: usize = 16;
pub const CONTEXT_KEY_MAX: usize = 32;
pub const CONTEXT_VALUE_MAX: usize = 200;
pub const COUNT_MAX: u32 = 1000;
/// The ping's `version` limit, the tighter of the two, used for both bodies.
pub const VERSION_MAX: usize = 32;
pub const ARCH_MAX: usize = 16;
pub const LOCALE_MAX: usize = 8;

/// `LauncherError` params that say what failed without saying whose it was.
const PARAMS: &[&str] = &[
    "version",
    "loader",
    "loader_version",
    "status",
    "item",
    "field",
    "step",
    "method",
    "component",
    "reason",
    "what",
    "min",
    "max",
    "count",
];

/// At most `max` bytes of `text`, cut on a character boundary. A byte limit is never looser
/// than the contract's character limit.
fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Printable text only: control characters would balloon as JSON escapes. Lines and tabs stay.
fn printable(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

fn valid_context_key(key: &str) -> bool {
    let mut chars = key.chars();
    key.len() <= CONTEXT_KEY_MAX
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    /// An error the window's own code threw.
    Exception,
    /// A promise in the window that nobody caught.
    UnhandledRejection,
    /// The launcher itself panicked.
    Panic,
    /// A `LauncherError` the player was shown.
    AppError,
    /// The game crashed; only the crash doctor's verdict is kept.
    GameCrash,
}

/// One thing that went wrong, already scrubbed and within every limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    kind: ReportKind,
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stack: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    context: BTreeMap<String, String>,
    #[serde(default = "one")]
    count: u32,
}

fn one() -> u32 {
    1
}

impl Report {
    /// `message` should already be scrubbed. An empty one becomes `(no message)`: the
    /// contract wants at least one character.
    pub fn new(kind: ReportKind, message: &str) -> Self {
        let message = printable(message);
        let message = cut(message.trim(), MESSAGE_MAX);
        Self {
            kind,
            message: if message.is_empty() {
                "(no message)".into()
            } else {
                message.into()
            },
            code: None,
            stack: None,
            context: BTreeMap::new(),
            count: 1,
        }
    }

    /// Lowercased, and anything outside `[a-z0-9_.:-]` becomes `_`. An empty code is none.
    pub fn code(mut self, code: &str) -> Self {
        let code: String = cut(code.trim(), CODE_MAX)
            .chars()
            .map(|c| c.to_ascii_lowercase())
            .map(|c| {
                if c.is_ascii_lowercase() || c.is_ascii_digit() || "_.:-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.code = (!code.is_empty()).then_some(code);
        self
    }

    pub fn stack(mut self, stack: &str) -> Self {
        let stack = printable(stack);
        let stack = cut(stack.trim_end(), STACK_MAX);
        self.stack = (!stack.trim().is_empty()).then(|| stack.to_string());
        self
    }

    /// Ignored when the key isn't one the contract allows, the value is empty, or there are
    /// sixteen already.
    pub fn context(mut self, key: &str, value: &str) -> Self {
        let value = printable(value);
        let value = cut(value.trim(), CONTEXT_VALUE_MAX);
        let room = self.context.len() < CONTEXT_KEYS_MAX || self.context.contains_key(key);
        if valid_context_key(key) && !value.is_empty() && room {
            self.context.insert(key.to_string(), value.to_string());
        }
        self
    }

    pub fn kind(&self) -> ReportKind {
        self.kind
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn code_str(&self) -> Option<&str> {
        self.code.as_deref()
    }
    pub fn stack_str(&self) -> Option<&str> {
        self.stack.as_deref()
    }
    pub fn context_map(&self) -> &BTreeMap<String, String> {
        &self.context
    }
    pub fn count(&self) -> u32 {
        self.count
    }

    /// The same report but for how many times it happened.
    pub fn same_as(&self, other: &Report) -> bool {
        self.kind == other.kind
            && self.message == other.message
            && self.code == other.code
            && self.stack == other.stack
            && self.context == other.context
    }

    pub(crate) fn add_count(&mut self, more: u32) {
        self.count = self.count.saturating_add(more).clamp(1, COUNT_MAX);
    }

    /// `count` less `sent`; `false` when nothing is left.
    pub(crate) fn sub_count(&mut self, sent: u32) -> bool {
        self.count = self.count.saturating_sub(sent);
        self.count > 0
    }

    /// The report rebuilt through its own setters: what a report read from disk goes through,
    /// so an edited file can't carry anything over a limit.
    pub(crate) fn rebuilt(&self) -> Report {
        let mut out = Report::new(self.kind, &self.message);
        if let Some(code) = &self.code {
            out = out.code(code);
        }
        if let Some(stack) = &self.stack {
            out = out.stack(stack);
        }
        for (key, value) in &self.context {
            out = out.context(key, value);
        }
        out.count = self.count.clamp(1, COUNT_MAX);
        out
    }
}

/// The report for an error the player was shown: its stable code, its plain message, its
/// category, and what is safe of its detail and params. `command` is the UI command that
/// failed, a name from the launcher's own code.
pub fn report_for_error(
    error: &LauncherError,
    command: Option<&str>,
    scrubber: &Scrubber,
) -> Report {
    let wire = |value: &dyn erased::Wire| value.wire();
    let mut report = Report::new(ReportKind::AppError, &scrubber.scrub(&error.message))
        .code(&wire(&error.code))
        .context("category", &wire(&error.category));
    if let Some(command) = command {
        report = report.context("command", command);
    }
    if let Some(detail) = &error.detail {
        report = report.context("detail", &scrubber.scrub(detail));
    }
    for (key, value) in &error.params {
        if PARAMS.contains(&key.as_str()) {
            report = report.context(&format!("p_{key}"), &scrubber.scrub(value));
        } else if key == "host" && is_public_host(value) {
            report = report.context("p_host", value);
        }
    }
    report
}

/// The snake_case name serde gives a unit enum variant.
mod erased {
    pub trait Wire {
        fn wire(&self) -> String;
    }
    impl<T: serde::Serialize> Wire for T {
        fn wire(&self) -> String {
            serde_json::to_value(self)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        }
    }
}

/// What both bodies say about the launcher that sends them. Nothing here tells two
/// computers apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub version: String,
    /// `windows`, `linux`, `macos` or `other`.
    pub os: &'static str,
    pub arch: String,
    pub locale: String,
}

impl Client {
    /// `locale` is a language tag such as `es` or `en-US`; anything else becomes `und`.
    pub fn new(version: &str, os: &str, arch: &str, locale: &str) -> Self {
        let os = match os {
            "windows" => "windows",
            "linux" => "linux",
            "macos" => "macos",
            _ => "other",
        };
        let tag = |text: &str, max: usize| -> String {
            cut(text.trim(), max)
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
                .collect()
        };
        // `es_MX.UTF-8` is `es-MX`.
        let locale: String = locale
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .chars()
            .map(|c| if c == '_' { '-' } else { c })
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(LOCALE_MAX)
            .collect();
        let or_unknown = |text: String| {
            if text.is_empty() {
                "unknown".to_string()
            } else {
                text
            }
        };
        Self {
            version: or_unknown(tag(version, VERSION_MAX)),
            os,
            arch: or_unknown(tag(arch, ARCH_MAX)),
            locale: if locale.is_empty() {
                "und".into()
            } else {
                locale
            },
        }
    }

    /// This build on this computer.
    pub fn current(version: &str, locale: &str) -> Self {
        Self::new(
            version,
            std::env::consts::OS,
            std::env::consts::ARCH,
            locale,
        )
    }
}

/// The body of `POST /api/v1/telemetry/errors`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorBatch {
    source: &'static str,
    version: String,
    os: &'static str,
    arch: String,
    locale: String,
    reports: Vec<Report>,
}

impl ErrorBatch {
    /// `reports` should hold 1 to 20 (see `State::batches`).
    pub fn new(client: &Client, reports: Vec<Report>) -> Self {
        Self {
            source: "launcher",
            version: client.version.clone(),
            os: client.os,
            arch: client.arch.clone(),
            locale: client.locale.clone(),
            reports,
        }
    }

    pub fn reports(&self) -> &[Report] {
        &self.reports
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PingChannel {
    Stable,
    Beta,
    Dev,
}

/// The body of `POST /api/v1/telemetry/launcher/ping`. No field tells two installs apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ping {
    pub version: String,
    pub channel: PingChannel,
    pub os: &'static str,
    pub arch: String,
    pub locale: String,
    pub signed_in: bool,
    pub first_ever: bool,
    pub first_this_week: bool,
    pub first_this_month: bool,
    pub install_week: String,
    pub counters: BTreeMap<String, u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ErrorCode, LauncherError};
    use serde_json::json;

    #[test]
    fn every_field_is_cut_to_the_contract() {
        let long = "é".repeat(9000);
        let report = Report::new(ReportKind::Panic, &long)
            .code(&"A b/".repeat(40))
            .stack(&long)
            .context("ok_key1", &long)
            .context("1bad", "x")
            .context("bad-key", "x")
            .context("", "x")
            .context(&"k".repeat(33), "x")
            .context("empty", "  ");
        assert_eq!(report.message().len(), 500);
        assert!(report.message().chars().all(|c| c == 'é'));
        let code = report.code_str().unwrap();
        assert_eq!(code.len(), 64);
        assert!(code.starts_with("a_b_a_b_"), "{code}");
        assert_eq!(report.stack_str().unwrap().len(), 8192);
        assert_eq!(report.context_map().len(), 1);
        assert_eq!(report.context_map()["ok_key1"].len(), 200);

        let mut many = Report::new(ReportKind::Exception, "x");
        for n in 0..30 {
            many = many.context(&format!("k{n}"), "v");
        }
        assert_eq!(many.context_map().len(), 16);
    }

    #[test]
    fn empty_and_control_text_is_made_harmless() {
        let report = Report::new(ReportKind::Exception, " \u{0}\u{7} ")
            .code("  ")
            .stack(" \n ");
        assert_eq!(report.message(), "(no message)");
        assert_eq!(report.code_str(), None);
        assert_eq!(report.stack_str(), None);
        let lines = Report::new(ReportKind::Exception, "a\u{1b}[0mb").stack("one\n\ttwo\r\n");
        assert_eq!(lines.message(), "a[0mb");
        assert_eq!(lines.stack_str(), Some("one\n\ttwo"));
    }

    #[test]
    fn a_report_read_back_is_cut_again() {
        let edited = json!({
            "kind": "app_error",
            "message": "m".repeat(900),
            "code": "NOT A CODE",
            "context": { "bad key": "x", "fine": "y" },
            "count": 999_999
        });
        let report: Report = serde_json::from_value(edited).unwrap();
        let rebuilt = report.rebuilt();
        assert_eq!(rebuilt.message().len(), 500);
        assert_eq!(rebuilt.code_str(), Some("not_a_code"));
        assert_eq!(rebuilt.context_map().len(), 1);
        assert_eq!(rebuilt.count(), 1000);
    }

    #[test]
    fn the_error_batch_is_the_contract() {
        let client = Client::new("0.2.15", "linux", "x86_64", "es");
        let report = Report::new(ReportKind::AppError, "Couldn't save settings")
            .code("disk_write_failed")
            .context("category", "disk");
        let body = serde_json::to_value(ErrorBatch::new(&client, vec![report])).unwrap();
        assert_eq!(
            body,
            json!({
                "source": "launcher",
                "version": "0.2.15",
                "os": "linux",
                "arch": "x86_64",
                "locale": "es",
                "reports": [{
                    "kind": "app_error",
                    "message": "Couldn't save settings",
                    "code": "disk_write_failed",
                    "context": { "category": "disk" },
                    "count": 1
                }]
            })
        );
        for (kind, wire) in [
            (ReportKind::Exception, "exception"),
            (ReportKind::UnhandledRejection, "unhandled_rejection"),
            (ReportKind::Panic, "panic"),
            (ReportKind::AppError, "app_error"),
            (ReportKind::GameCrash, "game_crash"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), wire);
        }
    }

    #[test]
    fn the_ping_is_the_contract() {
        let ping = Ping {
            version: "0.2.15".into(),
            channel: PingChannel::Stable,
            os: "windows",
            arch: "x86_64".into(),
            locale: "es".into(),
            signed_in: true,
            first_ever: false,
            first_this_week: true,
            first_this_month: false,
            install_week: "2026-W46".into(),
            counters: BTreeMap::from([("game_launches".to_string(), 4)]),
        };
        assert_eq!(
            serde_json::to_value(&ping).unwrap(),
            json!({
                "version": "0.2.15",
                "channel": "stable",
                "os": "windows",
                "arch": "x86_64",
                "locale": "es",
                "signedIn": true,
                "firstEver": false,
                "firstThisWeek": true,
                "firstThisMonth": false,
                "installWeek": "2026-W46",
                "counters": { "game_launches": 4 }
            })
        );
        assert_eq!(serde_json::to_value(PingChannel::Beta).unwrap(), "beta");
        assert_eq!(serde_json::to_value(PingChannel::Dev).unwrap(), "dev");
    }

    #[test]
    fn version_arch_and_locale_hold_only_what_the_engine_accepts() {
        let allowed = |text: &str| {
            !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
        };
        for (version, arch, locale) in [
            ("0.9.3-beta.1", "x86_64", "es-419"),
            ("0.2.15 (dev build)/x", "x86 64/v2", "es_MX.UTF-8"),
            ("1.0.0+build 7", "aarch64 (arm)", "en US"),
            ("versión 1", "ärch", "español"),
            ("", "", ""),
        ] {
            let client = Client::new(version, "linux", arch, locale);
            for (field, value) in [("arch", &client.arch), ("locale", &client.locale)] {
                assert!(allowed(value), "{field}: {value:?}");
            }
            assert!(
                client.version.is_empty() || allowed(&client.version),
                "{:?}",
                client.version
            );
        }
        let client = Client::new("0.9.3-beta.1", "linux", "x86_64", "es-419");
        assert_eq!(
            (
                client.version.as_str(),
                client.arch.as_str(),
                client.locale.as_str()
            ),
            ("0.9.3-beta.1", "x86_64", "es-419")
        );
        assert_eq!(Client::new("", "linux", "", "").arch, "unknown");
        assert_eq!(Client::new("", "linux", "", "").version, "unknown");
    }

    #[test]
    fn the_client_says_only_what_the_contract_allows() {
        let client = Client::new(&"1".repeat(80), "freebsd", &"a".repeat(40), "es_MX.UTF-8");
        assert_eq!(client.version.len(), 32);
        assert_eq!(client.os, "other");
        assert_eq!(client.arch.len(), 16);
        assert_eq!(client.locale, "es-MX");
        assert_eq!(Client::new("1", "macos", "aarch64", "").locale, "und");
        assert_eq!(
            Client::new("1", "windows", "x86_64", "en-US").locale,
            "en-US"
        );
    }

    /// Everything private a realistic error could carry.
    const PRIVATE: &[&str] = &[
        "moises",
        "Ana Maria",
        "PedroGamer99",
        "pedro@correo.example",
        "tok_9f8e7d6c5b4a",
        "eyJhbGciOiJIUzI1NiJ9.session",
        "pedrocraft",
        "25565",
        "Servidor de Pedro",
        "AppData",
        "/home/",
        "C:\\",
        "C:/",
    ];

    fn scrubbers() -> [Scrubber; 2] {
        let known = |s: Scrubber| {
            s.with_secret("PedroGamer99")
                .with_secret("pedro@correo.example")
                .with_secret("tok_9f8e7d6c5b4a")
                .with_secret("eyJhbGciOiJIUzI1NiJ9.session")
                .with_secret("Servidor de Pedro")
        };
        [
            known(
                Scrubber::new()
                    .with_data_root("/home/moises/.local/share/PerlandLauncher")
                    .with_home("/home/moises"),
            ),
            known(
                Scrubber::new()
                    .with_data_root(r"C:\Users\Ana Maria\AppData\Roaming\PerlandLauncher")
                    .with_home(r"C:\Users\Ana Maria"),
            ),
        ]
    }

    fn realistic_errors() -> Vec<LauncherError> {
        vec![
            LauncherError::network(ErrorCode::HostUnreachable, "Couldn't reach play.pedrocraft.net")
                .with_param("host", "play.pedrocraft.net")
                .with_param("status", 502)
                .with_detail(
                    "error sending request for url (https://play.pedrocraft.net/api/v1/invites/tok_9f8e7d6c5b4a/pack?t=eyJhbGciOiJIUzI1NiJ9.session): connection refused 203.0.113.9:25565",
                ),
            LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save Servidor de Pedro")
                .with_param("item", "instance")
                .with_param("name", "Servidor de Pedro")
                .with_param("id", "servidor-de-pedro")
                .with_param("file", "pedrocraft-mod.jar")
                .with_param("path", "/home/moises/x")
                .with_detail(
                    "failed to write /home/moises/.local/share/PerlandLauncher/instances/servidor-de-pedro/instance.json: Permission denied (os error 13)",
                ),
            LauncherError::disk(ErrorCode::DiskIo, "Couldn't read or write a launcher file")
                .with_detail(
                    r"C:\Users\Ana Maria\AppData\Roaming\PerlandLauncher\instances\Servidor de Pedro\mods\a.jar: Access is denied. (os error 5)",
                ),
            LauncherError::disk(ErrorCode::DiskIo, "Couldn't read or write a launcher file")
                .with_detail(
                    "C:\\\\Users\\\\Ana Maria\\\\Downloads\\\\Servidor de Pedro.mrpack and C:/Users/Ana Maria/Desktop/x",
                ),
            LauncherError::auth(ErrorCode::SignInFailed, "Couldn't sign in PedroGamer99")
                .with_param("uuid", "069a79f4-44e9-4726-a5be-fca90e38aaf5")
                .with_param("address", "play.pedrocraft.net:25565")
                .with_param("url", "https://play.pedrocraft.net/x")
                .with_param("value", "pedro@correo.example")
                .with_detail("account pedro@correo.example (PedroGamer99) token eyJhbGciOiJIUzI1NiJ9.session rejected"),
            LauncherError::game(ErrorCode::ServerWakeFailed, "The server didn't wake up")
                .with_param("host", "mc.pedrocraft.gg")
                .with_detail("mc.pedrocraft.gg did not answer; tried [2001:db8::1]:25565"),
        ]
    }

    #[test]
    fn a_report_for_a_realistic_error_holds_nothing_private() {
        for scrubber in scrubbers() {
            for error in realistic_errors() {
                let report = report_for_error(&error, Some("play"), &scrubber);
                let sent = serde_json::to_string(&ErrorBatch::new(
                    &Client::new("0.2.15", "windows", "x86_64", "es"),
                    vec![report],
                ))
                .unwrap();
                for private in PRIVATE {
                    assert!(
                        !sent.to_lowercase().contains(&private.to_lowercase()),
                        "{private} left in {sent}"
                    );
                }
                assert!(!sent.contains("069a79f4"), "{sent}");
            }
        }
    }

    #[test]
    fn a_report_for_an_error_keeps_what_helps() {
        let scrubber = Scrubber::new().with_home("/home/moises");
        let error =
            LauncherError::network(ErrorCode::HostTrouble, "The download server has trouble")
                .with_param("host", "api.modrinth.com")
                .with_param("status", 502)
                .with_param("version", "1.20.1")
                .with_detail("GET https://api.modrinth.com/v2/project/sodium answered 502");
        let report = report_for_error(&error, Some("browse_search"), &scrubber);
        assert_eq!(report.kind(), ReportKind::AppError);
        assert_eq!(report.code_str(), Some("host_trouble"));
        assert_eq!(report.message(), "The download server has trouble");
        let context = report.context_map();
        assert_eq!(context["category"], "network");
        assert_eq!(context["command"], "browse_search");
        assert_eq!(context["p_host"], "api.modrinth.com");
        assert_eq!(context["p_status"], "502");
        assert_eq!(context["p_version"], "1.20.1");
        assert_eq!(
            context["detail"],
            "GET https://api.modrinth.com answered 502"
        );
        assert_eq!(report.stack_str(), None);
    }
}
