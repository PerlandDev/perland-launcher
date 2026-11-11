//! Takes everything private out of a text before it is queued: known names and tokens, the
//! data folder and the home folder, emails, addresses, anything shaped like a token, and
//! every absolute path. What is left are placeholders (`<home>`, `<data>/instances`, `<path>`,
//! `<url>`, `<address>`, `<email>`, `<user>`, `<redacted>`) and the words around them.
//!
//! When in doubt it removes: a few words too many after a path, a Java package that ends
//! like a host name. It never keeps something because it "looks fine".
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// The longest text scrubbed; the rest is cut first, at a space. Far above any field's limit.
const INPUT_CAP: usize = 32 << 10;

/// Stand-ins while scrubbing, from Unicode's private use area. Any already in the input are
/// removed first, so a text can't smuggle one in.
const PRIVATE_USE: std::ops::RangeInclusive<char> = '\u{E000}'..='\u{E01F}';
const DATA: char = '\u{E000}';
const HOME: char = '\u{E001}';
const STASH: char = '\u{E002}';
const STASH_END: char = '\u{E003}';
/// Stands before a path start made while scrubbing, and is taken out again at the end.
const PAD: char = '\u{E004}';
/// A stashed piece's number is written in these ten, so no name or number can match it.
const DIGIT_0: u32 = 0xE010;

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

/// Public services the launcher talks to. A URL on one of these keeps its host (never its
/// path or query); any other host is the player's business.
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

/// Endings that make `name.ext` a file (or a place in one, with `:12`), not an address.
const FILE_ENDINGS: &[&str] = &[
    "rs",
    "js",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "jsx",
    "java",
    "kt",
    "json",
    "toml",
    "jar",
    "class",
    "txt",
    "log",
    "zip",
    "png",
    "dat",
    "cfg",
    "mrpack",
    "exe",
    "dll",
    "so",
    "html",
    "css",
    "md",
    "yml",
    "yaml",
    "xml",
    "properties",
    "lock",
    "tmp",
    "bak",
    "old",
    "nbt",
    "mca",
    "ogg",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "svg",
    "ico",
    "gz",
    "tar",
    "sha1",
    "bin",
    "pack",
    "mcmeta",
    "disabled",
    "rlib",
    "lib",
    "pdb",
    "dylib",
    "dat_old",
    "ini",
    "conf",
    "csv",
    "msi",
    "deb",
    "appimage",
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

/// Characters a path's last part can't hold, as the inside of a character class: whitespace,
/// quotes of every kind, and what no file name has.
const NOT_IN_PATH: &str = r#"\s"'`<>|*?:«»“”‘’\u{E002}-\u{E004}"#;

/// Windows spellings that only wrap a path: `\\?\C:\x`, `\\?\UNC\server\x`, `\\.\C:\x`.
static VERBATIM: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:\\\\|//)[?.](?:\\|/)(?P<unc>UNC(?:\\|/))?"));
/// `file:/C:/x` and `jar:file:/x!/y`: what follows is a path.
static FILE_SCHEME: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(?:jar:)?file:/+(?P<drive>[A-Za-z]:[\\/])?"));
static URL: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)\b[a-z][a-z0-9+.-]*://[^{NOT_IN_PATH_URL}]+",
        NOT_IN_PATH_URL = r#"\s"'`<>«»“”‘’\u{E000}-\u{E01F}"#
    ))
});
/// A path, wherever it stands: a stand-in for a known folder, a UNC share or `\x`, `~/…`,
/// `./…`, `/…` that doesn't follow a letter, a digit or another `/`, and a relative one
/// (`saves\Mi mundo\x`, or `a/b` when another `/` follows on the line).
///
/// Once started, a space does not end it: folder names have spaces. It runs to the last
/// separator on the line, and from there to a real end: a double quote, a backtick, « » “ ”,
/// `: `, ` (os error`, a `)` `]` `}` `,` `;` before a space, or the end of the line. Words
/// after a path go with it ("couldn't read <path>"): never too few.
static PATH: LazyLock<Regex> = LazyLock::new(|| {
    let seg = format!("[^{NOT_IN_PATH}]");
    let word = format!(r"[^{NOT_IN_PATH}/\\]");
    let hard = r#"\n\r"`<>|*?«»“”\u{E002}\u{E003}"#;
    re(&format!(
        r"(?P<lead>^|[^A-Za-z0-9/\\])(?P<path>(?:[\u{{E000}}\u{{E001}}]|\\{seg}|~[\\/]|\.{{1,2}}[\\/]|/{seg}|{word}+\\|{word}+/[^{hard}]*/)(?:[^{hard}]*[\\/])*[^{hard}]*?)(?P<end>$|[{hard}]|: |:$| \(os error|[)\]}},;](?:\s|$))"
    ))
});
/// A drive path. It needs no lead: after the URL step nothing else looks like `X:\`.
static DRIVE: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z]:[\\/]"));
static EMAIL: LazyLock<Regex> = LazyLock::new(|| re(r"[\w.%+-]+@[\w-]+(?:\.[\w-]+)+"));
/// A dotted word, its port, and anything glued to it with a `/` (`host/the-rest`).
static DOTTED: LazyLock<Regex> =
    LazyLock::new(|| re(r#"\b[\w-]+(?:\.[\w-]+)+(?P<port>:\d{1,5})?(?P<glued>/[^\s"'<>]*)?"#));
/// `name:25565`: a host without a dot, with a port a game or a service would use.
static NAME_PORT: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Za-z][\w-]*:(?P<port>\d{4,5})\b"));
/// IPv6: in brackets, written out, or shortened with `::`. What follows is captured so a
/// Rust path (`f32::from`) can be told apart.
static IPV6: LazyLock<Regex> = LazyLock::new(|| {
    let h = "[0-9A-Fa-f]{1,4}";
    re(&format!(
        r"\[[0-9A-Fa-f:.]{{2,}}\](?::\d{{1,5}})?|(?P<v6>\b(?:{h}:){{1,7}}:(?:{h}(?::{h})*)?|::(?:{h}:)*{h}(?:(?:\.\d{{1,3}}){{3}})?|\b(?:{h}:){{3,7}}{h})(?P<next>[\w:]?)"
    ))
});
/// Things shaped like a credential, whatever they are: a JWT, a bearer token, `token=…`.
static JWT: LazyLock<Regex> = LazyLock::new(|| re(r"\beyJ[\w-]{6,}\.[\w-]+(?:\.[\w-]+)?"));
static BEARER: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(?P<word>bearer|basic)\s+[^\s,;]+"));
static KEYED: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)\b(?P<key>[\w-]*(?:token|secret|password|passwd|authorization|api[_-]?key|session)|cookie|sid|pwd|pass)(?P<sep>["']?\s*[=:]\s*["']?)[^\s"',;&)]+"#,
    )
});
/// `--accessToken abc`, `--username Ana`: a command line's own secrets and names.
static FLAGGED: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?P<flag>--[\w-]*(?:token|secret|password|username|uuid|session|key)[\w-]*)[\s=]+\S+")
});
/// Keys that say what they are by how they start.
static PREFIXED: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(?:plt|pma|pk|sk|ghp|gho)_[A-Za-z0-9_-]{8,}"));
/// An account id, with or without dashes.
static UUID: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\b[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}\b|\b[0-9a-fA-F]{32}\b")
});
/// A long unbroken run of letters and digits: a token, a hash, an invite's code.
static LONG_RUN: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z0-9_-]{24,}"));
/// The launcher's own source, as a panic or a backtrace names it.
static SOURCE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:\./)?(?:crates|src|src-tauri|library|ui)/[\w.+/-]+(?::\d+){0,2}$"));
/// `code=…`: what a sign-in brings back in a URL or a form. (`exit code: 1` is not one.)
static CODE: LazyLock<Regex> = LazyLock::new(|| re(r#"(?i)\bcode=[^\s"',;&)]+"#));
static CARGO: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\.cargo/registry/src/[^/]+/(?P<tail>[\w.+-]+(?:/[\w.+-]+)*(?::\d+){0,2})$")
});
static RUSTC: LazyLock<Regex> =
    LazyLock::new(|| re(r"^/rustc/[0-9a-f]+/(?P<tail>[\w.+-]+(?:/[\w.+-]+)*(?::\d+){0,2})$"));
/// A page of the launcher's own window: only its bundled files keep their path.
static APP_PATH: LazyLock<Regex> =
    LazyLock::new(|| re(r"^/(?:assets|src)/[\w./@-]+(?::\d+){0,2}$"));

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
        // As a URL writes it: spaces and other characters percent-encoded.
        for spelling in spellings.clone() {
            let encoded = spelling.replace(' ', "%20");
            if encoded != spelling {
                spellings.push(encoded);
            }
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

/// `text` between word boundaries, where its own ends are word characters (a boundary next
/// to a `.` or a space would never match).
fn whole_word(text: &str) -> String {
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    format!(
        "{}{}{}",
        if is_word(text.chars().next()) {
            r"\b"
        } else {
            ""
        },
        regex::escape(text),
        if is_word(text.chars().last()) {
            r"\b"
        } else {
            ""
        }
    )
}

/// See the module's own words. Built from what the launcher knows right then.
#[derive(Default)]
pub struct Scrubber {
    /// Names, each as a pattern: matched without regard to case, short ones only as whole
    /// words. Longest first.
    name_patterns: Vec<String>,
    /// All of `name_patterns` as one alternation.
    names: Option<Regex>,
    roots: Vec<Root>,
    user: Option<Regex>,
}

// Manual Debug: this holds what must never print.
impl std::fmt::Debug for Scrubber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scrubber")
            .field("names", &self.name_patterns.len())
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
            self.user = Regex::new(&format!("(?i){}", whole_word(user))).ok();
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
        self.add_secrets([secret]);
    }

    /// Several at once: the names are compiled into one pattern, once.
    pub fn add_secrets<I, S>(&mut self, secrets: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for secret in secrets {
            let secret = secret.as_ref().trim();
            let len = secret.chars().count();
            if len < 2 {
                continue;
            }
            let mut spellings = vec![secret.to_string()];
            // As a URL writes it.
            let encoded =
                percent_encoding::utf8_percent_encode(secret, percent_encoding::NON_ALPHANUMERIC)
                    .to_string();
            let spaces = secret.replace(' ', "%20");
            for other in [encoded, spaces] {
                if !spellings.contains(&other) {
                    spellings.push(other);
                }
            }
            for spelling in spellings {
                let pattern = if len >= 5 {
                    regex::escape(&spelling)
                } else {
                    whole_word(&spelling)
                };
                if !self.name_patterns.contains(&pattern) {
                    self.name_patterns.push(pattern);
                }
            }
        }
        // Longest first, so a name isn't half-taken by a shorter one inside it.
        self.name_patterns
            .sort_by_key(|pattern| std::cmp::Reverse(pattern.len()));
        self.names = (!self.name_patterns.is_empty())
            .then(|| {
                regex::RegexBuilder::new(&format!("(?i)(?:{})", self.name_patterns.join("|")))
                    .size_limit(64 << 20)
                    .build()
                    .ok()
            })
            .flatten();
    }

    /// Known secrets and names out of `text`.
    fn mask(&self, text: &str) -> String {
        match &self.names {
            Some(names) => names.replace_all(text, "<redacted>").into_owned(),
            None => text.to_string(),
        }
    }

    pub fn scrub(&self, input: &str) -> String {
        let mut text: String = input.chars().filter(|c| !PRIVATE_USE.contains(c)).collect();
        if text.len() > INPUT_CAP {
            // Cut at a space, so no name or token is left half there for a later rule to miss.
            let mut end = INPUT_CAP;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let cut = text[..end].rfind(char::is_whitespace).unwrap_or(0);
            text.truncate(cut);
        }
        // Spellings that only wrap a path.
        text = VERBATIM
            .replace_all(&text, |found: &Captures| {
                if found.name("unc").is_some() {
                    r"\\".to_string()
                } else {
                    String::new()
                }
            })
            .into_owned();
        // Shapes first (a URL, a path, an address are recognised whole), then what the
        // launcher knows by name. Finished pieces wait in `stash`, where no later step can
        // touch them; a piece that keeps some of its text is masked on its way in.
        let mut stash: Vec<String> = Vec::new();
        let mut keep = |piece: String| {
            stash.push(self.mask(&piece));
            let number: String = (stash.len() - 1)
                .to_string()
                .bytes()
                .filter_map(|digit| char::from_u32(DIGIT_0 + u32::from(digit - b'0')))
                .collect();
            format!("{STASH}{number}{STASH_END}")
        };
        text = URL
            .replace_all(&text, |found: &Captures| keep(url_placeholder(&found[0])))
            .into_owned();
        // Anything shaped like a credential, whatever it is.
        text = JWT.replace_all(&text, "<redacted>").into_owned();
        text = BEARER.replace_all(&text, "$word <redacted>").into_owned();
        text = KEYED.replace_all(&text, "$key$sep<redacted>").into_owned();
        text = CODE.replace_all(&text, "code=<redacted>").into_owned();
        text = FLAGGED.replace_all(&text, "$flag <redacted>").into_owned();
        text = FILE_SCHEME
            .replace_all(&text, |found: &Captures| match found.name("drive") {
                Some(drive) => format!("{PAD}{}", drive.as_str()),
                None => format!("{PAD}/"),
            })
            .into_owned();
        for root in &self.roots {
            for form in &root.forms {
                text = form
                    .replace_all(&text, root.mark.to_string().as_str())
                    .into_owned();
            }
        }
        // A drive becomes a path start the path rule takes wherever it stands.
        text = DRIVE
            .replace_all(&text, format!("{PAD}//").as_str())
            .into_owned();
        // A known folder may follow anything (`)C:\Users\…`, `\nC:\…`): give it room.
        text = text
            .replace(DATA, &format!("{PAD}{DATA}"))
            .replace(HOME, &format!("{PAD}{HOME}"));
        // More than once: a path's end can be the very character the next one starts after.
        for _ in 0..8 {
            let replaced = PATH
                .replace_all(&text, |found: &Captures| {
                    format!(
                        "{}{}{}",
                        &found["lead"],
                        keep(path_placeholder(&found["path"])),
                        &found["end"]
                    )
                })
                .into_owned();
            if replaced == text {
                break;
            }
            text = replaced;
        }
        text = text.replace(DATA, "<data>").replace(HOME, "<home>");
        // After the paths, so a token inside one can't break it in two.
        text = PREFIXED.replace_all(&text, "<redacted>").into_owned();
        text = UUID.replace_all(&text, "<redacted>").into_owned();
        text = LONG_RUN
            .replace_all(&text, |found: &Captures| {
                let run = &found[0];
                let mixed = run.bytes().any(|b| b.is_ascii_digit())
                    && run.bytes().any(|b| b.is_ascii_alphabetic());
                if mixed {
                    "<redacted>".to_string()
                } else {
                    run.to_string()
                }
            })
            .into_owned();
        text = EMAIL.replace_all(&text, "<email>").into_owned();
        text = IPV6
            .replace_all(&text, |found: &Captures| {
                let whole = &found[0];
                let Some(v6) = found.name("v6") else {
                    return "<address>".to_string();
                };
                let next = found.name("next").map_or("", |m| m.as_str());
                // `f32::from` and `serde::de::Error` are Rust, and `12:34:56` is the time.
                let rust_or_time =
                    !next.is_empty() || !v6.as_str().bytes().any(|b| b.is_ascii_digit());
                if rust_or_time {
                    whole.to_string()
                } else {
                    "<address>".to_string()
                }
            })
            .into_owned();
        text = DOTTED
            .replace_all(&text, |found: &Captures| {
                dotted_placeholder(
                    &found[0],
                    found.name("port").is_some(),
                    found.name("glued").map_or("", |m| m.as_str()),
                )
            })
            .into_owned();
        text = NAME_PORT
            .replace_all(&text, |found: &Captures| {
                let port: u32 = found["port"].parse().unwrap_or(0);
                if (1024..=65535).contains(&port) {
                    "<address>".to_string()
                } else {
                    found[0].to_string()
                }
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
            let number: Option<usize> = after[..end]
                .chars()
                .map(|c| char::from_digit((c as u32).wrapping_sub(DIGIT_0), 10))
                .collect::<Option<String>>()
                .and_then(|digits| digits.parse().ok());
            if let Some(piece) = number.and_then(|i| stash.get(i)) {
                out.push_str(piece);
            }
            rest = &after[end + STASH_END.len_utf8()..];
        }
        out.push_str(rest);
        out.retain(|c| c != PAD);
        out.trim().to_string()
    }
}

/// A URL on a public service keeps its host; the window's own bundled files become
/// `<app>/…`; anything else is `<url>`.
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
        // A stack frame: the bundled file and the `:line:column` after it, nothing else.
        return if APP_PATH.is_match(url.path()) {
            format!("<app>{}{tail}", url.path())
        } else {
            format!("<app>{tail}")
        };
    }
    if matches!(url.scheme(), "http" | "https") && is_public_host(host) {
        return format!("{}://{}{tail}", url.scheme(), host.to_ascii_lowercase());
    }
    format!("<url>{tail}")
}

fn path_placeholder(path: &str) -> String {
    let unix = path.replace("\\\\", "/").replace('\\', "/");
    // `file.rs:12:5:` at the head of a panic message.
    let trimmed = unix.trim_end_matches(':');
    let colons = &unix[trimmed.len()..];
    let unix = trimmed.to_string();
    if let Some(rest) = unix.strip_prefix(DATA) {
        let top = rest
            .trim_start_matches('/')
            .split(['/', ' '])
            .next()
            .unwrap_or("");
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
    // player, and what makes a backtrace readable. Only when the rest is plainly source.
    if let Some(found) = CARGO.captures(&unix) {
        return format!("<cargo>/{}{colons}", &found["tail"]);
    }
    if let Some(found) = RUSTC.captures(&unix) {
        return format!("<rustc>/{}{colons}", &found["tail"]);
    }
    // The launcher's own source files, relative to its repository.
    if SOURCE.is_match(&unix) {
        return format!("{}{colons}", unix.trim_start_matches("./"));
    }
    "<path>".into()
}

/// Endings a host name commonly has. A dotted word that ends in one is an address whatever
/// else it looks like.
const HOST_ENDINGS: &[&str] = &[
    "com", "net", "org", "io", "gg", "online", "xyz", "me", "co", "dev", "fun", "club", "es", "mx",
    "ar", "cl", "pe", "ve", "uy", "ec", "bo", "py", "br", "uk", "de", "fr", "it", "nl", "pl", "ru",
    "eu", "us", "ca", "au", "jp", "cn", "in", "tk", "host", "pro", "live", "world", "games",
    "land", "space", "site", "cc", "tv", "info", "app", "cloud", "network", "lat", "local", "lan",
    "store", "server", "shop", "tech", "top", "icu", "link", "one", "play", "gay", "wtf", "lol",
    "cool",
];

/// File endings that are also real host endings: only a plain `name.ext` is taken as a file.
const FILE_OR_HOST: &[&str] = &["rs", "zip", "md", "so", "properties", "sh", "py"];

/// `a.b.c`, with or without a port or something glued on with `/`.
///
/// An address (`<address>`, the glued part with it): an IPv4 address; anything with a port
/// that isn't a place in a file; anything that ends like a host ([`HOST_ENDINGS`], or
/// punycode); three labels or more that end in letters; and a lowercase `name.word`.
/// Kept: a public service's host (without what is glued on), a file name, a version, and a
/// name that ends in a class (`java.net.UnknownHostException`, `Object.render`).
fn dotted_placeholder(found: &str, has_port: bool, glued: &str) -> String {
    let whole = &found[..found.len() - glued.len()];
    let name = whole.split(':').next().unwrap_or(whole);
    let labels: Vec<&str> = name.split('.').collect();
    let first = labels.first().copied().unwrap_or("");
    let last = labels.last().copied().unwrap_or("");
    let lower = last.to_lowercase();
    let all_digits = |l: &&str| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit());
    if labels.len() == 4 && labels.iter().all(|l| l.len() <= 3 && all_digits(l)) {
        return "<address>".into();
    }
    if is_public_host(name) {
        return name.to_ascii_lowercase();
    }
    let versioned = labels
        .iter()
        .any(|l| l.starts_with(|c: char| c.is_ascii_digit()));
    let file_ending = FILE_ENDINGS.contains(&lower.as_str());
    // `mods.zip` and `sodium-0.5.8.jar` are files; `mc.pedro.rs` is a host.
    let is_file =
        file_ending && (labels.len() == 2 || versioned || !FILE_OR_HOST.contains(&lower.as_str()));
    if is_file {
        return found.into();
    }
    if has_port {
        return "<address>".into();
    }
    let letters = last.chars().count() >= 2 && last.chars().all(char::is_alphabetic);
    let host_ending = HOST_ENDINGS.contains(&lower.as_str()) || lower.starts_with("xn--");
    // `…client.Main`, `Object.render`: code.
    let camel = |l: &str| l.starts_with(char::is_uppercase) && l.chars().any(char::is_lowercase);
    let class_last = camel(last) && !host_ending;
    let plain = |l: &str| !l.chars().any(char::is_uppercase);
    let address = host_ending
        || (letters && !class_last && labels.len() >= 3)
        || (letters && labels.len() == 2 && plain(first) && plain(last));
    if address {
        "<address>".into()
    } else {
        found.into()
    }
}

/// `text` without `values`: what one error calls its own private parts (a param that is not
/// on the allow-list), wherever its message or detail repeats them, whatever their case.
pub fn without_values(text: &str, values: &[&str]) -> String {
    let mut out = text.to_string();
    let mut values: Vec<&str> = values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect();
    values.sort_by_key(|v| std::cmp::Reverse(v.len()));
    for value in values {
        let pattern = if value.chars().count() >= 4 {
            format!("(?i){}", regex::escape(value))
        } else {
            format!("(?i){}", whole_word(value))
        };
        if let Ok(found) = Regex::new(&pattern) {
            out = found.replace_all(&out, "<redacted>").into_owned();
        }
    }
    out
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
            assert_eq!(s.scrub(text), "open <home>", "{text}");
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
            "No such file: <home>"
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
            ("open \"/media/Mi Disco/a.jar\" and", "open \"<path>\" and"),
            ("open /media/Mi Disco/Juegos/a.jar now", "open <path>"),
            (r"open D:\Juegos\Minecraft de Luis\a.jar", "open <path>"),
            (r"open \\nas\luis\a.jar", "open <path>"),
            ("path=/opt/x", "<path>"),
            ("file:/home/otro/x", "<path>"),
            (r"D:\Games (old)\Users\luis\x now", "<path>"),
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
            "GET https://api.modrinth.com failed."
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
        assert_eq!(s.scrub("a/b and 50%"), "a/b and 50%");
    }

    #[test]
    fn windows_verbatim_and_file_scheme_spellings_are_paths_too() {
        let s = windows();
        for (text, want) in [
            (
                r"open \\?\C:\Users\Ana Maria\Downloads\Tarea de Luis\pack.mrpack failed",
                "open <home>",
            ),
            (
                r"\\?\C:\Users\Ana Maria\AppData\Roaming\PerlandLauncher\instances\Mundo de Luis\mods\x.jar: denied",
                "<data>/instances: denied",
            ),
            (r"open \\?\D:\Juegos\Luis Gomez\a.jar failed", "open <path>"),
            (r"open \\?\UNC\nas\luis\a.jar", "open <path>"),
            (r"open \\.\D:\Luis\a.jar", "open <path>"),
            (
                "jar:file:/C:/Users/Ana Maria/Downloads/Tarea de Luis/x.jar!/fabric.mod.json",
                "<home>",
            ),
            ("file:/D:/Luis/x.png gone", "<path>"),
        ] {
            assert_eq!(s.scrub(text), want, "{text}");
        }
    }

    #[test]
    fn a_path_is_a_path_whatever_stands_before_it() {
        let s = windows();
        for text in [
            r"«C:\Users\Ana Maria\Documents\Tarea de Luis\x.png»",
            r"“D:\Luis\x.png”",
            r"failed)C:\Users\Ana Maria\Documents\Luis\x",
            r"line1\nC:\Users\Ana Maria\Documents\Luis\x",
            r"-path-D:\Luis\x",
            r"a|D:\Luis\y",
            r"a+D:\Luis\z",
            r"<D:\Luis\w>",
            "@/home/otro/luis/x",
            "#/home/otro/luis/y",
            "%/srv/luis/z",
            r"cwd=C:\Users\Ana Maria\Documents\Luis;PATH=C:\x|D:\Luis\y",
            "~/Documentos/Tarea de Luis/x",
            r"~\Documents\Luis\x",
        ] {
            let out = s.scrub(text).to_lowercase();
            for private in [
                "luis",
                "ana",
                "maria",
                "otro",
                "users",
                "documents",
                "documentos",
            ] {
                assert!(
                    !out.contains(private),
                    "{private} left in {out:?} from {text}"
                );
            }
        }
    }

    #[test]
    fn folder_and_file_names_of_many_words_go_whole() {
        let s = linux();
        for (text, want) in [
            (
                r"open D:\Los juegos de mi hermano Luis Gomez\a.jar failed",
                "open <path>",
            ),
            (
                "open /home/moises/Mis documentos muy importantes de Pedro Gomez/x.png failed",
                "open <home>",
            ),
            (
                "No such file: /home/moises/Descargas/skin de Pedro Gomez.png (os error 2)",
                "No such file: <home> (os error 2)",
            ),
            (
                "copy /media/Mi Disco/a b c.zip to /srv/x: denied",
                "copy <path>: denied",
            ),
        ] {
            assert_eq!(s.scrub(text), want, "{text}");
        }
    }

    #[test]
    fn hosts_on_any_ending_and_bare_names_with_a_port_go() {
        let s = Scrubber::new();
        for text in [
            "dns error for play.pedrocraft.tk",
            "dns error for mc.luis.ru",
            "dns error for pedro.com.ve",
            "dns error for survival.pedro.server",
            "dns error for mc.pedro.store",
            "connect to DESKTOP-PEDRO:25565 failed",
            "dns error for mc.peña.net",
            "dns error for PLAY.PEDROCRAFT.NET",
            "sent to josé.peña@correo.mx",
        ] {
            let out = s.scrub(text).to_lowercase();
            for private in ["pedro", "luis", "peña", "josé", "25565"] {
                assert!(!out.contains(private), "{private} left in {out:?}");
            }
        }
        // Code, files and versions are not hosts.
        for text in [
            "java.util.concurrent.ExecutionException",
            "net.fabricmc.loader.impl.FormattedException",
            "com.mojang.authlib.GameProfile",
            "TypeError: Object.keys is not a function",
            "at Object.render (index.js:1:2)",
            "fabric.mod.json and quilt.mod.json",
            "launcher_core.rlib",
            "options.txt, servers.dat and level.dat",
            "exit code: 1 after 12:34:56",
            "v0.2.15-beta.1 on Minecraft 1.21.4",
            "port:80 is taken",
        ] {
            assert_eq!(s.scrub(text), text);
        }
    }

    #[test]
    fn shortened_ipv6_goes_and_rust_paths_stay() {
        let s = Scrubber::new();
        assert_eq!(
            s.scrub("peer 2001:db8::1 gone, fe80::a1b2:c3d4:e5f6:1234 and 2001:db8:85a3::8a2e:370:7334, also ::1"),
            "peer <address> gone, <address> and <address>, also <address>"
        );
        for text in [
            "serde::de::Error at f32::from",
            "launcher_app::runs::play",
            "core::fmt::Display for u8::MAX",
            "std::fs::read",
            "a::b",
            "at 12:34:56",
        ] {
            assert_eq!(s.scrub(text), text);
        }
    }

    #[test]
    fn anything_shaped_like_a_credential_goes() {
        let s = Scrubber::new();
        let out = s.scrub(
            "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abcDEF123 token=abc123secret access_token: M.R3_BAY.-Cabc refresh_token\":\"0.AXoA9 code=M.C5_SN1.2 password=hunter22 session: s3ss10n",
        );
        for private in [
            "eyJ",
            "abc123secret",
            "R3_BAY",
            "AXoA9",
            "C5_SN1",
            "hunter22",
            "s3ss10n",
        ] {
            assert!(!out.contains(private), "{private} left in {out}");
        }
        // Not credentials.
        for text in ["exit code 1", "exit code: 1", "status code: 404 Not Found"] {
            assert_eq!(s.scrub(text), text);
        }
    }

    #[test]
    fn kept_pieces_keep_only_what_every_player_shares() {
        let s = windows().with_secret("Servidor de Pedro");
        assert_eq!(
            s.scrub(
                "GET https://api.mojang.com/users/profiles/minecraft/LuisFriend77 answered 404"
            ),
            "GET https://api.mojang.com answered 404"
        );
        assert_eq!(
            s.scrub("at tauri://localhost/C:/Users/Ana%20Maria/x.js:1:2"),
            "at <app>"
        );
        assert_eq!(
            s.scrub("at /mnt/pedro/registry/src/x/Pedro Secret/file.rs"),
            "at <path>"
        );
        assert_eq!(
            s.scrub("at /home/pedro/.cargo/registry/src/index.crates.io-1/Pedro Secret/file.rs:3"),
            "at <path>"
        );
        for text in [
            "open Servidor%20de%20Pedro now",
            "open servidor%20de%20pedro now",
            "C:/Users/Ana%20Maria/Documents/x",
        ] {
            let out = s.scrub(text).to_lowercase();
            assert!(!out.contains("pedro") && !out.contains("maria"), "{out}");
        }
    }

    #[test]
    fn a_cut_never_leaves_half_a_secret() {
        let s = Scrubber::new().with_secret("SECRETTOKEN1234567890");
        let filler = "x ".repeat(INPUT_CAP / 2 - 6);
        let out = s.scrub(&format!("{filler}SECRETTOKEN1234567890 and more"));
        assert!(!out.contains("SECRET"), "{}", &out[out.len() - 40..]);
        assert!(out.len() <= INPUT_CAP);
    }

    #[test]
    fn names_that_end_in_punctuation_or_are_numbers_are_still_masked() {
        let s = Scrubber::new()
            .with_user("ana.")
            .with_secret("12")
            .with_secret("x.y");
        assert_eq!(s.scrub("user ana. logged in"), "user <user> logged in");
        // A numeric name can't eat a stashed piece's number.
        assert_eq!(
            s.scrub("12 at https://api.modrinth.com/a and /srv/b and /srv/c"),
            "<redacted> at https://api.modrinth.com and <path>"
        );
    }

    /// The second review's inputs, as it sent them.
    #[test]
    fn an_apostrophe_or_a_colon_inside_a_path_does_not_end_it() {
        let l = Scrubber::new()
            .with_data_root("/home/sofia/.local/share/PerlandLauncher")
            .with_home("/home/sofia");
        let w = Scrubber::new()
            .with_data_root(r"C:\Users\Ana María\AppData\Roaming\PerlandLauncher")
            .with_home(r"C:\Users\Ana María");
        for (scrubber, text, want) in [
            (&l, "/home/sofia/It's mine/secret dir/file.txt", "<home>"),
            (
                &l,
                r"D:\Juegos\Sofía's Mods\secreto.jar not found",
                "<path>",
            ),
            (
                &w,
                r"C:\Users\Ana María\AppData\Roaming\.minecraft\saves\Casa de Sofía's\level.dat",
                "<home>",
            ),
            (
                &w,
                r"C:\Users\Ana María\AppData\Roaming\PerlandLauncher\instances\ana's world\mods\Secret Mod.jar",
                "<data>/instances",
            ),
            (
                &l,
                r"D:\Juegos de Sofía\Mundo: La casa\level.dat missing",
                "<path>",
            ),
            (&l, "/home/sofia/a:b/secret/file.txt", "<home>"),
            (
                &l,
                r"D:\Juegos\Sofía's MultiMC\instances\Casa de Sofía\.minecraft\options.txt: permission denied",
                "<path>: permission denied",
            ),
            (
                &w,
                r"C:\Users\Ana María\Sofía's Java\bin\javaw.exe",
                "<home>",
            ),
            (
                &l,
                "failed to open \"/home/sofia/It's mine/x.txt\": denied",
                "failed to open \"<home>\": denied",
            ),
            (&l, "open /home/sofia/it's.txt now", "open <home>"),
        ] {
            assert_eq!(scrubber.scrub(text), want, "{text}");
        }
    }

    #[test]
    fn a_host_is_a_host_whatever_its_first_label_ending_or_capitals() {
        let s = Scrubber::new();
        for host in [
            "fabric.sofiacraft.net",
            "mods.sofiacraft.net",
            "dev.sofiacraft.net",
            "forge.sofiacraft.net",
            "mod.sofiacraft.net",
            "me.sofiacraft.net",
            "net.sofiacraft.org",
            "io.sofiacraft.gg",
            "quilt.sofiacraft.xyz",
            "mc.sofiacraft.rs",
            "mc.sofiacraft.md",
            "mc.sofiacraft.so",
            "Play.SofiaCraft.net",
            "Sofia.aternos.me",
            "sofia_smp.example.com",
            "sofiacraft.xn--p1ai",
            "sofiacraft.cool",
            "sofiacraft.land",
        ] {
            let text = format!("Couldn't reach the server's Perland site at {host}");
            assert_eq!(
                s.scrub(&text),
                "Couldn't reach the server's Perland site at <address>",
                "{host}"
            );
        }
        assert_eq!(
            s.scrub("java.net.UnknownHostException: fabric.sofiacraft.net"),
            "java.net.UnknownHostException: <address>"
        );
        // Still not hosts.
        for text in [
            "sodium-fabric-0.5.8.jar and fabric-api-0.92.0+1.20.1.jar",
            "fabric.mod.json, pack.mcmeta, mods.zip and level.dat_old",
            "net.minecraft.client.Main",
            "at Object.render (index.js:1:2)",
            "Minecraft 1.20.1 with Forge 47.2.0",
        ] {
            assert_eq!(s.scrub(text), text);
        }
    }

    #[test]
    fn tokens_nobody_taught_the_scrubber_go_by_their_shape() {
        let s = Scrubber::new();
        for (text, private) in [
            ("key plt_9f8e7d6c5b4a39281706f5e4d3c2b1a0 refused", "9f8e7d"),
            ("key pk_live_51Habcdefghijklmnop refused", "51Habc"),
            (
                "invite token AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-abcde is not valid",
                "AbCdEf",
            ),
            (
                "play.sofiacraft.net/AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-abcde",
                "AbCdEf",
            ),
            (
                "perland:join/AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-abcde",
                "AbCdEf",
            ),
            ("github.com/sofia-lopez/secret-pack", "sofia"),
            (
                "--accessToken ZXhhbXBsZXRva2VuMTIzNDU2Nzg5MGFiY2RlZg --username Sofi_2014",
                "ZXhhbX",
            ),
            (
                "--accessToken ZXhhbXBsZXRva2VuMTIzNDU2Nzg5MGFiY2RlZg --username Sofi_2014",
                "Sofi_2014",
            ),
            (
                "refresh token M.C507_BAY.0.U.-CqZ8vQ0rabcdefghijklmnopqrstuvwxyz0123456789 expired",
                "CqZ8vQ0r",
            ),
            (
                "cookie: sid=s%3Aabcdefghijklmnop.qrstuvwxyz0123456789",
                "abcdefghijklmnop",
            ),
            ("pwd=hunter2", "hunter2"),
            ("pass: hunter2", "hunter2"),
            ("uuid 069a79f4-44e9-4726-a5be-fca90e38aaf5", "069a79f4"),
            ("uuid 069a79f444e94726a5befca90e38aaf5", "069a79f4"),
        ] {
            let out = s.scrub(text);
            assert!(!out.contains(private), "{private} left in {out:?}");
        }
    }

    #[test]
    fn an_ipv4_address_inside_an_ipv6_one_goes_whole() {
        let s = Scrubber::new();
        assert_eq!(
            s.scrub("peer ::ffff:203.0.113.9 gone"),
            "peer <address> gone"
        );
        assert_eq!(
            s.scrub("peer [::ffff:203.0.113.9]:25565 gone"),
            "peer <address> gone"
        );
    }

    #[test]
    fn relative_paths_go_and_the_launchers_own_source_stays() {
        let s = Scrubber::new().with_user("Ana María");
        for text in [
            r"Caused by: java.nio.file.NoSuchFileException: saves\Casa de Sofía\level.dat",
            "zip entry overrides/saves/Casa de Sofía/level.dat is invalid",
            r"..\..\Users\Sofía\Desktop\x.txt",
            r"\Users\Ana María\Documents\secret plans\x.txt",
            r"C:Users\Sofía\x.txt",
            "./mundos/Casa de Sofía/level.dat",
        ] {
            let out = s.scrub(text).to_lowercase();
            for private in [
                "sofía", "casa", "secret", "users", "saves", "mundos", "desktop",
            ] {
                assert!(
                    !out.contains(private),
                    "{private} left in {out:?} from {text}"
                );
            }
        }
        for (text, want) in [
            (
                "panicked at crates/launcher-app/src/runs.rs:120:5",
                "panicked at crates/launcher-app/src/runs.rs:120:5",
            ),
            (
                r"panicked at crates\launcher-app\src\runs.rs:120:5",
                "panicked at crates/launcher-app/src/runs.rs:120:5",
            ),
            (
                "   at ./crates/launcher-core/src/x.rs:7:1",
                "at crates/launcher-core/src/x.rs:7:1",
            ),
            ("at src/main.rs:88", "at src/main.rs:88"),
        ] {
            assert_eq!(s.scrub(text), want, "{text}");
        }
        // One separator is not a path.
        for text in ["and/or", "50 KB/s", "application/json", "I/O error"] {
            assert_eq!(s.scrub(text), text);
        }
    }

    #[test]
    fn an_errors_own_private_values_go_wherever_it_repeats_them() {
        assert_eq!(
            without_values(
                "Couldn't reach FABRIC.sofiacraft.net (fabric.sofiacraft.net) for Al, also",
                &["fabric.sofiacraft.net", "Al", "x"],
            ),
            "Couldn't reach <redacted> (<redacted>) for <redacted>, also"
        );
    }

    #[test]
    fn many_names_are_one_pattern_and_the_longest_wins() {
        let mut s = Scrubber::new();
        s.add_secrets(["Pedro", "Servidor de Pedro", "Al", "PedroGamer99"]);
        assert_eq!(
            s.scrub("Servidor de Pedro, PedroGamer99, pedro and al, not also"),
            "<redacted>, <redacted>, <redacted> and <redacted>, not also"
        );
    }

    /// The second review's second round.
    #[test]
    fn a_folder_name_with_spaces_at_the_end_of_a_path_goes_whole() {
        let s = Scrubber::new()
            .with_data_root(r"C:\Users\Ana\AppData\Roaming\PerlandLauncher")
            .with_home(r"C:\Users\Ana");
        for (text, want) in [
            (
                r"D:\Juegos\Casa de Sofía: permission denied",
                "<path>: permission denied",
            ),
            (
                r"C:\Users\Ana\Documents\Casa de Sofía: permission denied",
                "<home>: permission denied",
            ),
            (
                "/mnt/datos/Casa de Sofía: No such file or directory (os error 2)",
                "<path>: No such file or directory (os error 2)",
            ),
            (
                "/mnt/datos/Casa de Sofía (os error 13)",
                "<path> (os error 13)",
            ),
            (
                "couldn't read /mnt/datos/Mundo de Sofía",
                "couldn't read <path>",
            ),
            (
                r#"failed to open "D:\\Juegos\\Casa de Sofía": denied"#,
                r#"failed to open "<path>": denied"#,
            ),
            (
                r"C:\Users\Ana\AppData\Roaming\PerlandLauncher\instances\x\minecraft\saves\Casa de Sofía is locked",
                "<data>/instances",
            ),
            ("path '/mnt/a/Sofía's stuff' is gone", "path '<path>"),
            ("/Applications/Minecraft Sofía.app", "<path>"),
            (
                "see (/mnt/a/Casa de Sofía) and [/mnt/b/Otra casa], then",
                "see <path>, then",
            ),
            (
                "first /mnt/a/Casa de Sofía\nthen /mnt/b/Otra casa\n",
                "first <path>\nthen <path>",
            ),
            ("moved /mnt/a/Casa: /mnt/b/Otra casa", "moved <path>"),
        ] {
            assert_eq!(s.scrub(text), want, "{text}");
        }
    }

    #[test]
    fn a_relative_path_with_spaces_in_a_folder_name_goes_too() {
        let s = Scrubber::new();
        for (text, want) in [
            ("saves/Casa de Sofía/level.dat missing", "<path>"),
            (
                "cwd=instances/mi mundo secreto/mods/x.jar relative",
                "<path>",
            ),
            (
                "zip entry overrides/Casa de Sofía/level.dat is invalid",
                "zip entry <path>",
            ),
        ] {
            assert_eq!(s.scrub(text), want, "{text}");
        }
        // One `/` on a line is still not a path.
        for text in [
            "and/or",
            "50 KB/s",
            "application/json",
            "I/O error",
            "read/write",
        ] {
            assert_eq!(s.scrub(text), text);
        }
    }
}
