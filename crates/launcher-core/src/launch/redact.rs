/// Scrubs secrets and the user's home path from anything written to logs.
#[derive(Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
    /// Every spelling of the home folder (the long path, its 8.3 short form, ...).
    homes: Vec<String>,
}

// Manual Debug: the whole point of this type is to hold secrets, so they must never print.
impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redactor")
            .field("secrets", &self.secrets.len())
            .field("homes", &self.homes)
            .finish()
    }
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_current_user() -> Self {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        Self::for_home(home)
    }

    /// The home plus, on Windows, its 8.3 short form (`C:\Users\STEVEN~1`), which some tools print.
    fn for_home(home: String) -> Self {
        #[allow(unused_mut)]
        let mut redactor = Self::new().with_home(home.clone());
        #[cfg(windows)]
        if let Some(short) = short_path(&home) {
            redactor = redactor.with_home_alias(short);
        }
        redactor
    }

    /// Adds a home folder to mask; calling it again adds another (see `with_home_alias`).
    pub fn with_home(mut self, home: impl Into<String>) -> Self {
        let home = home.into();
        if !home.is_empty() && !self.homes.contains(&home) {
            self.homes.push(home);
        }
        self
    }

    /// Another spelling of the home folder, masked like the home itself.
    pub fn with_home_alias(self, alias: impl Into<String>) -> Self {
        self.with_home(alias)
    }

    /// Secrets shorter than 4 characters (like the offline token "0") are ignored; redacting them would mangle every log line.
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        let secret = secret.into();
        if secret.len() >= 4 {
            self.secrets.push(secret);
        }
        self
    }

    pub fn redact(&self, input: &str) -> String {
        let mut out = input.to_string();
        for secret in &self.secrets {
            out = out.replace(secret.as_str(), "<redacted>");
        }
        for (form, windows) in self.home_forms() {
            out = if windows {
                replace_ascii_case_insensitive(&out, &form, "~")
            } else {
                out.replace(form.as_str(), "~")
            };
        }
        out
    }

    /// Every form of every home, longest first so a home that is a prefix of another
    /// (`C:\Users\steve` and `C:\Users\steve.old`) never leaves a fragment behind. Each form says
    /// whether it is a Windows path, which matches without regard to ASCII letter case.
    fn home_forms(&self) -> Vec<(String, bool)> {
        let mut forms: Vec<(String, bool)> = Vec::new();
        for home in &self.homes {
            let windows = is_windows_path(home);
            let mut add = |form: String| {
                if !forms.iter().any(|(f, _)| *f == form) {
                    forms.push((form, windows));
                }
            };
            // Debug-formatted log fields double every backslash, and Java prints Windows paths
            // with forward slashes, so mask those forms too.
            if home.contains('\\') {
                add(home.replace('\\', "\\\\"));
                add(home.replace('\\', "/"));
            }
            add(home.clone());
        }
        forms.sort_by_key(|(form, _)| std::cmp::Reverse(form.len()));
        forms
    }

    /// Like `redact`, for text that was cut off mid-line: a secret the cut went through can't be
    /// matched whole, so a trailing fragment (4 characters or more) of any secret is masked too.
    pub fn redact_cut(&self, input: &str) -> String {
        let mut out = self.redact(input);
        let fragment_len = self
            .secrets
            .iter()
            .flat_map(|secret| {
                secret
                    .char_indices()
                    .map(|(i, _)| i)
                    .filter(|&i| i >= 4)
                    .map(|i| &secret[..i])
            })
            .filter(|prefix| out.ends_with(prefix))
            .map(str::len)
            .max();
        if let Some(len) = fragment_len {
            out.truncate(out.len() - len);
            out.push_str("<redacted>");
        }
        out
    }
}

/// A path with a backslash or a drive letter (`C:`) is a Windows path.
fn is_windows_path(path: &str) -> bool {
    let b = path.as_bytes();
    path.contains('\\') || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// `str::replace`, but ASCII letters match regardless of case. Lowercasing ASCII keeps every
/// byte offset (and char boundary), so matches found in the lowercased copy splice the original.
fn replace_ascii_case_insensitive(haystack: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let lower = haystack.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    for (i, _) in lower.match_indices(needle_lower.as_str()) {
        out.push_str(&haystack[last..i]);
        out.push_str(with);
        last = i + needle.len();
    }
    out.push_str(&haystack[last..]);
    out
}

/// The 8.3 short form of an existing path, or `None` when Windows has none to give.
#[cfg(windows)]
fn short_path(path: &str) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    if path.is_empty() {
        return None;
    }
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated; a zero-length buffer only asks for the size needed.
    let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return None;
    }
    let mut buf = vec![0u16; needed as usize];
    // SAFETY: `buf` holds `needed` u16s, which is the capacity passed.
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), needed) };
    if len == 0 || len >= needed {
        return None;
    }
    String::from_utf16(&buf[..len as usize]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens_and_home() {
        let r = Redactor::new()
            .with_secret("eyJhbGciOiJ.secret.token")
            .with_home("C:\\Users\\mmxtu");
        let out = r.redact("token=eyJhbGciOiJ.secret.token dir=C:\\Users\\mmxtu\\AppData");
        assert_eq!(out, "token=<redacted> dir=~\\AppData");
    }

    #[test]
    fn the_home_path_is_masked_in_its_backslash_escaped_form_too() {
        let r = Redactor::new().with_home(r"C:\Users\steve");
        assert_eq!(r.redact(r"C:\Users\steve\x"), r"~\x");
        assert_eq!(r.redact(r"C:\\Users\\steve\\x"), r"~\\x");
        assert_eq!(r.redact_cut(r"C:\\Users\\steve\\x"), r"~\\x");
    }

    #[test]
    fn the_forward_slash_form_of_a_windows_home_is_masked_too() {
        let r = Redactor::new().with_home(r"C:\Users\steve");
        assert_eq!(
            r.redact("game dir C:/Users/steve/AppData/x"),
            "game dir ~/AppData/x"
        );
        // A POSIX home is left as it is.
        let posix = Redactor::new().with_home("/home/steve");
        assert_eq!(posix.redact("/home/steve/x"), "~/x");
    }

    #[test]
    fn a_secret_cut_in_half_is_masked() {
        let r = Redactor::new().with_secret("eyJhbGciOiJ.secret.token");
        // The line was cut after "eyJhbGciOiJ.sec": `redact` alone can't see a full secret.
        let cut = "--accessToken eyJhbGciOiJ.sec";
        assert_eq!(r.redact(cut), cut);
        assert_eq!(r.redact_cut(cut), "--accessToken <redacted>");
        // Whole secrets earlier in the line are still replaced.
        assert_eq!(
            r.redact_cut("a eyJhbGciOiJ.secret.token b eyJh"),
            "a <redacted> b <redacted>"
        );
        // Fragments under 4 characters are left alone; so is an unrelated ending.
        assert_eq!(r.redact_cut("ends in eyJ"), "ends in eyJ");
        assert_eq!(r.redact_cut("plain text"), "plain text");
    }

    #[test]
    fn a_windows_home_matches_in_any_letter_case() {
        let r = Redactor::new().with_home(r"C:\Users\steve");
        assert_eq!(r.redact(r"c:\users\STEVE\x"), r"~\x");
        assert_eq!(r.redact(r"c:\\USERS\\Steve\\x"), r"~\\x");
        assert_eq!(r.redact("c:/users/STEVE/x"), "~/x");
        assert_eq!(r.redact_cut(r"C:\USERS\STEVE\x"), r"~\x");
        // A drive-letter home with no backslash is Windows-style too.
        let fwd = Redactor::new().with_home("C:/Users/steve");
        assert_eq!(fwd.redact("c:/USERS/steve/x"), "~/x");
        // Non-ASCII text around a match keeps its place.
        assert_eq!(r.redact(r"ñ C:\USERS\steve ñ"), "ñ ~ ñ");
    }

    #[test]
    fn a_posix_home_stays_case_sensitive() {
        let r = Redactor::new().with_home("/home/steve");
        assert_eq!(r.redact("/home/STEVE/x"), "/home/STEVE/x");
        assert_eq!(r.redact("/home/steve/x"), "~/x");
    }

    #[test]
    fn every_home_alias_is_masked_and_longer_forms_go_first() {
        let r = Redactor::new()
            .with_home(r"C:\Users\steve")
            .with_home_alias(r"C:\Users\STEVE~1")
            .with_home_alias(r"C:\Users\steve.old");
        assert_eq!(r.redact(r"C:\Users\STEVE~1\x"), r"~\x");
        assert_eq!(r.redact(r"C:\\Users\\STEVE~1\\x"), r"~\\x");
        assert_eq!(r.redact("C:/Users/STEVE~1/x"), "~/x");
        // "C:\Users\steve" is a prefix of the alias: the alias must win, leaving no ".old".
        assert_eq!(r.redact(r"C:\Users\steve.old\x"), r"~\x");
        assert_eq!(r.redact(r"C:\Users\steve\x"), r"~\x");
        // Order of registration doesn't matter.
        let r = Redactor::new()
            .with_home_alias(r"C:\Users\steve")
            .with_home(r"C:\Users\steve.old");
        assert_eq!(r.redact(r"C:\Users\steve.old\x"), r"~\x");
    }

    #[test]
    fn debug_never_prints_secrets() {
        let r = Redactor::new()
            .with_secret("supersecrettoken")
            .with_home(r"C:\Users\steve");
        assert!(!format!("{r:?}").contains("supersecrettoken"));
    }

    #[cfg(windows)]
    #[test]
    fn the_short_form_of_the_home_is_masked() {
        let tmp = tempfile::tempdir().unwrap();
        let long = tmp.path().join("a very long folder name 123");
        std::fs::create_dir_all(&long).unwrap();
        let long = long.to_str().unwrap().to_string();
        let r = Redactor::for_home(long.clone());
        assert_eq!(r.redact(&format!(r"{long}\x")), r"~\x");
        // Volumes with 8.3 names turned off hand back the same path (or nothing).
        if let Some(short) = short_path(&long).filter(|s| !s.eq_ignore_ascii_case(&long)) {
            assert_eq!(r.redact(&format!(r"{short}\x")), r"~\x", "{short}");
        }
    }

    #[test]
    fn short_secrets_are_ignored() {
        let r = Redactor::new().with_secret("0").with_secret("");
        assert_eq!(
            r.redact("Loaded 100 chunks in 0.5s"),
            "Loaded 100 chunks in 0.5s"
        );
    }
}
