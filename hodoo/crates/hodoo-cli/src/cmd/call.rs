//! `hodoo call <model> <method>`: the escape hatch.

use serde_json::Value;

use crate::Failure;
use crate::cli::CallArgs;
use crate::cmd::Ctx;
use crate::prompt;

/// Calls any model method with the body given.
///
/// # Errors
///
/// [`Failure::Usage`] when the body is not a JSON object, and any error the call
/// itself produces.
pub async fn run(ctx: &Ctx, args: &CallArgs) -> Result<(), Failure> {
    let mut body = match &args.body {
        Some(text) => serde_json::from_str::<Value>(text)
            .map_err(|error| Failure::Usage(format!("--body is not valid JSON: {error}")))?,
        None => Value::Object(serde_json::Map::new()),
    };
    let object = body.as_object_mut().ok_or_else(|| {
        Failure::Usage(
            "--body must be a JSON object of named arguments, e.g. '{\"domain\":[]}'".to_owned(),
        )
    })?;
    if !args.ids.is_empty() {
        object.insert("ids".into(), serde_json::json!(args.ids));
    }

    if ctx.dry_run {
        return Ok(prompt::preview(
            &ctx.out,
            &format!("call {}.{}", args.model, args.method),
            &body,
        )?);
    }

    let result = ctx.client.call(&args.model, &args.method, body).await?;
    // The shape belongs to Odoo here, so the renderer decides: a table for records, a
    // block for one, a bare value otherwise.
    Ok(ctx.out.print_auto(&result)?)
}
