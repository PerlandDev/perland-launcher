//! The operating system's language, for a player who never picked one in the launcher.

/// The user's language tag as the system gives it (`es-MX`, `en_US.UTF-8`), if it says.
/// [`super::Client::new`] cuts it down to a short tag.
#[cfg(windows)]
pub fn os_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    // LOCALE_NAME_MAX_LENGTH.
    let mut name = [0u16; 85];
    // SAFETY: the pointer is to `name`, and the length passed is its length.
    let len = unsafe { GetUserDefaultLocaleName(name.as_mut_ptr(), name.len() as i32) };
    // The length counts the closing NUL.
    let len = usize::try_from(len).ok()?.checked_sub(1)?;
    (len > 0).then(|| String::from_utf16_lossy(&name[..len]))
}

#[cfg(not(windows))]
pub fn os_locale() -> Option<String> {
    from_env(|name| std::env::var(name).ok())
}

/// POSIX order: `LC_ALL`, then `LC_MESSAGES`, then `LANG`. `C` and `POSIX` say nothing.
#[cfg_attr(windows, allow(dead_code))]
fn from_env(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(var)
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .filter(|value| {
            let language = value.split(['.', '@']).next().unwrap_or("");
            !matches!(language, "C" | "POSIX")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn the_first_set_variable_wins_and_c_says_nothing() {
        assert_eq!(
            from_env(env(&[("LANG", "es_MX.UTF-8")])).as_deref(),
            Some("es_MX.UTF-8")
        );
        assert_eq!(
            from_env(env(&[("LANG", "es_MX.UTF-8"), ("LC_ALL", "en_US")])).as_deref(),
            Some("en_US")
        );
        assert_eq!(
            from_env(env(&[("LC_ALL", ""), ("LC_MESSAGES", "pt_BR")])).as_deref(),
            Some("pt_BR")
        );
        assert_eq!(from_env(env(&[("LANG", "C.UTF-8")])), None);
        assert_eq!(from_env(env(&[("LANG", "POSIX")])), None);
        assert_eq!(from_env(env(&[])), None);
    }

    #[test]
    fn whatever_the_system_says_becomes_a_short_tag() {
        let tag = |raw: &str| crate::telemetry::Client::new("1", "linux", "x86_64", raw).locale;
        assert_eq!(tag("es_MX.UTF-8"), "es-MX");
        assert_eq!(tag("en-US"), "en-US");
        assert_eq!(tag("sr_RS@latin"), "sr-RS");
        assert_eq!(tag("zh-Hans-CN"), "zh-Hans-");
    }
}
