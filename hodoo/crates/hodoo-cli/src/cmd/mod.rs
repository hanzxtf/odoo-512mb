//! One module per noun, each a thin translation from arguments to client calls.
//!
//! The rule everywhere: resolve references through [`crate::refs`], build the fields,
//! honour `--dry-run` before sending anything, then render. No module reaches for
//! `reqwest`, and only [`crate::main`] decides exit codes.

pub mod account;
pub mod board;
pub mod call;
pub mod completions;
pub mod milestone;
pub mod project;
pub mod stage;
pub mod tag;
pub mod task;

use std::collections::HashMap;
use std::time::Duration;

use hodoo::{Client, Config, Id};
use serde_json::{Value, json};

use crate::Failure;
use crate::cli::Global;
use crate::output::{ColorChoice, Column, Output, Table, human_due, plain_text};
use crate::refs;

/// Everything a command needs: where to talk, and how to show it.
pub struct Ctx {
    /// The client, built once per run.
    pub client: Client,
    /// How output is rendered.
    pub out: Output,
    /// `--dry-run`: show what would be sent, send nothing.
    pub dry_run: bool,
    /// `--no-input`: never prompt.
    pub no_input: bool,
    /// The server that was resolved, for `whoami` and `version` to name.
    url: String,
}

impl Ctx {
    /// Builds the client and resolves the output settings.
    ///
    /// # Errors
    ///
    /// [`Failure::Usage`] when the invocation cannot work - no server, a bad output
    /// format, an unknown colour - which is why those exit 2 rather than 1.
    pub fn new(global: &Global) -> Result<Self, Failure> {
        let file = hodoo::dotenv::load(".").map(|file| file.unwrap_or_default())?;

        let url = global
            .url
            .clone()
            .or_else(|| hodoo::dotenv::resolve("ODOO_URL", &file))
            .ok_or_else(|| {
                Failure::Usage(
                    "no server to talk to. Pass --url, set ODOO_URL, or put ODOO_URL in a .env"
                        .to_owned(),
                )
            })?;
        let api_key = global
            .api_key
            .clone()
            .or_else(|| hodoo::dotenv::resolve("ODOO_API_KEY", &file))
            .unwrap_or_default();
        let db = global
            .db
            .clone()
            .or_else(|| hodoo::dotenv::resolve("ODOO_DB", &file));

        let mut config = Config::new(&url, api_key)?
            .with_timeout(Duration::from_secs(global.timeout))
            .accept_invalid_certs(global.insecure)
            .retry_reads(!global.no_retry);
        if let Some(db) = db {
            config = config.with_db(db);
        }

        let color = match global.color.as_deref() {
            None | Some("auto") => None,
            Some("always") => Some(ColorChoice::Always),
            Some("never") => Some(ColorChoice::Never),
            Some(other) => {
                return Err(Failure::Usage(format!(
                    "--color {other:?} is not a choice: try auto, always or never"
                )));
            }
        };

        Ok(Self {
            client: Client::new(config)?,
            out: Output::resolve(
                global.output.as_deref(),
                global.json,
                color,
                global.no_headers,
                global.quiet,
                global.pretty,
            )?,
            dry_run: global.dry_run,
            no_input: global.no_input,
            url,
        })
    }

    /// The server this run is talking to.
    #[must_use]
    pub fn url(&self) -> String {
        self.url.clone()
    }
}

/// The user behind the API key, for `--mine` and `--assignee me`.
///
/// # Errors
///
/// Any error of the `res.users/context_get` call, whose `uid` is the key's user.
pub async fn me(ctx: &Ctx) -> Result<Id<hodoo::id::User>, Failure> {
    let context = ctx.client.whoami().await?;
    Ok(Id::new(
        context
            .get("uid")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    ))
}

/// Names for a set of ids, in one call, so tables can show records by name instead
/// of by number.
///
/// Best effort on purpose: a name is decoration, and a key that may read projects but
/// not users should still get its table. A failure here leaves dashes in one column
/// rather than failing the command.
pub async fn names(
    ctx: &Ctx,
    model: &str,
    ids: impl IntoIterator<Item = i64>,
) -> Result<HashMap<i64, String>, Failure> {
    let mut ids: Vec<i64> = ids.into_iter().collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let Ok(rows) = ctx
        .client
        .call(model, "read", json!({ "ids": ids, "fields": ["name"] }))
        .await
    else {
        return Ok(HashMap::new());
    };
    let mut found = HashMap::new();
    for row in rows.as_array().map(Vec::as_slice).unwrap_or_default() {
        if let (Some(id), Some(name)) = (
            row.get("id").and_then(Value::as_i64),
            row.get("name").and_then(Value::as_str),
        ) {
            found.insert(id, name.to_owned());
        }
    }
    Ok(found)
}

/// Looks one name up, for a single-record view. Also best effort.
///
/// # Errors
///
/// Any error of [`names`], which itself only fails when Odoo does.
pub async fn name_of(ctx: &Ctx, model: &str, id: i64) -> Result<Option<String>, Failure> {
    Ok(names(ctx, model, [id]).await?.remove(&id))
}

/// A record's name for a message, or `#id` when the lookup found nothing.
///
/// Best effort like [`name_of`]: a name is decoration in a sentence, and a key that
/// may not read the model should not fail the command it was decorating.
pub async fn name_or_id(ctx: &Ctx, model: &str, id: i64) -> String {
    name_of(ctx, model, id)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("#{id}"))
}

/// How many records match a domain, for the sentences that say what a delete takes
/// with it.
///
/// Best effort on purpose: the count is a courtesy in a confirmation, and a delete
/// the user asked for should not fail because a counting read did. `None` means Odoo
/// did not answer with a number, and the caller phrases its sentence without one.
pub async fn count_of(ctx: &Ctx, model: &str, domain: Value) -> Option<i64> {
    ctx.client
        .call(model, "search_count", json!({ "domain": domain }))
        .await
        .ok()?
        .as_i64()
}

/// A record's chatter, newest last, flattened for printing: when, who, what.
///
/// # Errors
///
/// Any error of the underlying `mail.message` search.
pub async fn chatter(
    ctx: &Ctx,
    model: &str,
    id: i64,
    limit: u32,
) -> Result<Vec<(String, String, String)>, Failure> {
    let rows = ctx
        .client
        .call(
            "mail.message",
            "search_read",
            json!({
                "domain": [["model", "=", model], ["res_id", "=", id]],
                "fields": ["body", "author_id", "date"],
                "order": "id desc",
                "limit": limit,
            }),
        )
        .await?;
    let rows = rows.as_array().cloned().unwrap_or_default();
    let authors = names(
        ctx,
        "res.partner",
        rows.iter().filter_map(|row| {
            row.get("author_id")
                .and_then(Value::as_array)
                .and_then(|pair| pair.first())
                .and_then(Value::as_i64)
        }),
    )
    .await?;

    let now = chrono::Utc::now();
    let mut flattened = Vec::new();
    for row in rows.iter().rev() {
        let body = row
            .get("body")
            .and_then(Value::as_str)
            .map(plain_text)
            .unwrap_or_default();
        if body.is_empty() {
            continue;
        }
        let when = row
            .get("date")
            .and_then(Value::as_str)
            .and_then(|text| hodoo::datetime::parse_datetime(text).ok())
            .and_then(|stamp| human_due(Some(stamp), now))
            .map(|(text, _)| text)
            .unwrap_or_else(|| "-".to_owned());
        let who = row
            .get("author_id")
            .and_then(Value::as_array)
            .and_then(|pair| pair.first())
            .and_then(Value::as_i64)
            .and_then(|id| authors.get(&id).cloned())
            .unwrap_or_else(|| "-".to_owned());
        flattened.push((when, who, clip(&body, 90)));
    }
    Ok(flattened)
}

/// Renders a chatter list as a block, and prints nothing when it is empty.
///
/// # Errors
///
/// The I/O error of writing to stdout.
pub fn chatter_block(ctx: &Ctx, messages: &[(String, String, String)]) -> std::io::Result<()> {
    if messages.is_empty() {
        return Ok(());
    }
    let mut table = Table::new(vec![
        Column::text("WHEN"),
        Column::text("WHO"),
        Column::text("MESSAGE").flexible(),
    ]);
    for (when, who, body) in messages {
        table.push([when.clone(), who.clone(), body.clone()]);
    }
    ctx.out.show(format!(
        "{}\n{}",
        ctx.out.bold("chatter"),
        table.render(ctx.out).trim_end()
    ))
}

/// Shortens text for a table cell.
#[must_use]
pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

/// Serializes a value for JSON output, or null: a display detail must never fail
/// the command it belongs to.
#[must_use]
pub fn as_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Resolves tag names, creating the ones that do not exist yet, so `--tag urgent`
/// works on a fresh database.
///
/// # Errors
///
/// Any error of the underlying tag search or create.
pub async fn tags_of(ctx: &Ctx, tags: &[String]) -> Result<Vec<Id<hodoo::Tag>>, Failure> {
    let mut ids = Vec::new();
    for tag in tags {
        ids.push(refs::tag(&ctx.client, tag).await?);
    }
    Ok(ids)
}

/// `--limit 0` means "no limit".
#[must_use]
pub fn limit_of(limit: u32) -> Option<u32> {
    if limit == 0 { None } else { Some(limit) }
}

/// `--offset 0` means "from the start".
#[must_use]
pub fn offset_of(offset: u32) -> Option<u32> {
    if offset == 0 { None } else { Some(offset) }
}
