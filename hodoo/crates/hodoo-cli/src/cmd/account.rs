//! `hodoo whoami` and `hodoo version`: is the connection what I think it is?

use serde_json::{Value, json};

use crate::Failure;
use crate::cmd::Ctx;
use crate::output::{Column, Mode, Table};

/// `hodoo whoami`: the user behind the key, and the server it reaches.
///
/// # Errors
///
/// Any error of the context lookup or the version call.
pub async fn whoami(ctx: &Ctx, url: &str) -> Result<(), Failure> {
    let context = ctx.client.whoami().await?;
    let uid = context
        .get("uid")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let user = ctx
        .client
        .call(
            "res.users",
            "read",
            json!({ "ids": [uid], "fields": ["name", "login"] }),
        )
        .await?;
    let first = user.as_array().and_then(|rows| rows.first());
    let name = first
        .and_then(|row| row.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    let login = first
        .and_then(|row| row.get("login"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    let version = ctx
        .client
        .version()
        .await
        .unwrap_or_else(|_| "?".to_owned());

    if ctx.out.mode() == Mode::Json {
        return Ok(ctx.out.print_json(&json!({
            "uid": uid,
            "name": name,
            "login": login,
            "lang": context.get("lang"),
            "server": url,
            "version": version,
        }))?);
    }

    let mut table = Table::new(vec![
        Column::text("FIELD"),
        Column::text("VALUE").flexible(),
    ]);
    table.push(["user".to_owned(), format!("{name} ({login})")]);
    table.push(["uid".to_owned(), uid.to_string()]);
    table.push(["server".to_owned(), format!("{url} · Odoo {version}")]);
    if let Some(lang) = context.get("lang").and_then(Value::as_str) {
        table.push(["language".to_owned(), lang.to_owned()]);
    }
    Ok(ctx.out.show(table.render(ctx.out).trim_end())?)
}

/// `hodoo version`: the server's Odoo version, without needing a key.
///
/// # Errors
///
/// [`Failure::Usage`] when there is no server to ask; any transport error otherwise.
pub async fn version(ctx: &Ctx, url: &str) -> Result<(), Failure> {
    let version = ctx.client.version().await?;
    if ctx.out.mode() == Mode::Json {
        return Ok(ctx
            .out
            .print_json(&json!({ "version": version, "server": url }))?);
    }
    Ok(ctx.out.show(format!("Odoo {version}  ({url})"))?)
}
