/// Scrubs secrets and the user's home path from anything written to logs.
#[derive(Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
    home: Option<String>,
}

// Manual Debug: the whole point of this type is to hold secrets, so they must never print.
impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redactor")
            .field("secrets", &self.secrets.len())
            .field("home", &self.home)
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
        Self::new().with_home(home)
    }

    pub fn with_home(mut self, home: impl Into<String>) -> Self {
        let home = home.into();
        if !home.is_empty() {
            self.home = Some(home);
        }
        self
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
        if let Some(home) = &self.home {
            // Debug-formatted log fields double every backslash, so mask that form too.
            if home.contains('\\') {
                out = out.replace(home.replace('\\', "\\\\").as_str(), "~");
            }
            out = out.replace(home.as_str(), "~");
        }
        out
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
    fn short_secrets_are_ignored() {
        let r = Redactor::new().with_secret("0").with_secret("");
        assert_eq!(
            r.redact("Loaded 100 chunks in 0.5s"),
            "Loaded 100 chunks in 0.5s"
        );
    }
}
