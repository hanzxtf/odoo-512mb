//! Chatter: the messages attached to a record.
//!
//! Both `project.project` and `project.task` inherit `mail.thread`, so the same
//! two operations work on either: post a message, and read the thread.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Client;
use crate::datetime;
use crate::de;
use crate::error::Result;
use crate::id::PartnerId;
use crate::query;

/// The fields this crate reads from `mail.message`.
pub const FIELDS: &[&str] = &[
    "id",
    "body",
    "author_id",
    "date",
    "message_type",
    "subtype_id",
];

/// One entry in a record's chatter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// `mail.message.id`.
    pub id: i64,
    /// The message body, as HTML.
    #[serde(default, deserialize_with = "de::opt_string")]
    pub body: Option<String>,
    /// The author's partner (`author_id`).
    #[serde(default, rename = "author_id", deserialize_with = "de::opt_id")]
    pub author: Option<PartnerId>,
    /// When it was posted.
    #[serde(
        default,
        deserialize_with = "datetime::opt_datetime",
        serialize_with = "datetime::ser_opt_datetime"
    )]
    pub date: Option<DateTime<Utc>>,
    /// `comment`, `notification`, `email`, ...
    #[serde(default, rename = "message_type")]
    pub message_type: String,
    /// The message subtype, which is what tells an internal note from a
    /// comment. `mail.mt_note` is the note.
    #[serde(default, rename = "subtype_id", deserialize_with = "de::opt_raw_id")]
    pub subtype: Option<i64>,
}

/// The subtype of an internal note: only internal users see it.
const NOTE_SUBTYPE: &str = "mail.mt_note";

/// The subtype of a comment: followers see it.
const COMMENT_SUBTYPE: &str = "mail.mt_comment";

/// Posts a message. `body` is plain text and is escaped by Odoo; HTML bodies
/// have to go through [`Client::call`] with `body_is_html`.
pub(crate) async fn comment(
    client: &Client,
    model: &str,
    raw_id: i64,
    body: &str,
    internal: bool,
) -> Result<()> {
    let payload = json!({
        "ids": [raw_id],
        "body": body,
        "message_type": "comment",
        "subtype_xmlid": if internal { NOTE_SUBTYPE } else { COMMENT_SUBTYPE },
    });
    client
        .call(model, "message_post", payload)
        .await
        .map(|_| ())
}

/// Reads a record's thread, oldest first.
pub(crate) async fn messages(
    client: &Client,
    model: &str,
    raw_id: i64,
    limit: Option<u32>,
) -> Result<Vec<Message>> {
    let domain: Value = json!([["model", "=", model], ["res_id", "=", raw_id]]);
    client
        .call_as(
            "mail.message",
            "search_read",
            query::search(domain, FIELDS, limit, None, Some("id asc")),
        )
        .await
}
