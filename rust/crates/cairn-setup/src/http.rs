// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The one way the launcher fetches things. Everything that downloads takes an [`Http`], so the tests serve their files from memory.

use std::io::Read;
use std::time::Duration;

/// An answer: the status, the size the server announced (when it did) and the body to read.
pub struct HttpResponse {
    pub status: u16,
    pub content_length: Option<u64>,
    pub body: Box<dyn Read + Send>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as text, read up to `limit` bytes (a longer answer is cut).
    pub fn text(self, limit: u64) -> std::io::Result<String> {
        let mut bytes = Vec::new();
        self.body.take(limit).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// The server could not be reached (offline, DNS, TLS, timeout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError(pub String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HttpError {}

pub trait Http: Send + Sync {
    /// A GET, asking for `accept` when given. A status other than 2xx is an answer, not an error.
    fn get_with(&self, url: &str, accept: Option<&str>) -> Result<HttpResponse, HttpError>;

    fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
        self.get_with(url, None)
    }
}

/// The real network, with the launcher's user agent.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new(user_agent: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(user_agent)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        Self { agent: config.into() }
    }
}

impl Http for UreqHttp {
    fn get_with(&self, url: &str, accept: Option<&str>) -> Result<HttpResponse, HttpError> {
        let mut req = self.agent.get(url);
        if let Some(a) = accept {
            req = req.header("Accept", a);
        }
        let resp = req.call().map_err(|e| HttpError(e.to_string()))?;
        let status = resp.status().as_u16();
        let content_length = resp.body().content_length();
        Ok(HttpResponse {
            status,
            content_length,
            body: Box::new(resp.into_body().into_reader()),
        })
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::sync::Mutex;

    /// Serves fixed bodies by URL; an unknown URL is a 404, and `Unreachable` answers every request with an error.
    #[derive(Default)]
    pub struct FakeHttp {
        pub files: Mutex<HashMap<String, (u16, Vec<u8>)>>,
        pub unreachable: bool,
        pub asked: Mutex<Vec<String>>,
    }

    impl FakeHttp {
        pub fn serving(url: &str, body: &[u8]) -> Self {
            let f = Self::default();
            f.put(url, 200, body);
            f
        }
        pub fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.files.lock().unwrap().insert(url.to_string(), (status, body.to_vec()));
        }
    }

    impl Http for FakeHttp {
        fn get_with(&self, url: &str, _accept: Option<&str>) -> Result<HttpResponse, HttpError> {
            self.asked.lock().unwrap().push(url.to_string());
            if self.unreachable {
                return Err(HttpError("no network".into()));
            }
            let (status, body) = self.files.lock().unwrap().get(url).cloned().unwrap_or((404, Vec::new()));
            Ok(HttpResponse {
                status,
                content_length: Some(body.len() as u64),
                body: Box::new(Cursor::new(body)),
            })
        }
    }
}
