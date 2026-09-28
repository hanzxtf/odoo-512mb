//! A typed client for Odoo 19's JSON-2 API, aimed at project management.
//!
//! Odoo 19 replaced the XML-RPC and JSON-RPC endpoints with JSON-2:
//! `POST /json/2/<model>/<method>` with an API key as a bearer token, named
//! arguments in the body, and every error reported as an HTTP status plus a
//! JSON object. Everything this crate does is that one call shape.
//!
//! # Configuration
//!
//! An API key belongs to a user and inherits that user's access rights and
//! record rules. Create one in Odoo under *Preferences > Account Security >
//! New API Key*; keys last at most three months and are shown once.
//!
//! ```no_run
//! use std::time::Duration;
//! use hodoo::{Client, Config, ProjectFields, TaskFields, TaskFilter};
//!
//! # async fn run() -> hodoo::Result<()> {
//! let config = Config::new("https://odoo.example.com", std::env::var("ODOO_API_KEY").unwrap_or_default())?
//!     .with_timeout(Duration::from_secs(60))
//!     .retry_reads(true);
//! let client = Client::new(config)?;
//!
//! let project = client.projects().create(ProjectFields::new("Website")).await?;
//! let task = client
//!     .tasks()
//!     .create(TaskFields { name: Some("Write the copy".into()), project: Some(project), ..Default::default() })
//!     .await?;
//! let open = client.tasks().search(TaskFilter { project: Some(project), open_only: true, ..Default::default() }).await?;
//! println!("{task} is one of {} open tasks", open.len());
//! # Ok(())
//! # }
//! ```
//!
//! # Anything this crate does not model
//!
//! [`Client::call`] takes any model and method. Odoo's own dynamic
//! documentation at `/doc` (and `/doc-bearer/<model>.json`, for which the API
//! key's user needs the Settings group) lists what exists on a given database.
//!
//! ```no_run
//! # use hodoo::{Client, Config};
//! # use serde_json::json;
//! # async fn run(client: Client) -> hodoo::Result<()> {
//! let partners = client
//!     .call("res.partner", "search_read", json!({ "domain": [], "fields": ["name"], "limit": 5 }))
//!     .await?;
//! # Ok(())
//! # }
//! ```

// The lints strictness this crate holds itself to does not apply to its own
// tests, which assert by unwrapping.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

pub mod chatter;
pub mod datetime;
pub mod dotenv;
pub mod error;
pub mod id;
pub mod milestone;
pub mod project;
pub mod stage;
pub mod tag;
pub mod task;

mod de;
mod http;
mod query;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use chatter::Message;
pub use error::{Error, Result};
pub use id::{
    Id, MilestoneId, PartnerId, ProjectId, ProjectStageId, TagId, TaskId, TaskStageId, UserId,
};
pub use milestone::{Milestone, MilestoneFields};
pub use project::{Project, ProjectFields, ProjectFilter, Visibility};
pub use stage::{StageFields, StageFilter, TaskStage};
pub use tag::Tag;
pub use task::{Priority, Task, TaskFields, TaskFilter, TaskState};

use crate::http::Transport;

/// Connection settings.
///
/// The API key is never printed: [`Debug`](fmt::Debug) redacts it, so a config
/// can be logged next to the request it belongs to.
#[derive(Clone)]
pub struct Config {
    base_url: Url,
    api_key: String,
    db: Option<String>,
    timeout: Duration,
    insecure_cert: bool,
    retry_reads: bool,
}

impl Config {
    /// Builds a config. `base_url` is the Odoo server, with or without a path
    /// prefix; its query and fragment are ignored. An empty `api_key` is
    /// accepted so that [`Client::version`] works before a key exists, and
    /// every other call fails with [`Error::Config`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the URL is not absolute `http` or `https`,
    /// or has no host.
    pub fn new(base_url: impl AsRef<str>, api_key: impl Into<String>) -> Result<Self> {
        let mut url = Url::parse(base_url.as_ref()).map_err(|error| Error::Config {
            message: format!("{} is not a valid URL: {error}", base_url.as_ref()),
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::Config {
                message: format!("only http and https are supported, got {}", url.scheme()),
            });
        }
        if url.host_str().is_none() {
            return Err(Error::Config {
                message: format!("{url} has no host"),
            });
        }
        url.set_query(None);
        url.set_fragment(None);
        if !url.path().ends_with('/') {
            let path = format!("{}/", url.path());
            url.set_path(&path);
        }
        Ok(Self {
            base_url: url,
            api_key: api_key.into(),
            db: None,
            timeout: Duration::from_secs(30),
            insecure_cert: false,
            retry_reads: true,
        })
    }

    /// Builds a config from the process environment: `ODOO_URL` (required),
    /// `ODOO_API_KEY`, `ODOO_DB`.
    ///
    /// A `.env` file is *not* consulted here, because reading one is a side
    /// effect a library should not perform on its own. Load it with
    /// [`dotenv::load`] and pass the values, or let the `hodoo` CLI do the
    /// layering for you.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] when `ODOO_URL` is unset or is not a usable URL.
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("ODOO_URL").map_err(|_| Error::Config {
            message: "ODOO_URL is not set".into(),
        })?;
        let key = std::env::var("ODOO_API_KEY").unwrap_or_default();
        let mut config = Self::new(url, key)?;
        if let Ok(db) = std::env::var("ODOO_DB") {
            config = config.with_db(db);
        }
        Ok(config)
    }

    /// Sends the `X-Odoo-Database` header. Only needed when one server hosts
    /// several databases and the domain does not select one.
    #[must_use]
    pub fn with_db(mut self, db: impl Into<String>) -> Self {
        self.db = Some(db.into());
        self
    }

    /// Per-request timeout. Defaults to 30 seconds, which a first-time module
    /// init or a large search can exceed.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Accepts an invalid or self-signed certificate. The vhost this crate was
    /// written for ships a self-signed one until a real certificate replaces
    /// it.
    #[must_use]
    pub fn accept_invalid_certs(mut self, yes: bool) -> Self {
        self.insecure_cert = yes;
        self
    }

    /// Whether read-only calls are retried once after a connection failure or a
    /// 502/503/504. Defaults to `true`; writes are never retried.
    #[must_use]
    pub fn retry_reads(mut self, yes: bool) -> Self {
        self.retry_reads = yes;
        self
    }

    pub(crate) fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub(crate) fn api_key(&self) -> &str {
        &self.api_key
    }

    pub(crate) fn db(&self) -> Option<&str> {
        self.db.as_deref()
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }

    pub(crate) fn insecure_cert(&self) -> bool {
        self.insecure_cert
    }

    pub(crate) fn retries_reads(&self) -> bool {
        self.retry_reads
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url.as_str())
            .field("api_key", &"<redacted>")
            .field("db", &self.db)
            .field("timeout", &self.timeout)
            .field("insecure_cert", &self.insecure_cert)
            .field("retry_reads", &self.retry_reads)
            .finish()
    }
}

/// A connection to one Odoo database.
///
/// `Client` is `Clone + Send + Sync` and holds nothing mutable, so it can be
/// shared by a whole application: a web handler resolves it once and a future
/// multi-user deployment builds one per request from that user's own key.
#[derive(Clone, Debug)]
pub struct Client {
    transport: Arc<Transport>,
}

impl Client {
    /// Builds a client, and the connection pool that goes with it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] if the TLS backend or the connection pool
    /// cannot be configured.
    pub fn new(config: Config) -> Result<Self> {
        Ok(Self {
            transport: Arc::new(Transport::new(config)?),
        })
    }

    /// The escape hatch, and the only call this crate makes.
    ///
    /// `body` holds JSON-2's named arguments: `ids` for a method that works on
    /// records, `context` for the call context, and the method's parameters by
    /// name. Positional arguments do not exist in JSON-2.
    ///
    /// # Errors
    ///
    /// Any [`Error`] the endpoint can produce: a missing API key, a transport
    /// failure, Odoo's own error object, or a body that is not JSON.
    pub async fn call(&self, model: &str, method: &str, body: Value) -> Result<Value> {
        self.transport.call(model, method, &body).await
    }

    /// [`call`](Client::call), deserialized into `T`.
    ///
    /// # Errors
    ///
    /// The errors of [`call`](Client::call), plus [`Error::Decode`] when the
    /// result does not match `T`.
    pub async fn call_as<T: DeserializeOwned>(
        &self,
        model: &str,
        method: &str,
        body: Value,
    ) -> Result<T> {
        http::decode(self.call(model, method, body).await?)
    }

    /// The server's Odoo version, from `GET /web/version`. Needs no API key.
    ///
    /// # Errors
    ///
    /// [`Error::UnexpectedResponse`] if the endpoint answers with something
    /// other than its JSON envelope.
    pub async fn version(&self) -> Result<String> {
        let body = self.transport.get("web/version").await?;
        body.get("version")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| Error::UnexpectedResponse {
                status: 200,
                body: body.to_string(),
            })
    }

    /// `project.project`.
    #[must_use]
    pub fn projects(&self) -> project::Projects<'_> {
        project::Projects::new(self)
    }

    /// `project.task`.
    #[must_use]
    pub fn tasks(&self) -> task::Tasks<'_> {
        task::Tasks::new(self)
    }

    /// `project.task.type`, the stages a task moves through.
    #[must_use]
    pub fn stages(&self) -> stage::Stages<'_> {
        stage::Stages::new(self)
    }

    /// `project.milestone`.
    #[must_use]
    pub fn milestones(&self) -> milestone::Milestones<'_> {
        milestone::Milestones::new(self)
    }

    /// `project.tags`.
    #[must_use]
    pub fn tags(&self) -> tag::Tags<'_> {
        tag::Tags::new(self)
    }

    /// The current user's context, which is how a JSON-2 caller learns its own
    /// user id: `res.users/context_get` takes no argument.
    ///
    /// # Errors
    ///
    /// The same errors as [`call`](Client::call).
    pub async fn whoami(&self) -> Result<Value> {
        self.call("res.users", "context_get", json!({})).await
    }
}
