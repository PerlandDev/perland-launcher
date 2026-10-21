//! The plumbing every Engine client shares: one request, its capped answer, the envelope, and
//! the refusal fallback by status. Credentials go in headers only and nothing here prints one.
//!
//! The `reqwest::Client` given to a client built on this must not follow redirects (use
//! `http::client_without_redirects`): a redirect would carry the credential header to
//! whatever host it points at.

use std::time::Duration;

use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;

use crate::{
    error::{LauncherError, Result},
    http::read_capped,
    invite::client::{host_of, host_trouble, rate_limited, request_failed, unexpected},
};

use super::api::signed_out;

/// The longest error body read, in bytes.
const ERROR_CAP: usize = 4 << 10;

/// How a request proves who sends it.
#[derive(Clone, Copy)]
pub(super) enum Credential<'a> {
    None,
    /// `Authorization: Bearer <token>`
    Bearer(&'a str),
    /// `X-User-Token: <jwt>`
    UserToken(&'a str),
}

pub(super) struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

/// What a failed request's error data says.
pub(super) struct Failure {
    pub code: Option<String>,
    pub data: Value,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
}

/// An Engine to send to.
#[derive(Clone)]
pub(super) struct Wire {
    http: reqwest::Client,
    engine: Url,
}

impl Wire {
    pub fn new(http: reqwest::Client, engine: Url) -> Self {
        Self { http, engine }
    }

    pub fn engine(&self) -> &Url {
        &self.engine
    }

    pub fn host(&self) -> String {
        host_of(&self.engine)
    }

    /// `path` is the full path, such as `/api/v1/auth/memberships`.
    fn url(&self, path: &str) -> Url {
        Url::parse(&format!(
            "{}{path}",
            self.engine.as_str().trim_end_matches('/')
        ))
        .expect("a checked Engine URL plus a fixed path is a URL")
    }

    /// One request. Any status comes back as a `Reply`; only a request that got no answer, or
    /// a success too large to read, is an error here. An error body is read up to 4 KiB (and
    /// dropped when longer).
    pub async fn send(
        &self,
        timeout: Duration,
        method: Method,
        path: &str,
        credential: Credential<'_>,
        body: Option<&Value>,
        cap: usize,
    ) -> Result<Reply> {
        let host = self.host();
        let mut request = self.http.request(method, self.url(path)).timeout(timeout);
        request = match credential {
            Credential::None => request,
            Credential::Bearer(token) => request.bearer_auth(token),
            Credential::UserToken(jwt) => request.header("X-User-Token", jwt),
        };
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|e| request_failed(&host, e))?;
        let status = response.status().as_u16();
        let ok = response.status().is_success();
        let read = read_capped(response, if ok { cap } else { ERROR_CAP }).await;
        let body = match read {
            Ok(Some(body)) => body,
            Ok(None) if ok => return Err(unexpected(&host, "the answer is too large")),
            Ok(None) => Vec::new(),
            Err(e) if ok => return Err(request_failed(&host, e)),
            Err(_) => Vec::new(),
        };
        Ok(Reply { status, body })
    }

    /// The `data` of a success. The parser's own message can quote the body, so it is never
    /// shown.
    pub fn data<T: DeserializeOwned>(&self, reply: &Reply) -> Result<T> {
        serde_json::from_slice::<Envelope<T>>(&reply.body)
            .ok()
            .and_then(|e| e.data)
            .ok_or_else(|| unexpected(&self.host(), "the answer isn't valid"))
    }
}

pub(super) fn failure(reply: &Reply) -> Failure {
    let data = serde_json::from_slice::<Value>(&reply.body)
        .ok()
        .and_then(|mut v| v.get_mut("data").map(Value::take))
        .unwrap_or(Value::Null);
    let code = data.get("code").and_then(Value::as_str).map(str::to_string);
    Failure { code, data }
}

/// What a refusal means when no code did: 401 signed out, 429 rate limited, 400 unexpected,
/// else the host in trouble.
pub(super) fn status_fallback(host: &str, status: u16) -> LauncherError {
    match status {
        401 => signed_out(host),
        429 => rate_limited(host),
        400 => unexpected(host, "the request was refused"),
        s => host_trouble(host, s),
    }
}
