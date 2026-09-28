//! `hodoo task …`

use hodoo::{Id, Priority, Task, TaskFields, TaskFilter, TaskId, TaskState};
use serde_json::{Map, Value, json};

use crate::Failure;
use crate::cli::{PriorityArg, StateArg, TaskCreateArgs, TaskFieldArgs, TaskLsArgs};
use crate::cmd::{self, Ctx, chatter_block, clip, limit_of, name_of, names, offset_of};
use crate::output::{Cell, Column, Mode, Style, Table, human_due, plain_text};
use crate::prompt::{self, Ask};
use crate::refs::{self, Ref};

/// `hodoo task ls`
pub async fn ls(ctx: &Ctx, args: &TaskLsArgs) -> Result<(), Failure> {
    let project = match &args.project {
        Some(text) => Some(refs::project(&ctx.client, &Ref::parse(text)).await?),
        None => None,
    };
    let mut assignee = match &args.assignee {
        Some(text) => Some(refs::user(&ctx.client, &Ref::parse(text)).await?),
        None => None,
    };
    if args.mine {
        assignee = Some(cmd::me(ctx).await?);
    }
    let stage = match &args.stage {
        Some(text) => Some(refs::stage(&ctx.client, &Ref::parse(text), project).await?),
        None => None,
    };
    // `--overdue` is the deadline in the past, which is what "overdue" means to a
    // person; `--due-before` is the same thing for any other moment.
    let deadline_before = if args.overdue {
        Some(chrono::Utc::now())
    } else {
        match &args.due_before {
            Some(text) => Some(refs::when(text)?),
            None => None,
        }
    };

    let found = ctx
        .client
        .tasks()
        .search(TaskFilter {
            project,
            stage,
            assignee,
            open_only: args.open,
            state: args.state.map(state_of),
            name_contains: args.name.clone(),
            tags: tags_of(ctx, &args.tags).await?,
            deadline_before,
            parent: match &args.parent {
                Some(text) => Some(refs::task(&ctx.client, &Ref::parse(text)).await?),
                None => None,
            },
            order: args
                .order
                .clone()
                .or_else(|| Some("date_deadline, id".to_owned())),
            limit: limit_of(args.limit),
            offset: offset_of(args.offset),
        })
        .await?;

    if ctx.out.mode() == Mode::Json {
        return Ok(ctx.out.print_json(&as_json(&found))?);
    }
    if found.is_empty() {
        return Ok(ctx.out.note(
            "no tasks match. `hodoo task create --name \"…\" --project <project>` adds one",
        )?);
    }
    // Across projects, a task's project matters as much as its stage; inside one,
    // the column would say the same thing on every row.
    task_table(ctx, &found, args.project.is_none()).await?;
    Ok(())
}

/// Renders tasks as the table every task view shares.
async fn task_table(ctx: &Ctx, tasks: &[Task], with_project: bool) -> Result<(), Failure> {
    let stages = names(
        ctx,
        "project.task.type",
        tasks
            .iter()
            .filter_map(|task| task.stage)
            .map(|id| id.get()),
    )
    .await?;
    let people = names(
        ctx,
        "res.users",
        tasks
            .iter()
            .flat_map(|task| task.assignees.iter().map(|id| id.get())),
    )
    .await?;
    let projects = if with_project {
        names(
            ctx,
            "project.project",
            tasks
                .iter()
                .filter_map(|task| task.project)
                .map(|id| id.get()),
        )
        .await?
    } else {
        std::collections::HashMap::new()
    };
    let now = chrono::Utc::now();

    let mut columns = vec![Column::number("ID")];
    if with_project {
        columns.push(Column::text("PROJECT").flexible());
    }
    columns.extend([
        Column::text("PRI"),
        Column::text("STAGE").flexible(),
        Column::text("NAME").flexible(),
        Column::text("WHO"),
        Column::text("DUE"),
    ]);
    let mut table = Table::new(columns);
    for task in tasks {
        let (due, due_style) = match human_due(task.deadline, now) {
            Some((text, style)) if !task.is_closed => (text, Some(style)),
            // A finished task's deadline no longer matters.
            Some((text, _)) => (text, Some(Style::Dim)),
            None => ("-".to_owned(), None),
        };
        let who = if task.assignees.is_empty() {
            "-".to_owned()
        } else {
            task.assignees
                .iter()
                .map(|id| {
                    people
                        .get(&id.get())
                        .cloned()
                        .unwrap_or_else(|| format!("#{}", id.get()))
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut row = vec![Cell::text(task.id.to_string())];
        if with_project {
            row.push(Cell::text(
                task.project
                    .and_then(|id| projects.get(&id.get()).cloned())
                    .map(|name| clip(&name, 34))
                    .unwrap_or_else(|| "-".to_owned()),
            ));
        }
        row.extend([
            Cell::maybe(priority_word(task.priority), priority_style(task)),
            Cell::text(
                task.stage
                    .and_then(|id| stages.get(&id.get()).cloned())
                    .unwrap_or_else(|| "-".to_owned()),
            ),
            Cell::text(clip(&task.name, 46)),
            Cell::text(who),
            Cell::maybe(due, due_style),
        ]);
        table.push_cells(row, task.is_closed.then_some(Style::Dim));
    }
    Ok(ctx.out.print_table(&table)?)
}

/// `hodoo task show <task>`
pub async fn show(ctx: &Ctx, text: &str) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let task = ctx.client.tasks().get(id).await?;
    if ctx.out.mode() == Mode::Json {
        return Ok(ctx.out.print_json(&as_json(&task))?);
    }

    let project = match task.project {
        Some(id) => name_of(ctx, "project.project", id.get())
            .await?
            .unwrap_or_else(|| format!("#{}", id.get())),
        None => "private (no project)".to_owned(),
    };
    let stage = match task.stage {
        Some(id) => name_of(ctx, "project.task.type", id.get())
            .await?
            .unwrap_or_else(|| format!("#{}", id.get())),
        None => "no stage".to_owned(),
    };
    let who = if task.assignees.is_empty() {
        "unassigned".to_owned()
    } else {
        let people = names(ctx, "res.users", task.assignees.iter().map(|id| id.get())).await?;
        task.assignees
            .iter()
            .map(|id| {
                people
                    .get(&id.get())
                    .cloned()
                    .unwrap_or_else(|| format!("#{}", id.get()))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let due = match human_due(task.deadline, chrono::Utc::now()) {
        Some((text, _)) => text,
        None => "no deadline".to_owned(),
    };
    let tags = if task.tags.is_empty() {
        "none".to_owned()
    } else {
        let tagged = names(ctx, "project.tags", task.tags.iter().map(|id| id.get())).await?;
        task.tags
            .iter()
            .map(|id| {
                tagged
                    .get(&id.get())
                    .cloned()
                    .unwrap_or_else(|| format!("#{}", id.get()))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut block = Table::new(vec![
        Column::text("FIELD"),
        Column::text("VALUE").flexible(),
    ]);
    block.push(["project".to_owned(), project]);
    block.push([
        "stage".to_owned(),
        format!(
            "{stage} · {} · {} priority",
            state_word(task.state),
            priority_word(task.priority)
        ),
    ]);
    block.push(["who".to_owned(), who]);
    block.push(["due".to_owned(), due]);
    block.push(["tags".to_owned(), tags]);
    if let Some(hours) = task.allocated_hours {
        block.push(["planned".to_owned(), format!("{hours} hours")]);
    }
    if let Some(description) = &task.description {
        block.push(["description".to_owned(), plain_text(description)]);
    }
    for (label, related) in related_tasks(ctx, id).await? {
        block.push([label, related]);
    }
    ctx.out.show(format!(
        "{}\n{}",
        ctx.out.bold(&format!("{} (#{})", task.name, id.get())),
        block.render(ctx.out).trim_end()
    ))?;

    let messages = cmd::chatter(ctx, "project.task", id.get(), 3).await?;
    Ok(chatter_block(ctx, &messages)?)
}

/// `blocked by` and `blocks`, as two rows for a detail view.
async fn related_tasks(ctx: &Ctx, id: TaskId) -> Result<Vec<(String, String)>, Failure> {
    let waits_on = ctx.client.tasks().dependencies(id).await?;
    let blocks = blocking(ctx, id).await?;

    let mut out = Vec::new();
    if !waits_on.is_empty() {
        out.push((
            "blocked by".to_owned(),
            describe(ctx, waits_on.iter().map(|id| id.get()).collect()).await?,
        ));
    }
    if !blocks.is_empty() {
        out.push(("blocks".to_owned(), describe(ctx, blocks).await?));
    }
    Ok(out)
}

/// The tasks that wait on this one.
///
/// `dependent_ids` is not in the fields the client reads for every task - it is the
/// other direction of the same relation - so it is asked for on its own.
async fn blocking(ctx: &Ctx, id: TaskId) -> Result<Vec<i64>, Failure> {
    let rows = ctx
        .client
        .call(
            "project.task",
            "read",
            json!({ "ids": [id.get()], "fields": ["dependent_ids"] }),
        )
        .await?;
    Ok(rows
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("dependent_ids"))
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default())
}

/// `#31 name` for a list of task ids.
async fn describe(ctx: &Ctx, ids: Vec<i64>) -> Result<String, Failure> {
    let mut said = Vec::new();
    for id in ids {
        let name = name_of(ctx, "project.task", id)
            .await?
            .unwrap_or_else(|| "?".to_owned());
        said.push(format!("#{id} {name}"));
    }
    Ok(said.join(", "))
}

/// `hodoo task create`
pub async fn create(ctx: &Ctx, args: &TaskCreateArgs) -> Result<(), Failure> {
    let mut fields = TaskFields::new(args.name.clone());
    let mut preview = Map::new();
    preview.insert("name".into(), json!(args.name));
    apply(ctx, &args.fields, &mut fields, &mut preview, None).await?;

    if ctx.dry_run {
        return Ok(prompt::preview(&ctx.out, "create a task", &preview.into())?);
    }
    let id = ctx.client.tasks().create(fields).await?;
    ctx.out.created(id.get())?;
    ctx.out.note(&format!(
        "created  task #{}  {}",
        id.get(),
        clip(&args.name, 40)
    ))?;
    ctx.out.hint(&format!("hodoo task show {}", id.get()))?;
    Ok(())
}

/// `hodoo task update <task>`
pub async fn update(ctx: &Ctx, text: &str, args: &TaskFieldArgs) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let existing = ctx.client.tasks().get(id).await?;
    let mut fields = TaskFields::default();
    let mut preview = Map::new();
    // The project is needed to interpret a stage name, and is never changed by accident.
    apply(ctx, args, &mut fields, &mut preview, existing.project).await?;

    if preview.is_empty() {
        return Err(Failure::Usage(
            "nothing to change: pass a field, e.g. --priority urgent --due +3d".to_owned(),
        ));
    }
    if ctx.dry_run {
        return Ok(prompt::preview(
            &ctx.out,
            "update the task",
            &preview.into(),
        )?);
    }
    ctx.client.tasks().update(id, fields).await?;
    ctx.out.changed(id.get())?;
    ctx.out.note(&format!("updated  task #{}", id.get()))?;
    if let Some(state) = args.state {
        warn_if_recomputed(ctx, id, state_of(state)).await?;
    }
    Ok(())
}

/// Odoo computes a task's state from its open dependencies, so a state write can lose
/// to that compute: the task reads back as `waiting` on the next read whatever was
/// written. That is surprising enough to be worth a sentence where it happens, rather
/// than a silent no-op the caller finds out about later.
async fn warn_if_recomputed(ctx: &Ctx, id: TaskId, wanted: TaskState) -> Result<(), Failure> {
    let after = ctx.client.tasks().get(id).await?;
    if after.state == wanted {
        return Ok(());
    }
    ctx.out.hint(&format!(
        "the task reads {} rather than {}: Odoo computes the state from its open \
         dependencies, so a task that waits on something is waiting (hodoo task deps {})",
        state_word(after.state),
        state_word(wanted),
        id.get()
    ))?;
    Ok(())
}

/// `hodoo task done|cancel|reopen <task>`
pub async fn set_state(ctx: &Ctx, text: &str, state: TaskState) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let word = state_word(state);
    if ctx.dry_run {
        return Ok(ctx
            .out
            .note(&format!("would mark task #{} {word}", id.get()))?);
    }
    ctx.client.tasks().set_state(id, state).await?;
    ctx.out.changed(id.get())?;
    ctx.out.note(&format!("{word}  task #{}", id.get()))?;
    warn_if_recomputed(ctx, id, state).await?;
    Ok(())
}

/// `hodoo task move <task> --stage <stage>`
pub async fn move_to(ctx: &Ctx, text: &str, stage: &str) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let task = ctx.client.tasks().get(id).await?;
    let project = task.project.ok_or_else(|| {
        Failure::Usage(format!(
            "task #{} belongs to no project, so it has no stages to move between",
            id.get()
        ))
    })?;
    let stage = refs::stage(&ctx.client, &Ref::parse(stage), Some(project)).await?;

    if ctx.dry_run {
        return Ok(ctx.out.note(&format!(
            "would move task #{} to stage #{}",
            id.get(),
            stage.get()
        ))?);
    }
    ctx.client
        .tasks()
        .update(
            id,
            TaskFields {
                stage: Some(stage),
                ..TaskFields::default()
            },
        )
        .await?;
    let name = name_of(ctx, "project.task.type", stage.get())
        .await?
        .unwrap_or_else(|| format!("#{}", stage.get()));
    ctx.out.changed(id.get())?;
    ctx.out
        .note(&format!("moved  task #{} to {name}", id.get()))?;
    Ok(())
}

/// `hodoo task comment <task> --body <text>`
pub async fn comment(ctx: &Ctx, text: &str, body: &str, internal: bool) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let kind = if internal { "note" } else { "comment" };
    if ctx.dry_run {
        return Ok(ctx
            .out
            .note(&format!("would post a {kind} on task #{}", id.get()))?);
    }
    ctx.client.tasks().comment(id, body, internal).await?;
    ctx.out.changed(id.get())?;
    ctx.out
        .note(&format!("posted  {kind} on task #{}", id.get()))?;
    Ok(())
}

/// `hodoo task messages <task>`
pub async fn messages(ctx: &Ctx, text: &str, limit: u32) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let rows = ctx
        .client
        .call(
            "mail.message",
            "search_read",
            json!({
                "domain": [["model", "=", "project.task"], ["res_id", "=", id.get()]],
                "fields": ["body", "author_id", "date", "message_type"],
                "order": "id asc",
                "limit": limit,
            }),
        )
        .await?;
    if ctx.out.mode() == Mode::Json {
        return Ok(ctx.out.print_json(&rows)?);
    }
    let flattened = cmd::chatter(ctx, "project.task", id.get(), limit).await?;
    if flattened.is_empty() {
        return Ok(ctx
            .out
            .note(&format!("nothing on task #{}'s chatter yet", id.get()))?);
    }
    Ok(chatter_block(ctx, &flattened)?)
}

/// `hodoo task deps <task>`
pub async fn deps(ctx: &Ctx, text: &str) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let mut rows = related_tasks(ctx, id).await?;
    if ctx.out.mode() == Mode::Json {
        let blocked_by = ctx.client.tasks().dependencies(id).await?;
        let blocks = blocking(ctx, id).await?;
        return Ok(ctx.out.print_json(&json!({
            "task": id.get(),
            "blocked_by": blocked_by.iter().map(|id| id.get()).collect::<Vec<_>>(),
            "blocks": blocks,
        }))?);
    }
    if rows.is_empty() {
        return Ok(ctx.out.note(&format!(
            "task #{} neither waits on nor blocks anything",
            id.get()
        ))?);
    }
    let mut table = Table::new(vec![
        Column::text("RELATION"),
        Column::text("TASK").flexible(),
    ]);
    for (label, related) in rows.drain(..) {
        table.push([label, related]);
    }
    Ok(ctx.out.print_table(&table)?)
}

/// `hodoo task rm <task>`
pub async fn rm(ctx: &Ctx, text: &str, force: bool) -> Result<(), Failure> {
    let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
    let task = ctx.client.tasks().get(id).await?;
    // Deleting a task takes its subtasks with it (verified against 19.0), and a
    // confirmation that hides that is a confirmation that misleads.
    let subtasks = cmd::count_of(ctx, "project.task", json!([["parent_id", "=", id.get()]])).await;
    let action = match subtasks {
        Some(n) if n > 0 => format!(
            "delete task #{} \"{}\" and its {} subtasks",
            id.get(),
            clip(&task.name, 40),
            n
        ),
        _ => format!("delete task #{} \"{}\"", id.get(), clip(&task.name, 40)),
    };

    if ctx.dry_run {
        return Ok(ctx.out.note(&format!("would {action}"))?);
    }
    Ask::from_flags(force, ctx.no_input).destroy(&action)?;
    ctx.client.tasks().delete(id).await?;
    ctx.out.removed(id.get())?;
    ctx.out.note(&format!(
        "deleted  task #{}  {}",
        id.get(),
        clip(&task.name, 40)
    ))?;
    Ok(())
}

/// Turns the shared task flags into fields and a human preview.
async fn apply(
    ctx: &Ctx,
    args: &TaskFieldArgs,
    fields: &mut TaskFields,
    preview: &mut Map<String, Value>,
    fallback_project: Option<hodoo::ProjectId>,
) -> Result<(), Failure> {
    let mut project = fallback_project;
    if let Some(text) = &args.project {
        let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
        fields.project = Some(id);
        project = Some(id);
        preview.insert("project".into(), json!(format!("{text} (#{})", id.get())));
    }
    if let Some(text) = &args.stage {
        let id = refs::stage(&ctx.client, &Ref::parse(text), project).await?;
        fields.stage = Some(id);
        preview.insert("stage".into(), json!(format!("{text} (#{})", id.get())));
    }
    if let Some(state) = args.state {
        fields.state = Some(state_of(state));
        preview.insert("state".into(), json!(state_word(state_of(state))));
    }
    if let Some(priority) = args.priority {
        fields.priority = Some(priority_of(priority));
        preview.insert(
            "priority".into(),
            json!(priority_word(priority_of(priority))),
        );
    }
    if args.unassign {
        fields.assignees = Some(Vec::new());
        preview.insert("assignees".into(), json!("none"));
    } else if !args.assignees.is_empty() {
        let mut ids = Vec::new();
        let mut said = Vec::new();
        for text in &args.assignees {
            let id = if text == "me" {
                cmd::me(ctx).await?
            } else {
                refs::user(&ctx.client, &Ref::parse(text)).await?
            };
            ids.push(id);
            said.push(format!("{text} (#{})", id.get()));
        }
        fields.assignees = Some(ids);
        preview.insert("assignees".into(), json!(said.join(", ")));
    }
    if let Some(text) = &args.customer {
        let id = refs::partner(&ctx.client, &Ref::parse(text)).await?;
        fields.customer = Some(id);
        preview.insert("customer".into(), json!(format!("{text} (#{})", id.get())));
    }
    if let Some(text) = &args.due {
        let when = refs::when(text)?;
        fields.deadline = Some(when);
        preview.insert("due".into(), json!(format!("{text} ({when})")));
    }
    if let Some(hours) = args.hours {
        fields.allocated_hours = Some(hours);
        preview.insert("hours".into(), json!(hours));
    }
    if args.clear_tags {
        fields.tags = Some(Vec::new());
        preview.insert("tags".into(), json!("none"));
    } else if !args.tags.is_empty() {
        fields.tags = Some(tags_of(ctx, &args.tags).await?);
        preview.insert("tags".into(), json!(args.tags.join(", ")));
    }
    if let Some(text) = &args.parent {
        let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
        fields.parent = Some(id);
        preview.insert("parent".into(), json!(format!("{text} (#{})", id.get())));
    }
    if !args.depends_on.is_empty() {
        let mut ids = Vec::new();
        let mut said = Vec::new();
        for text in &args.depends_on {
            let id = refs::task(&ctx.client, &Ref::parse(text)).await?;
            ids.push(id);
            said.push(format!("{text} (#{})", id.get()));
        }
        fields.depends_on = Some(ids);
        preview.insert("waits on".into(), json!(said.join(", ")));
    }
    if let Some(text) = &args.milestone {
        let project = project.ok_or_else(|| {
            Failure::Usage("--milestone needs to know the project: pass --project too".to_owned())
        })?;
        let id = refs::milestone(&ctx.client, &Ref::parse(text), project).await?;
        fields.milestone = Some(id);
        preview.insert("milestone".into(), json!(format!("{text} (#{})", id.get())));
    }
    if let Some(description) = &args.description {
        fields.description = Some(description.clone());
        preview.insert("description".into(), json!(plain_text(description)));
    }
    Ok(())
}

/// Resolves tag names, creating the ones that do not exist yet.
async fn tags_of(ctx: &Ctx, tags: &[String]) -> Result<Vec<Id<hodoo::Tag>>, Failure> {
    let mut ids = Vec::new();
    for tag in tags {
        ids.push(refs::tag(&ctx.client, tag).await?);
    }
    Ok(ids)
}

/// Odoo's state value for a flag.
#[must_use]
pub fn state_of(state: StateArg) -> TaskState {
    match state {
        StateArg::InProgress => TaskState::InProgress,
        StateArg::ChangesRequested => TaskState::ChangesRequested,
        StateArg::Approved => TaskState::Approved,
        StateArg::Done => TaskState::Done,
        StateArg::Canceled => TaskState::Canceled,
        StateArg::Waiting => TaskState::Waiting,
    }
}

/// A short word for a state, for tables and confirmations.
#[must_use]
pub fn state_word(state: TaskState) -> &'static str {
    match state {
        TaskState::InProgress => "in progress",
        TaskState::ChangesRequested => "changes requested",
        TaskState::Approved => "approved",
        TaskState::Done => "done",
        TaskState::Canceled => "canceled",
        TaskState::Waiting => "waiting",
    }
}

/// Odoo's priority value for a flag.
#[must_use]
pub fn priority_of(priority: PriorityArg) -> Priority {
    match priority {
        PriorityArg::Low => Priority::Low,
        PriorityArg::Medium => Priority::Medium,
        PriorityArg::High => Priority::High,
        PriorityArg::Urgent => Priority::Urgent,
    }
}

/// A short word for a priority.
#[must_use]
pub fn priority_word(priority: Priority) -> &'static str {
    match priority {
        Priority::Low => "low",
        Priority::Medium => "medium",
        Priority::High => "high",
        Priority::Urgent => "urgent",
    }
}

fn priority_style(task: &Task) -> Option<Style> {
    if task.is_closed {
        return None;
    }
    match task.priority {
        Priority::Urgent => Some(Style::Red),
        Priority::High => Some(Style::Yellow),
        _ => None,
    }
}

/// Serializes a value, or null: a display detail must never fail a command.
fn as_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}
