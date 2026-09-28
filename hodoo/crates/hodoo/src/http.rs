//! The single place this crate performs HTTP.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::Config;
use crate::error::{Error, Result};

/// Methods that only read. A failure on one of these may be retried; a failure
/// on anything else must not be, because JSON-2 has no idempotency key and a
/// retried `create` would duplicate a record.
const READONLY_METHODS: &[&str] = &[
    "context_get",
    "fields_get",
    "formatted_read_group",
    "name_search",
    "read",
    "read_group",
    "search",
    "search_count",
    "search_read",
    "web_search_read",
];

/// How long to wait before the single retry a read is allowed.
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Odoo's error object, as documented for JSON-2.
#[derive(serde::Deserialize)]
struct OdooError {
    #[serde(default)]
    name: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    arguments: Vec<Value>,
    #[serde(default)]
    debug: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Transport {
    http: reqwest::Client,
    cfg: Arc<Config>,
}

impl Transport {
    pub(crate) fn new(cfg: Config) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(cfg.timeout())
            .danger_accept_invalid_certs(cfg.insecure_cert())
            .user_agent(concat!("hodoo/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            cfg: Arc::new(cfg),
        })
    }

    /// POSTs a JSON-2 call, retrying a read once if the box hiccups.
    pub(crate) async fn call(&self, model: &str, method: &str, body: &Value) -> Result<Value> {
        if self.cfg.api_key().is_empty() {
            return Err(Error::Config {
                message: "no API key: pass one to Config::new (or --api-key / ODOO_API_KEY). \
                          Create one in Odoo under Preferences > Account Security > New API Key"
                    .into(),
            });
        }

        let attempts = if self.cfg.retries_reads() && READONLY_METHODS.contains(&method) {
            2
        } else {
            1
        };
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.attempt(model, method, body).await {
                Ok(value) => return Ok(value),
                Err(error) if attempt >= attempts || !error.is_retryable() => return Err(error),
                Err(_) => tokio::time::sleep(RETRY_DELAY).await,
            }
        }
    }

    /// GETs a non-JSON-2 endpoint, currently only `/web/version`.
    pub(crate) async fn get(&self, path: &str) -> Result<Value> {
        let url = self.join(path)?;
        let response = self.http.get(url).send().await?;
        let status = response.status().as_u16();
        let text = response.text().await?;
        Self::json_or_unexpected(status, text)
    }

    async fn attempt(&self, model: &str, method: &str, body: &Value) -> Result<Value> {
        let url = self.join(&format!("json/2/{model}/{method}"))?;
        let mut request = self
            .http
            .post(url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.cfg.api_key()),
            )
            .json(body);
        if let Some(db) = self.cfg.db() {
            request = request.header("X-Odoo-Database", db);
        }

        let response = request.send().await?;
        let status = response.status().as_u16();
        let text = response.text().await?;
        let value = Self::json_or_unexpected(status, text)?;
        if (200..300).contains(&status) {
            return Ok(value);
        }
        Err(Self::odoo_error(status, value))
    }

    /// Reads the body as JSON, or reports the raw text: a 502 from nginx, or a
    /// 200 from something that is not JSON-2, is not an Odoo error object and
    /// saying so is more useful than "decode failed".
    fn json_or_unexpected(status: u16, text: String) -> Result<Value> {
        serde_json::from_str(&text).map_err(|_| Error::UnexpectedResponse {
            status,
            body: Error::truncate(text),
        })
    }

    fn odoo_error(status: u16, value: Value) -> Error {
        let raw = value.to_string();
        match serde_json::from_value::<OdooError>(value) {
            Ok(error) if !error.name.is_empty() || !error.message.is_empty() => Error::Odoo {
                status,
                name: error.name,
                message: error.message,
                arguments: error.arguments,
                debug: error.debug.map(Error::truncate),
            },
            // A JSON body that is not Odoo's error shape: report it verbatim
            // rather than inventing an empty exception name.
            _ => Error::UnexpectedResponse {
                status,
                body: Error::truncate(raw),
            },
        }
    }

    fn join(&self, path: &str) -> Result<Url> {
        self.cfg
            .base_url()
            .join(path)
            .map_err(|error| Error::Config {
                message: format!("cannot build the request URL: {error}"),
            })
    }
}

/// Deserializes a JSON-2 result, keeping the body when it does not fit.
pub(crate) fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|source| Error::Decode {
        source,
        body: value,
    })
}
