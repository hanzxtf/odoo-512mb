//! The stage commands: a project's (`project.project.stage`) and a task's
//! (`project.task.type`). Both live under `hodoo project`, because a stage is a
//! project's business either way.

use serde_json::{Map, Value, json};

use crate::Failure;
use crate::cli::{ProjectStagesCmd, TaskStagesCmd};
use crate::cmd::{self, Ctx};
use crate::output::Mode;
use crate::output::{Column, Table};
use crate::prompt;
use crate::refs::{self, Ref};

/// Dispatches `hodoo project task-stages …`.
///
/// # Errors
///
/// Any error of the calls behind them, plus [`Failure::Usage`] for a bad reference.
pub async fn run_task_stages(ctx: &Ctx, command: &TaskStagesCmd) -> Result<(), Failure> {
    match command {
        // The board itself lives with the rest of a project's detail: it counts the
        // project's tasks, which is a project read, not a stage one.
        TaskStagesCmd::Ls { project } => cmd::project::task_stages(ctx, project).await,
        TaskStagesCmd::Create {
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
                let mut preview = Map::new();
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
                    "create a task stage",
                    &Value::Object(preview),
                )?);
            }

            let id = match project {
                Some(project) => ctx.client.stages().create_in(project, fields).await?,
                None => ctx.client.stages().create(fields).await?,
            };
            ctx.out.created(id.get())?;
            ctx.out
                .note(&format!("created  task stage #{}  {name}", id.get()))?;
            if project.is_none() {
                ctx.out.hint(
                    "no project was given, so no task can use this stage yet: \
                     hodoo project attach <project> --task-stage <stage>",
                )?;
            }
            Ok(())
        }
    }
}

/// Dispatches `hodoo project stages …`.
///
/// # Errors
///
/// Any error of the calls behind them, plus [`Failure::Usage`] for a bad reference.
pub async fn run_project_stages(ctx: &Ctx, command: &ProjectStagesCmd) -> Result<(), Failure> {
    match command {
        ProjectStagesCmd::Ls { order, limit } => {
            let stages = ctx
                .client
                .project_stages()
                .list(hodoo::ProjectStageFilter {
                    order: order.clone(),
                    limit: cmd::limit_of(*limit),
                    ..hodoo::ProjectStageFilter::default()
                })
                .await?;

            if ctx.out.mode() == Mode::Json {
                return Ok(ctx.out.print_json(&as_json(&stages))?);
            }
            if stages.is_empty() {
                return Ok(ctx.out.note(
                    "no project stages: `hodoo project stages create --name \"On Hold\"`",
                )?);
            }

            // One read of the projects, counted here: cheaper and more honest than a
            // count call per stage.
            let projects = ctx
                .client
                .projects()
                .search(hodoo::ProjectFilter {
                    limit: None,
                    ..hodoo::ProjectFilter::default()
                })
                .await?;
            let mut counts: std::collections::HashMap<i64, usize> =
                std::collections::HashMap::new();
            for project in &projects {
                if let Some(stage) = project.stage {
                    *counts.entry(stage.get()).or_default() += 1;
                }
            }

            let mut table = Table::new(vec![
                Column::number("ID"),
                Column::text("STAGE").flexible(),
                Column::number("SEQ"),
                Column::text("FOLDED"),
                Column::text("ACTIVE"),
                Column::number("PROJECTS"),
            ]);
            for stage in &stages {
                table.push([
                    stage.id.to_string(),
                    stage.name.clone(),
                    stage.sequence.to_string(),
                    if stage.fold { "yes" } else { "no" }.to_owned(),
                    if stage.active { "yes" } else { "no" }.to_owned(),
                    counts
                        .get(&stage.id.get())
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                ]);
            }
            Ok(ctx.out.show(format!(
                "{}\n{}",
                ctx.out.bold("project stages"),
                table.render(ctx.out).trim_end()
            ))?)
        }

        ProjectStagesCmd::Create {
            name,
            sequence,
            fold,
        } => {
            if ctx.dry_run {
                let mut preview = Map::new();
                preview.insert("name".into(), json!(name));
                if let Some(sequence) = sequence {
                    preview.insert("sequence".into(), json!(sequence));
                }
                if *fold {
                    preview.insert("fold".into(), json!(true));
                }
                return Ok(prompt::preview(
                    &ctx.out,
                    "create a project stage",
                    &Value::Object(preview),
                )?);
            }

            let id = ctx
                .client
                .project_stages()
                .create(hodoo::StageFields {
                    name: Some(name.clone()),
                    sequence: *sequence,
                    fold: fold.then_some(true),
                    ..hodoo::StageFields::default()
                })
                .await?;
            ctx.out.created(id.get())?;
            ctx.out
                .note(&format!("created  project stage #{}  {name}", id.get()))?;
            ctx.out
                .hint("no project is in it yet: hodoo project update <project> --stage <stage>")?;
            Ok(())
        }

        ProjectStagesCmd::Update {
            stage,
            name,
            sequence,
            fold,
            unfold,
        } => {
            let id = refs::project_stage(&ctx.client, &Ref::parse(stage)).await?;
            let mut fields = hodoo::StageFields::default();
            let mut preview = Map::new();
            if let Some(name) = name {
                fields.name = Some(name.clone());
                preview.insert("name".into(), json!(name));
            }
            if let Some(sequence) = sequence {
                fields.sequence = Some(*sequence);
                preview.insert("sequence".into(), json!(sequence));
            }
            if *fold || *unfold {
                fields.fold = Some(*fold);
                preview.insert("fold".into(), json!(*fold));
            }
            if preview.is_empty() {
                return Err(Failure::Usage(
                    "nothing to change: pass a field, e.g. --name \"On Hold\"".to_owned(),
                ));
            }

            let name = cmd::name_of(ctx, "project.project.stage", id.get())
                .await?
                .unwrap_or_else(|| format!("#{}", id.get()));
            if ctx.dry_run {
                return Ok(prompt::preview(
                    &ctx.out,
                    &format!("update project stage {name}"),
                    &Value::Object(preview),
                )?);
            }
            ctx.client.project_stages().update(id, fields).await?;
            ctx.out.changed(id.get())?;
            ctx.out
                .note(&format!("updated  project stage #{}  {name}", id.get()))?;
            Ok(())
        }

        ProjectStagesCmd::Rm { stage, force } => {
            let id = refs::project_stage(&ctx.client, &Ref::parse(stage)).await?;
            let name = cmd::name_of(ctx, "project.project.stage", id.get())
                .await?
                .unwrap_or_else(|| format!("#{}", id.get()));
            // Odoo refuses to delete a stage a project points at, so the CLI refuses
            // first and says what has to move: a raw ValidationError tells nobody
            // which project is in the way.
            let projects =
                cmd::count_of(ctx, "project.project", json!([["stage_id", "=", id.get()]]))
                    .await
                    .unwrap_or_default();
            if projects > 0 {
                let what = if projects == 1 {
                    "1 project is in it".to_owned()
                } else {
                    format!("{projects} projects are in it")
                };
                return Err(Failure::Usage(format!(
                    "refusing to delete project stage #{id} \"{name}\": {what}. Move them \
                     first (hodoo project update <project> --stage <other>), or archive the \
                     stage: hodoo call project.project.stage write --ids {id} \
                     --body '{{\"vals\":{{\"active\":false}}}}'"
                )));
            }

            let action = format!("delete project stage #{id} \"{name}\"");
            if ctx.dry_run {
                return Ok(ctx.out.note(&format!("would {action}"))?);
            }
            prompt::Ask::from_flags(*force, ctx.no_input).destroy(&action)?;
            ctx.client.project_stages().delete(id).await?;
            ctx.out.removed(id.get())?;
            ctx.out
                .note(&format!("deleted  project stage #{}  {name}", id.get()))?;
            Ok(())
        }
    }
}

/// Serializes a value, or null: a display detail must never fail a command.
fn as_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}
