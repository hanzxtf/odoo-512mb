//! Errors returned by [`Client`](crate::Client).

use serde_json::Value;

/// Result alias used throughout this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while talking to Odoo.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Rejected before a request was sent: a bad base URL, no API key, a value
    /// that is not a date.
    #[error("configuration error: {message}")]
    Config {
        /// What was wrong, in a form meant for a human.
        message: String,
    },

    /// The request never produced an HTTP response, or the connection failed.
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),

    /// Odoo answered with an HTTP error status and its error object, which is
    /// `{name, message, arguments, context, debug}`; `debug` holds the Python
    /// traceback and can be large or reveal server paths.
    #[error("Odoo error {status}: {message}")]
    Odoo {
        /// The HTTP status Odoo replied with.
        status: u16,
        /// The fully qualified Python exception name.
        name: String,
        /// The exception message.
        message: String,
        /// The exception arguments, as sent.
        arguments: Vec<Value>,
        /// The Python traceback, when the server included one.
        debug: Option<String>,
    },

    /// A response that was not the expected JSON object, such as nginx's HTML
    /// 502 page or a 200 from an endpoint that does not speak JSON-2.
    #[error("unexpected response ({status}): {body}")]
    UnexpectedResponse {
        /// The HTTP status of the unexpected response.
        status: u16,
        /// The response body, truncated to a readable size.
        body: String,
    },

    /// A single record was asked for and Odoo returned none, which usually
    /// means it was deleted since it was last read.
    #[error("{model} {raw_id} not found")]
    Missing {
        /// The Odoo model that was searched.
        model: String,
        /// The id that produced nothing.
        raw_id: i64,
    },

    /// A successful response whose body did not match the shape this crate
    /// expects. The body is kept for inspection.
    #[error("could not decode the response")]
    Decode {
        /// The underlying serde error.
        #[source]
        source: serde_json::Error,
        /// The body that failed to deserialize.
        body: Value,
    },
}

impl Error {
    /// The HTTP status behind this error, when there was one.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Odoo { status, .. } | Error::UnexpectedResponse { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Whether this is an authentication failure, so a caller can tell "wrong
    /// key" apart from "Odoo said no".
    #[must_use]
    pub fn is_unauthorized(&self) -> bool {
        match self {
            Error::Odoo { status, name, .. } => *status == 401 || name.ends_with("Unauthorized"),
            _ => false,
        }
    }

    /// Whether the model, method or record did not exist.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        match self {
            Error::Odoo { status, .. } => *status == 404,
            Error::Missing { .. } => true,
            _ => false,
        }
    }

    /// A short machine-readable name for this error, used by the CLI's JSON
    /// output.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Config { .. } => "config",
            Error::Transport(_) => "transport",
            Error::Odoo { .. } => "odoo",
            Error::UnexpectedResponse { .. } => "unexpected_response",
            Error::Missing { .. } => "missing",
            Error::Decode { .. } => "decode",
        }
    }

    /// Connection failures and gateway errors are worth one more attempt on a
    /// read. This box runs Odoo and PostgreSQL on 512 MB, so the OOM killer
    /// does take Odoo down and nginx does answer 502 while it restarts.
    pub(crate) fn is_retryable(&self) -> bool {
        match self {
            Error::Transport(_) => true,
            Error::Odoo { status, .. } | Error::UnexpectedResponse { status, .. } => {
                matches!(*status, 502..=504)
            }
            _ => false,
        }
    }

    /// Caps the traceback and response body this crate keeps in memory.
    pub(crate) fn truncate(mut text: String) -> String {
        const LIMIT: usize = 4096;
        if text.len() <= LIMIT {
            return text;
        }
        let cut = (0..=LIMIT)
            .rev()
            .find(|i| text.is_char_boundary(*i))
            .unwrap_or(0);
        text.truncate(cut);
        text.push_str("... (truncated)");
        text
    }
}
