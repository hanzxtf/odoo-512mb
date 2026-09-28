//! `hodoo milestone …`

use serde_json::Value;

use crate::Failure;
use crate::cli::MilestoneCmd;
use crate::cmd::Ctx;
use crate::output::{Column, Mode, Style, Table, show_date};
use crate::prompt::{self, Ask};
use crate::refs::{self, Ref};

/// Dispatches the milestone subcommands.
///
/// # Errors
///
/// Any error of the calls behind them, plus [`Failure::Usage`] for a bad reference.
pub async fn run(ctx: &Ctx, command: &MilestoneCmd) -> Result<(), Failure> {
    match command {
        MilestoneCmd::Ls { project } => {
            let project = refs::project(&ctx.client, &Ref::parse(project)).await?;
            let milestones = ctx.client.milestones().list(project).await?;
            if ctx.out.mode() == Mode::Json {
                return Ok(ctx.out.print_json(&as_json(&milestones))?);
            }
            if milestones.is_empty() {
                return Ok(ctx.out.note(
                    "no milestones. The project has to allow them: \
                     hodoo project update <project> --milestones",
                )?);
            }
            let mut table = Table::new(vec![
                Column::number("ID"),
                Column::text("MILESTONE").flexible(),
                Column::text("DUE"),
                Column::text("REACHED"),
                Column::number("TASKS"),
            ]);
            for milestone in &milestones {
                table.push_styled(
                    [
                        milestone.id.to_string(),
                        milestone.name.clone(),
                        show_date(milestone.deadline),
                        if milestone.is_reached {
                            match milestone.reached_date {
                                Some(date) => format!("yes ({date})"),
                                None => "yes".to_owned(),
                            }
                        } else {
                            "not yet".to_owned()
                        },
                        milestone.task_count.to_string(),
                    ],
                    milestone.is_reached.then_some(Style::Green),
                );
            }
            Ok(ctx.out.print_table(&table)?)
        }
        MilestoneCmd::Create { project, name, due } => {
            let project = refs::project(&ctx.client, &Ref::parse(project)).await?;
            let due = match due {
                Some(text) => Some(refs::date(text)?),
                None => None,
            };
            let fields = hodoo::MilestoneFields {
                name: Some(name.clone()),
                project: Some(project),
                deadline: due,
                ..hodoo::MilestoneFields::default()
            };

            if ctx.dry_run {
                let mut preview = serde_json::Map::new();
                preview.insert("name".into(), serde_json::json!(name));
                preview.insert("project".into(), serde_json::json!(project.get()));
                if let Some(date) = due {
                    preview.insert("due".into(), serde_json::json!(date.to_string()));
                }
                return Ok(prompt::preview(
                    &ctx.out,
                    "create a milestone",
                    &Value::Object(preview),
                )?);
            }
            let id = ctx.client.milestones().create(fields).await?;
            ctx.out.created(id.get())?;
            ctx.out
                .note(&format!("created  milestone #{}  {name}", id.get()))?;
            Ok(())
        }
        MilestoneCmd::Reached { milestone, undo } => {
            if ctx.dry_run {
                return Ok(ctx.out.note(&format!(
                    "would mark milestone #{milestone} {}",
                    if *undo { "unreached" } else { "reached" }
                ))?);
            }
            ctx.client
                .milestones()
                .set_reached(hodoo::MilestoneId::new(*milestone), !undo)
                .await?;
            ctx.out.changed(*milestone)?;
            ctx.out.note(&format!(
                "marked  milestone #{milestone} {}",
                if *undo { "unreached" } else { "reached" }
            ))?;
            Ok(())
        }
        MilestoneCmd::Rm { milestone, force } => {
            if ctx.dry_run {
                return Ok(ctx
                    .out
                    .note(&format!("would delete milestone #{milestone}"))?);
            }
            Ask::from_flags(*force, ctx.no_input)
                .destroy(&format!("delete milestone #{milestone}"))?;
            ctx.client
                .milestones()
                .delete(hodoo::MilestoneId::new(*milestone))
                .await?;
            ctx.out.note(&format!("deleted  milestone #{milestone}"))?;
            Ok(())
        }
    }
}

/// Serializes a value, or null: a display detail must never fail a command.
fn as_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}
