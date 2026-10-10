//! The one HTTP client setup every launcher request goes through.
use std::time::Duration;

use crate::error::{ErrorCode, LauncherError, Result};

/// The descriptive `User-Agent` sent with every request.
pub fn user_agent(launcher_version: &str) -> String {
    format!("PerlandLauncher/{launcher_version} (+https://github.com/PerlandDev/perland-launcher)")
}

/// A client with the launcher's user agent and timeouts, so a stalled connection fails instead of
/// hanging forever. The read timeout applies per read, so large downloads still work.
pub fn client(launcher_version: &str) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(user_agent(launcher_version))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| {
            LauncherError::network(
                ErrorCode::HttpClientFailed,
                "Couldn't start the HTTP client",
            )
            .with_detail(e)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_names_the_launcher() {
        assert_eq!(
            user_agent("0.1.0"),
            "PerlandLauncher/0.1.0 (+https://github.com/PerlandDev/perland-launcher)"
        );
    }

    #[test]
    fn client_builds() {
        assert!(client("0.1.0").is_ok());
    }
}
