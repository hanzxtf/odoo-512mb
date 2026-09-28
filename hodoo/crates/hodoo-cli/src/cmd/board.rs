//! `hodoo board [project]`: the kanban a founder actually looks at.

use std::collections::HashMap;

use hodoo::{ProjectFilter, Task, TaskFilter, UserId};
use serde_json::Value;

use crate::Failure;
use crate::cli::BoardArgs;
use crate::cmd::{self, Ctx};
use crate::output::{Mode, human_due, plain_text};
use crate::refs::{self, Ref};

/// Every open task of a project, grouped by stage.
///
/// # Errors
///
/// Any error of the reads behind it, plus [`Failure::Usage`] for a bad reference.
pub async fn run(ctx: &Ctx, args: &BoardArgs) -> Result<(), Failure> {
    let mine: Option<UserId> = if args.mine {
        Some(cmd::me(ctx).await?)
    } else {
        None
    };

    let projects = match &args.project {
        Some(text) => {
            let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
            vec![ctx.client.projects().get(id).await?]
        }
        None => {
            ctx.client
                .projects()
                .search(ProjectFilter {
                    order: Some("name".to_owned()),
                    ..ProjectFilter::default()
                })
                .await?
        }
    };

    if ctx.out.mode() == Mode::Json {
        let mut boards = Vec::new();
        for project in &projects {
            let tasks = tasks_of(ctx, project.id, args.all, mine).await?;
            boards.push(serde_json::json!({
                "project": project.id.get(),
                "name": project.name,
                "tasks": serde_json::to_value(&tasks).unwrap_or(Value::Null),
            }));
        }
        return Ok(ctx.out.print_json(&Value::Array(boards))?);
    }

    if projects.is_empty() {
        return Ok(ctx
            .out
            .note("no projects yet. `hodoo project create --name \"…\"` starts one")?);
    }

    let now = chrono::Utc::now();
    let mut printed = false;
    for project in &projects {
        let tasks = tasks_of(ctx, project.id, args.all, mine).await?;
        if tasks.is_empty() && !args.all {
            continue;
        }
        printed = true;
        let open = tasks.iter().filter(|task| !task.is_closed).count();
        ctx.out.show(format!(
            "{}  {} open of {}",
            ctx.out
                .bold(&format!("{} (#{})", project.name, project.id.get())),
            open,
            project.task_count
        ))?;
        stage_lines(ctx, project.id, &tasks, now).await?;
    }

    if !printed {
        ctx.out
            .hint("nothing open. `--all` includes finished work, `--mine` only yours")?;
    }
    Ok(())
}

/// Prints one line per stage: how much is in it, and what.
async fn stage_lines(
    ctx: &Ctx,
    project: hodoo::ProjectId,
    tasks: &[Task],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), Failure> {
    let stages = ctx
        .client
        .stages()
        .list(hodoo::StageFilter {
            project: Some(project),
            limit: None,
            ..hodoo::StageFilter::default()
        })
        .await?;
    let mut by_stage: HashMap<i64, Vec<&Task>> = HashMap::new();
    for task in tasks {
        if let Some(stage) = task.stage {
            by_stage.entry(stage.get()).or_default().push(task);
        }
    }
    // Tasks in a stage the project does not list (possible over the API) still belong
    // somewhere, so they are shown under a synthetic last line.
    let mut known: Vec<i64> = stages.iter().map(|stage| stage.id.get()).collect();
    let unknown: Vec<i64> = by_stage
        .keys()
        .filter(|id| !known.contains(id))
        .copied()
        .collect();
    known.extend(unknown.iter().copied());

    for stage_id in known {
        let name = match stages
            .iter()
            .find(|candidate| candidate.id.get() == stage_id)
        {
            Some(found) => found.name.clone(),
            None => cmd::name_of(ctx, "project.task.type", stage_id)
                .await?
                .unwrap_or_else(|| format!("#{stage_id}")),
        };
        let emptiness = by_stage.get(&stage_id).map_or(0, Vec::len);
        // Wide enough for real stage names ("In Progress (scenario)"), narrow enough
        // that the tasks still start on the same column.
        let mut line = format!("  {:<24} {:>3}  ", clip_head(&name, 24), emptiness);
        if let Some(in_stage) = by_stage.get(&stage_id) {
            let said: Vec<String> = in_stage.iter().map(|task| describe(task, now)).collect();
            line.push_str(&said.join(" · "));
        }
        ctx.out.show(line.trim_end())?;
    }
    Ok(())
}

/// One task, as a board entry: id, name, priority and deadline, finished work dimmed.
fn describe(task: &Task, now: chrono::DateTime<chrono::Utc>) -> String {
    let due = human_due(task.deadline, now)
        .map(|(text, _)| text)
        .unwrap_or_else(|| "no date".to_owned());
    let marker = if task.is_closed { "(done) " } else { "" };
    format!(
        "#{} {}{} ({}, {})",
        task.id.get(),
        marker,
        clip_head(&plain_text(&task.name), 40),
        crate::cmd::task::priority_word(task.priority),
        due
    )
}

fn clip_head(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

/// The tasks of a project, optionally only mine, optionally including closed ones.
async fn tasks_of(
    ctx: &Ctx,
    project: hodoo::ProjectId,
    include_closed: bool,
    mine: Option<UserId>,
) -> Result<Vec<Task>, Failure> {
    Ok(ctx
        .client
        .tasks()
        .search(TaskFilter {
            project: Some(project),
            assignee: mine,
            open_only: !include_closed,
            order: Some("date_deadline, priority desc, id".to_owned()),
            ..TaskFilter::default()
        })
        .await?)
}
