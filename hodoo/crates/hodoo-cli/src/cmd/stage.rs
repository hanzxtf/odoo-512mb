//! `hodoo stage …`

use serde_json::{Value, json};

use crate::Failure;
use crate::cli::StageCmd;
use crate::cmd::{Ctx, limit_of};
use crate::output::Mode;
use crate::output::{Column, Table};
use crate::prompt;
use crate::refs::{self, Ref};

/// Dispatches the stage subcommands.
///
/// # Errors
///
/// Any error of the calls behind them, plus [`Failure::Usage`] for a bad reference.
pub async fn run(ctx: &Ctx, command: &StageCmd) -> Result<(), Failure> {
    match command {
        StageCmd::Ls { project, limit } => {
            let project = match project {
                Some(text) => Some(refs::project(&ctx.client, &Ref::parse(text)).await?),
                None => None,
            };
            let stages = ctx
                .client
                .stages()
                .list(hodoo::StageFilter {
                    project,
                    limit: limit_of(*limit),
                    ..hodoo::StageFilter::default()
                })
                .await?;

            if ctx.out.mode() == Mode::Json {
                return Ok(ctx.out.print_json(&as_json(&stages))?);
            }
            if stages.is_empty() {
                return Ok(ctx.out.note(
                    "no stages. `hodoo stage create --name Backlog --project <project>` adds one",
                )?);
            }

            let mut table = Table::new(vec![
                Column::number("ID"),
                Column::text("STAGE").flexible(),
                Column::number("SEQ"),
                Column::text("FOLDED"),
                Column::text("ACTIVE"),
            ]);
            for stage in &stages {
                table.push([
                    stage.id.to_string(),
                    stage.name.clone(),
                    stage.sequence.to_string(),
                    if stage.fold { "yes" } else { "no" }.to_owned(),
                    if stage.active { "yes" } else { "no" }.to_owned(),
                ]);
            }
            Ok(ctx.out.print_table(&table)?)
        }
        StageCmd::Create {
            name,
            project,
            sequence,
            fold,
        } => {
            let fields = hodoo::StageFields {
                name: Some(name.clone()),
                sequence: *sequence,
                fold: fold.then_some(true),
                ..hodoo::StageFields::default()
            };
            let project = match project {
                Some(text) => Some(refs::project(&ctx.client, &Ref::parse(text)).await?),
                None => None,
            };

            if ctx.dry_run {
                let mut preview = serde_json::Map::new();
                preview.insert("name".into(), json!(name));
                if let Some(id) = project {
                    preview.insert("project".into(), json!(id.get()));
                }
                if let Some(sequence) = sequence {
                    preview.insert("sequence".into(), json!(sequence));
                }
                if *fold {
                    preview.insert("fold".into(), json!(true));
                }
                return Ok(prompt::preview(
                    &ctx.out,
                    "create a stage",
                    &Value::Object(preview),
                )?);
            }

            let id = match project {
                Some(project) => ctx.client.stages().create_in(project, fields).await?,
                None => ctx.client.stages().create(fields).await?,
            };
            ctx.out.created(id.get())?;
            ctx.out
                .note(&format!("created  stage #{}  {name}", id.get()))?;
            if project.is_none() {
                ctx.out.hint(
                    "no project was given, so no task can use this stage yet: \
                     hodoo project attach <project> --stage <stage>",
                )?;
            }
            Ok(())
        }
    }
}

/// Serializes a value, or null: a display detail must never fail a command.
fn as_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}
