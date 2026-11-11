//! The two requests, sent over the launcher's own HTTP client with no credential. A send never
//! fails loudly: it tells its caller whether to forget the body, drop it, or keep it.
use std::time::Duration;

use serde::Serialize;

use super::payload::{ErrorBatch, Ping};
use crate::{http::HttpClient, invite::Url};

/// A request gets this long, start to finish.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The Engine took it (any 2xx).
    Delivered,
    /// The Engine will never take this body (400 or 413): drop it.
    Rejected,
    /// Anything else, a timeout or no network: keep it for the next start.
    Retry,
}

#[derive(Debug, Clone)]
pub struct Sender {
    http: HttpClient,
    timeout: Duration,
}

impl Sender {
    pub fn new(http: HttpClient) -> Self {
        Self {
            http,
            timeout: SEND_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// `POST {engine}/api/v1/telemetry/errors`.
    pub async fn errors(&self, engine: &Url, batch: &ErrorBatch) -> Outcome {
        self.post(engine, "/api/v1/telemetry/errors", batch).await
    }

    /// `POST {engine}/api/v1/telemetry/launcher/ping`.
    pub async fn ping(&self, engine: &Url, ping: &Ping) -> Outcome {
        self.post(engine, "/api/v1/telemetry/launcher/ping", ping)
            .await
    }

    async fn post(&self, engine: &Url, route: &str, body: &impl Serialize) -> Outcome {
        let url = format!("{}{route}", engine.as_str().trim_end_matches('/'));
        let sent = self
            .http
            .post(url)
            .timeout(self.timeout)
            .json(body)
            .send()
            .await;
        match sent.map(|response| response.status().as_u16()) {
            Ok(200..=299) => Outcome::Delivered,
            Ok(400 | 413) => Outcome::Rejected,
            Ok(status) => {
                tracing::debug!(status, "usage data not taken; kept for later");
                Outcome::Retry
            }
            Err(_) => Outcome::Retry,
        }
    }
}
