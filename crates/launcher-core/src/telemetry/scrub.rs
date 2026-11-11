//! Takes everything private out of a text before it is queued: known names and tokens, the
//! data folder and the home folder, emails, addresses, and every absolute path. What is left
//! are placeholders (`<home>`, `<data>/instances`, `<path>`, `<url>`, `<address>`, `<email>`,
//! `<user>`, `<redacted>`) and the words around them.
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::{download::redact_url, launch::Redactor};

/// The longest text scrubbed; the rest is cut first. Far above any field's own limit.
const INPUT_CAP: usize = 32 << 10;

/// Stand-ins while scrubbing, from Unicode's private use area. Any already in the input are
/// removed first, so a text can't smuggle one in.
const DATA: char = '\u{E000}';
const HOME: char = '\u{E001}';
const STASH: char = '\u{E002}';
const STASH_END: char = '\u{E003}';

/// Folders and files the launcher itself names inside its data folder: safe to keep.
const DATA_TOP: &[&str] = &[
    "instances",
    "libraries",
    "assets",
    "versions",
    "java",
    "meta",
    "logs",
    "cache",
    "skins",
    "settings.json",
    "accounts.json",
    "telemetry.json",
    "instance-order.json",
];

/// Public services the launcher talks to. A URL on one of these keeps its host and path
/// (never its query); any other host is the player's business.
const PUBLIC_HOSTS: &[&str] = &[
    "mojang.com",
    "minecraft.net",
    "minecraftservices.com",
    "modrinth.com",
    "curseforge.com",
    "forgecdn.net",
    "fabricmc.net",
    "quiltmc.org",
    "neoforged.net",
    "minecraftforge.net",
    "microsoftonline.com",
    "live.com",
    "xboxlive.com",
    "microsoft.com",
    "github.com",
    "githubusercontent.com",
    "adoptium.net",
];

/// Endings that make `name.ext:12` a place in a file, not an address.
const FILE_ENDINGS: &[&str] = &[
    "rs", "js", "mjs", "cjs", "ts", "tsx", "jsx", "java", "kt", "json", "toml", "jar", "class",
    "txt", "log", "zip", "png", "dat", "cfg", "mrpack", "exe", "dll", "so", "html", "css", "md",
];

/// Endings that make a dotted word a host name.
const HOST_ENDINGS: &[&str] = &[
    "com", "net", "org", "io", "gg", "online", "xyz", "me", "co", "dev", "fun", "club", "es", "mx",
    "ar", "cl", "pe", "br", "uk", "de", "fr", "eu", "us", "ca", "host", "pro", "live", "world",
    "games", "land", "space", "site", "cc", "tv", "info", "app", "cloud", "network", "lat",
    "local", "lan",
];

/// Is `host` one of [`PUBLIC_HOSTS`] or under one?
pub fn is_public_host(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    PUBLIC_HOSTS.iter().any(|known| {
        host == *known
            || host
                .strip_suffix(known)
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a fixed pattern compiles")
}

static URL: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?i)\b[a-z][a-z0-9+.-]*://[^\s"'`<>\u{E000}-\u{E003}]+"#));
/// An absolute path: a stand-in for a known folder, a drive, a UNC share, or `/…` after
/// something that can come before a path. Then every following run of words that ends in one
/// with a separator in it, so `C:\Users\Ana Maria\x` goes whole. It may take a few words too
/// many ("/a/b and then c/d"), never too few.
static PATH: LazyLock<Regex> = LazyLock::new(|| {
    let seg = r#"[^\s"'`<>|*?:\u{E000}-\u{E003}]"#;
    // A word of a folder's name: up to three may sit between two separators ("Mi disco de juegos").
    let word = r#"[^\s"'`<>|*?:/\\\u{E000}-\u{E003}]"#;
    re(&format!(
        r#"(?P<lead>^|[\s"'`(=\[{{,;:>])(?P<path>(?:[\u{{E000}}\u{{E001}}]|[A-Za-z]:[\\/]|\\\\|/{seg}){seg}*(?:(?: {word}+){{0,3}} {seg}*[\\/]{seg}*)*)"#
    ))
});
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| re(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+"));
static DOTTED: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+(?P<port>:\d{1,5})?"));
static IPV6: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\[[0-9A-Fa-f:.]{2,}\](?::\d{1,5})?|\b(?:[0-9A-Fa-f]{1,4}:){3,7}[0-9A-Fa-f]{1,4}\b")
});
static CARGO: LazyLock<Regex> = LazyLock::new(|| re(r"registry/src/[^/]+/(?P<tail>.+)$"));
static RUSTC: LazyLock<Regex> = LazyLock::new(|| re(r"/rustc/[0-9a-f]+/(?P<tail>.+)$"));

/// A known folder in each spelling it can turn up in.
struct Root {
    forms: Vec<Regex>,
    mark: char,
}

impl Root {
    fn new(path: &str, mark: char) -> Option<Self> {
        let path = path.trim_end_matches(['/', '\\']);
        // "/" or "C:" alone would swallow every path.
        if path.len() < 4 {
            return None;
        }
        let mut spellings = vec![path.to_string()];
        if path.contains('\\') {
            // Debug-formatted text doubles every backslash, and Java prints forward slashes.
            spellings.push(path.replace('\\', "\\\\"));
            spellings.push(path.replace('\\', "/"));
        }
        // Longest first, so the doubled form isn't half-eaten by the plain one.
        spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
        let windows = path.contains('\\') || path.as_bytes().get(1) == Some(&b':');
        let forms = spellings
            .iter()
            .filter_map(|s| {
                let flag = if windows { "(?i)" } else { "" };
                Regex::new(&format!("{flag}{}", regex::escape(s))).ok()
            })
            .collect();
        Some(Self { forms, mark })
    }
}

/// See the module's own words. Built once per report from what the launcher knows right then.
#[derive(Default)]
pub struct Scrubber {
    /// Long secrets, replaced wherever they appear.
    secrets: Redactor,
    /// Names: matched without regard to case, short ones only as whole words.
    names: Vec<Regex>,
    roots: Vec<Root>,
    user: Option<Regex>,
}

// Manual Debug: this holds what must never print.
impl std::fmt::Debug for Scrubber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scrubber")
            .field("names", &self.names.len())
            .field("roots", &self.roots.len())
            .finish_non_exhaustive()
    }
}

impl Scrubber {
    pub fn new() -> Self {
        Self::default()
    }

    /// The home folder and user name from the environment, plus the launcher's data folder.
    pub fn for_current_user(data_root: &std::path::Path) -> Self {
        let env = |name: &str| std::env::var(name).unwrap_or_default();
        let home = [env("USERPROFILE"), env("HOME")]
            .into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        let user = [env("USERNAME"), env("USER")]
            .into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        Self::new()
            .with_data_root(&data_root.to_string_lossy())
            .with_home(&home)
            .with_user(&user)
    }

    /// The launcher's data folder: becomes `<data>`, keeping only a top-level name the
    /// launcher itself chose (`<data>/instances`).
    pub fn with_data_root(mut self, root: &str) -> Self {
        // First, so it wins over a home folder it sits inside.
        if let Some(root) = Root::new(root, DATA) {
            self.roots.insert(0, root);
        }
        self
    }

    /// The home folder: becomes `<home>`. Its last part is taken as a user name too.
    pub fn with_home(mut self, home: &str) -> Self {
        if let Some(root) = Root::new(home, HOME) {
            self.roots.push(root);
            let name = home
                .trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("");
            if self.user.is_none() {
                self = self.with_user(name);
            }
        }
        self
    }

    /// The operating system's user name: becomes `<user>` wherever it stands as a word.
    pub fn with_user(mut self, user: &str) -> Self {
        let user = user.trim();
        if user.chars().count() >= 2 {
            self.user = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(user))).ok();
        }
        self
    }

    /// Something that must never leave: a token, an account or player name, an email, a
    /// server's name or address. Five characters or more go wherever they appear; shorter
    /// ones (down to two) only as whole words, so a player called "Al" doesn't mask "Also".
    pub fn with_secret(mut self, secret: &str) -> Self {
        self.add_secret(secret);
        self
    }

    pub fn add_secret(&mut self, secret: &str) {
        let secret = secret.trim();
        let len = secret.chars().count();
        if len < 2 {
            return;
        }
        let escaped = regex::escape(secret);
        let pattern = if len >= 5 {
            // Exact matches first, through the launcher's own redactor.
            self.secrets = std::mem::take(&mut self.secrets).with_secret(secret);
            format!("(?i){escaped}")
        } else {
            format!(r"(?i)\b{escaped}\b")
        };
        if let Ok(name) = Regex::new(&pattern) {
            self.names.push(name);
        }
    }

    /// Known secrets and names out of `text`.
    fn mask(&self, text: &str) -> String {
        let mut text = self.secrets.redact(text);
        for name in &self.names {
            text = name.replace_all(&text, "<redacted>").into_owned();
        }
        text
    }

    pub fn scrub(&self, input: &str) -> String {
        let mut text: String = input
            .chars()
            .filter(|c| !(DATA..=STASH_END).contains(c))
            .take(INPUT_CAP)
            .collect();
        // Shapes first (a URL, a path, an address are recognised whole), then what the
        // launcher knows by name. Finished pieces wait in `stash`, where no later step can
        // touch them; a piece that keeps some of its text is masked on its way in.
        let mut stash: Vec<String> = Vec::new();
        let mut keep = |piece: String| {
            stash.push(self.mask(&piece));
            format!("{STASH}{}{STASH_END}", stash.len() - 1)
        };
        text = URL
            .replace_all(&text, |found: &Captures| keep(url_placeholder(&found[0])))
            .into_owned();
        for root in &self.roots {
            for form in &root.forms {
                text = form
                    .replace_all(&text, root.mark.to_string().as_str())
                    .into_owned();
            }
        }
        text = PATH
            .replace_all(&text, |found: &Captures| {
                format!(
                    "{}{}",
                    &found["lead"],
                    keep(path_placeholder(&found["path"]))
                )
            })
            .into_owned();
        // A known folder named without a path after it and not at a path's start.
        text = text.replace(DATA, "<data>").replace(HOME, "<home>");
        text = EMAIL.replace_all(&text, "<email>").into_owned();
        text = IPV6.replace_all(&text, "<address>").into_owned();
        text = DOTTED
            .replace_all(&text, |found: &Captures| {
                dotted_placeholder(&found[0], found.name("port").is_some())
            })
            .into_owned();
        text = self.mask(&text);
        if let Some(user) = &self.user {
            text = user.replace_all(&text, "<user>").into_owned();
        }
        // Put the finished pieces back. A stand-in is never inside another, so one pass does.
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(start) = rest.find(STASH) {
            out.push_str(&rest[..start]);
            let after = &rest[start + STASH.len_utf8()..];
            let Some(end) = after.find(STASH_END) else {
                rest = after;
                continue;
            };
            if let Some(piece) = after[..end]
                .parse::<usize>()
                .ok()
                .and_then(|i| stash.get(i))
            {
                out.push_str(piece);
            }
            rest = &after[end + STASH_END.len_utf8()..];
        }
        out.push_str(rest);
        out
    }
}

/// A URL on a public service keeps host and path; the webview's own pages become `<app>/…`;
/// anything else is `<url>`.
fn url_placeholder(raw: &str) -> String {
    // Punctuation that ended the sentence, not the URL.
    let trimmed = raw.trim_end_matches(['.', ',', ';', ')', ']', '}']);
    let tail = &raw[trimmed.len()..];
    let Ok(url) = reqwest::Url::parse(trimmed) else {
        return format!("<url>{tail}");
    };
    let host = url.host_str().unwrap_or("");
    let own_page = url.scheme() == "tauri"
        || host.eq_ignore_ascii_case("tauri.localhost")
        || (matches!(url.scheme(), "http" | "https")
            && host.eq_ignore_ascii_case("localhost")
            && url.port() == Some(1420));
    if own_page {
        // A stack frame: keep the file and the `:line:column` after it.
        return format!("<app>{}{tail}", url.path());
    }
    if matches!(url.scheme(), "http" | "https") && is_public_host(host) {
        return format!("{}{tail}", redact_url(trimmed));
    }
    format!("<url>{tail}")
}

fn path_placeholder(path: &str) -> String {
    let unix = path.replace("\\\\", "/").replace('\\', "/");
    if let Some(rest) = unix.strip_prefix(DATA) {
        let top = rest.trim_start_matches('/').split('/').next().unwrap_or("");
        return if DATA_TOP.contains(&top) {
            format!("<data>/{top}")
        } else {
            "<data>".into()
        };
    }
    if unix.starts_with(HOME) {
        return "<home>".into();
    }
    // Where the build machine kept a dependency or the standard library: the same for every
    // player, and what makes a backtrace readable.
    if let Some(found) = CARGO.captures(&unix) {
        return format!("<cargo>/{}", &found["tail"]);
    }
    if let Some(found) = RUSTC.captures(&unix) {
        return format!("<rustc>/{}", &found["tail"]);
    }
    "<path>".into()
}

/// `a.b.c` with or without a port: an IPv4 address, a host with a port, or a word that ends
/// like a host name is `<address>`. File names, versions and Java packages stay.
fn dotted_placeholder(found: &str, has_port: bool) -> String {
    let name = found.split(':').next().unwrap_or(found);
    let labels: Vec<&str> = name.split('.').collect();
    let last = labels.last().copied().unwrap_or("").to_ascii_lowercase();
    let ipv4 = labels.len() == 4
        && labels
            .iter()
            .all(|l| l.len() <= 3 && !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()));
    let is_file = FILE_ENDINGS.contains(&last.as_str());
    let is_host = HOST_ENDINGS.contains(&last.as_str());
    if ipv4 || (has_port && !is_file) || (is_host && !is_public_host(name)) {
        "<address>".into()
    } else {
        found.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows() -> Scrubber {
        Scrubber::new()
            .with_data_root(r"C:\Users\Ana Maria\AppData\Roaming\PerlandLauncher")
            .with_home(r"C:\Users\Ana Maria")
    }

    fn linux() -> Scrubber {
        Scrubber::new()
            .with_data_root("/home/moises/.local/share/PerlandLauncher")
            .with_home("/home/moises")
    }

    #[test]
    fn a_windows_home_goes_in_every_spelling() {
        let s = windows();
        for text in [
            r"open C:\Users\Ana Maria\Downloads\pack.mrpack failed",
            r"open C:\\Users\\Ana Maria\\Downloads\\pack.mrpack failed",
            "open C:/Users/Ana Maria/Downloads/pack.mrpack failed",
            r"open c:\users\ana maria\Downloads\pack.mrpack failed",
        ] {
            assert_eq!(s.scrub(text), "open <home> failed", "{text}");
        }
    }

    #[test]
    fn the_data_folder_keeps_only_a_name_the_launcher_chose() {
        let s = windows();
        assert_eq!(
            s.scrub(r"C:\Users\Ana Maria\AppData\Roaming\PerlandLauncher\instances\Servidor de Pedro\mods\x.jar: denied"),
            "<data>/instances: denied"
        );
        assert_eq!(
            s.scrub(r"C:\Users\Ana Maria\AppData\Roaming\PerlandLauncher\Mis cosas\x.txt"),
            "<data>"
        );
        let l = linux();
        assert_eq!(
            l.scrub(
                "can't read /home/moises/.local/share/PerlandLauncher/settings.json (os error 13)"
            ),
            "can't read <data>/settings.json (os error 13)"
        );
        assert_eq!(
            l.scrub(
                "/home/moises/.local/share/PerlandLauncher/instances/smp-de-juan/instance.json"
            ),
            "<data>/instances"
        );
    }

    #[test]
    fn a_linux_home_and_the_user_name_go() {
        let s = linux();
        assert_eq!(
            s.scrub("No such file: /home/moises/Descargas/mi skin.png"),
            "No such file: <home> skin.png"
        );
        assert_eq!(
            s.scrub("user moises has no rights"),
            "user <user> has no rights"
        );
        // Only as a word.
        assert_eq!(s.scrub("moisesland"), "moisesland");
    }

    #[test]
    fn any_other_absolute_path_is_a_placeholder() {
        let s = linux();
        for (text, want) in [
            ("open /media/disco/Juegos/a.jar: gone", "open <path>: gone"),
            ("open '/media/Mi Disco/a.jar'", "open '<path>'"),
            ("open /media/Mi Disco/Juegos/a.jar now", "open <path> now"),
            (r"open D:\Juegos\Minecraft de Luis\a.jar", "open <path>"),
            (r"open \\nas\luis\a.jar", "open <path>"),
            ("path=/opt/x", "path=<path>"),
            ("file:/home/otro/x", "file:<path>"),
            (r"D:\Games (old)\Users\luis\x now", "<path> now"),
        ] {
            assert_eq!(s.scrub(text), want, "{text}");
        }
        // Not paths.
        for text in ["and/or", "50 KB/s", "application/json", "1/2 done"] {
            assert_eq!(s.scrub(text), text);
        }
    }

    #[test]
    fn build_machine_paths_keep_the_part_every_player_shares() {
        let s = linux();
        assert_eq!(
            s.scrub("at /home/runner/.cargo/registry/src/index.crates.io-6f17d22bba15001f/tokio-1.38.0/src/runtime/task.rs:120:5"),
            "at <cargo>/tokio-1.38.0/src/runtime/task.rs:120:5"
        );
        assert_eq!(
            s.scrub(r"at C:\Users\runneradmin\.cargo\registry\src\index.crates.io-6f17d22bba15001f\serde-1.0.0\src\de.rs:7"),
            "at <cargo>/serde-1.0.0/src/de.rs:7"
        );
        assert_eq!(
            s.scrub(
                "at /rustc/2d8144b78aabbccddeeff00112233445566778899/library/core/src/option.rs:1"
            ),
            "at <rustc>/library/core/src/option.rs:1"
        );
        // A relative source path is the launcher's own code.
        assert_eq!(
            s.scrub("panicked at crates/launcher-app/src/runs.rs:120:5"),
            "panicked at crates/launcher-app/src/runs.rs:120:5"
        );
    }

    #[test]
    fn urls_keep_nothing_private() {
        let s = linux();
        assert_eq!(
            s.scrub("GET https://api.modrinth.com/v2/project/sodium?token=abc#x failed."),
            "GET https://api.modrinth.com/v2/project/sodium failed."
        );
        assert_eq!(
            s.scrub("error sending request for url (https://play.pedrocraft.net/api/v1/invites/tok_123/pack)"),
            "error sending request for url (<url>)"
        );
        assert_eq!(s.scrub("http://user:pw@10.0.0.7:8080/x"), "<url>");
        assert_eq!(s.scrub("file:///home/moises/x.txt"), "<url>");
        assert_eq!(
            s.scrub("at render (http://tauri.localhost/assets/index-C3x.js:12:3456)"),
            "at render (<app>/assets/index-C3x.js:12:3456)"
        );
        assert_eq!(
            s.scrub("at tauri://localhost/assets/index-C3x.js:1:2"),
            "at <app>/assets/index-C3x.js:1:2"
        );
    }

    #[test]
    fn emails_and_addresses_go() {
        let s = Scrubber::new();
        assert_eq!(
            s.scrub("sent to ana.maria+mc@correo.example.mx"),
            "sent to <email>"
        );
        for (text, want) in [
            (
                "connect play.pedrocraft.net:25565 refused",
                "connect <address> refused",
            ),
            (
                "connect 192.168.1.20:25565 refused",
                "connect <address> refused",
            ),
            ("connect 10.0.0.7 refused", "connect <address> refused"),
            ("dns error for mc.pedrocraft.gg", "dns error for <address>"),
            ("connect [2001:db8::1]:25565", "connect <address>"),
            ("peer 2001:db8:0:0:0:0:0:1 gone", "peer <address> gone"),
        ] {
            assert_eq!(s.scrub(text), want, "{text}");
        }
        // Not addresses: versions, file places, Java names, public services, the time.
        for text in [
            "Minecraft 1.20.1 with Forge 47.2.0",
            "NeoForge 20.4.237 and 14.23.5.2859",
            "at runs.rs:120:5",
            "sodium-fabric-0.5.8.jar",
            "net.minecraft.client.Main",
            "java.lang.OutOfMemoryError",
            "api.modrinth.com answered 502",
            "at 12:34:56",
        ] {
            assert_eq!(s.scrub(text), text);
        }
    }

    #[test]
    fn secrets_and_names_go_whatever_their_case() {
        let s = Scrubber::new()
            .with_secret("eyJhbGciOiJ.secret.token")
            .with_secret("PedroGamer99")
            .with_secret("Al")
            .with_secret("x");
        assert_eq!(
            s.scrub("token eyJhbGciOiJ.secret.token for pedrogamer99, also Al and AL."),
            "token <redacted> for <redacted>, also <redacted> and <redacted>."
        );
        // One character is never a secret: it would mask everything.
        assert_eq!(s.scrub("x marks"), "x marks");
    }

    #[test]
    fn stand_ins_in_the_input_cannot_smuggle_anything_back() {
        let s = linux();
        let sneaky = format!("{STASH}0{STASH_END} {DATA} /home/moises/x");
        assert_eq!(s.scrub(&sneaky), "0  <home>");
    }

    #[test]
    fn public_hosts_match_whole_labels_only() {
        assert!(is_public_host("api.modrinth.com"));
        assert!(is_public_host("MODRINTH.com"));
        assert!(!is_public_host("evilmodrinth.com"));
        assert!(!is_public_host("modrinth.com.evil.net"));
    }

    #[test]
    fn a_root_too_short_to_be_a_folder_is_ignored() {
        let s = Scrubber::new().with_home("/").with_data_root("C:");
        assert_eq!(s.scrub("a/b and 1/2"), "a/b and 1/2");
    }
}
