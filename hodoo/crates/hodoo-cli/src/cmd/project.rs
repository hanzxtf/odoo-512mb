//! `hodoo project …`

use hodoo::{
    Id, Project, ProjectFields, ProjectFilter, ProjectId, ProjectStageId, TaskFilter, Visibility,
};
use serde_json::{Map, Value, json};

use crate::Failure;
use crate::cli::{ProjectCreateArgs, ProjectFieldArgs, ProjectLsArgs, VisibilityArg};
use crate::cmd::{self, Ctx, chatter_block, clip, limit_of, name_of, names, offset_of};
use crate::output::{Column, Mode, Style, Table, plain_text, show_date};
use crate::prompt::{self, Ask};
use crate::refs::{self, Ref};

/// `hodoo project ls`
pub async fn ls(ctx: &Ctx, args: &ProjectLsArgs) -> Result<(), Failure> {
    let mut filter = ProjectFilter {
        name_contains: args.name.clone(),
        customer: match &args.customer {
            Some(text) => Some(refs::partner(&ctx.client, &Ref::parse(text)).await?),
            None => None,
        },
        manager: match &args.manager {
            Some(text) => Some(refs::user(&ctx.client, &Ref::parse(text)).await?),
            None => None,
        },
        stage: match &args.stage {
            Some(text) => Some(project_stage(ctx, text).await?),
            None => None,
        },
        tags: tags_of(ctx, &args.tags).await?,
        order: args.order.clone(),
        limit: limit_of(args.limit),
        offset: offset_of(args.offset),
    };
    if args.mine {
        filter.manager = Some(cmd::me(ctx).await?);
    }

    let projects = ctx.client.projects().search(filter).await?;
    if ctx.out.mode() == Mode::Json {
        return Ok(ctx.out.print_json(&as_json(&projects))?);
    }
    if projects.is_empty() {
        return Ok(ctx
            .out
            .note("no projects match. `hodoo project create --name \"…\"` starts one")?);
    }

    let customers = names(
        ctx,
        "res.partner",
        projects
            .iter()
            .filter_map(|project| project.customer)
            .map(|id| id.get()),
    )
    .await?;
    let managers = names(
        ctx,
        "res.users",
        projects
            .iter()
            .filter_map(|project| project.manager)
            .map(|id| id.get()),
    )
    .await?;

    let mut table = Table::new(vec![
        Column::number("ID"),
        Column::text("NAME").flexible(),
        Column::text("CUSTOMER"),
        Column::text("MANAGER"),
        Column::text("VISIBILITY"),
        Column::text("OPEN/TASKS"),
        Column::text("ENDS"),
    ]);
    for project in &projects {
        table.push_styled(
            [
                project.id.to_string(),
                project.name.clone(),
                lookup(&customers, project.customer.map(|id| id.get())),
                lookup(&managers, project.manager.map(|id| id.get())),
                project.visibility.as_str().to_owned(),
                format!("{}/{}", project.open_task_count, project.task_count),
                show_date(project.date),
            ],
            finished(project),
        );
    }
    Ok(ctx.out.print_table(&table)?)
}

/// `hodoo project show <project>`
pub async fn show(ctx: &Ctx, text: &str) -> Result<(), Failure> {
    let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
    let project = ctx.client.projects().get(id).await?;
    if ctx.out.mode() == Mode::Json {
        return Ok(ctx.out.print_json(&as_json(&project))?);
    }

    let customer = match project.customer {
        Some(id) => name_of(ctx, "res.partner", id.get())
            .await?
            .unwrap_or_else(|| format!("#{}", id.get())),
        None => "none".to_owned(),
    };
    let manager = match project.manager {
        Some(id) => name_of(ctx, "res.users", id.get())
            .await?
            .unwrap_or_else(|| format!("#{}", id.get())),
        None => "unassigned".to_owned(),
    };

    let mut block = Table::new(vec![
        Column::text("FIELD"),
        Column::text("VALUE").flexible(),
    ]);
    block.push(["customer".to_owned(), customer]);
    block.push(["manager".to_owned(), manager]);
    block.push([
        "visibility".to_owned(),
        project.visibility.as_str().to_owned(),
    ]);
    block.push([
        "dates".to_owned(),
        format!(
            "{} → {}",
            show_date(project.date_start),
            show_date(project.date)
        ),
    ]);
    block.push([
        "tasks".to_owned(),
        format!("{} open of {}", project.open_task_count, project.task_count),
    ]);
    if let Some(description) = &project.description {
        block.push(["description".to_owned(), plain_text(description)]);
    }
    ctx.out.show(format!(
        "{}\n{}",
        ctx.out.bold(&format!("{} (#{})", project.name, id.get())),
        block.render(ctx.out).trim_end()
    ))?;

    stage_table(ctx, id).await?;
    let messages = cmd::chatter(ctx, "project.project", id.get(), 3).await?;
    Ok(chatter_block(ctx, &messages)?)
}

/// `hodoo project create`
pub async fn create(ctx: &Ctx, args: &ProjectCreateArgs) -> Result<(), Failure> {
    if let Some(template) = &args.template {
        return from_template(ctx, template, args).await;
    }

    let mut fields = ProjectFields::new(args.name.clone());
    let mut preview = Map::new();
    preview.insert("name".into(), json!(args.name));
    apply(ctx, &args.fields, &mut fields, &mut preview).await?;

    if ctx.dry_run {
        return Ok(prompt::preview(
            &ctx.out,
            "create a project",
            &preview.into(),
        )?);
    }
    let id = ctx.client.projects().create(fields).await?;
    ctx.out.created(id.get())?;
    ctx.out
        .note(&format!("created  project #{}  {}", id.get(), args.name))?;
    ctx.out.hint(&format!(
        "a task needs a stage first: hodoo stage create --name Backlog --project {}",
        id.get()
    ))?;
    Ok(())
}

/// `hodoo project create --template <template>`
async fn from_template(ctx: &Ctx, template: &str, args: &ProjectCreateArgs) -> Result<(), Failure> {
    let template_id = template_id(ctx, template).await?;
    let source = ctx.client.projects().get(template_id).await?;

    let mut preview = Map::new();
    preview.insert("name".into(), json!(args.name));
    preview.insert(
        "template".into(),
        json!(format!("{} (#{})", source.name, template_id.get())),
    );
    let mut values = Map::new();
    values.insert("name".into(), json!(args.name));

    if let Some(customer) = &args.fields.customer {
        let id = refs::partner(&ctx.client, &Ref::parse(customer)).await?;
        values.insert("partner_id".into(), json!(id.get()));
        preview.insert(
            "customer".into(),
            json!(format!("{customer} (#{})", id.get())),
        );
    }
    if let Some(visibility) = args.fields.visibility {
        // Copies keep the template's fields, so these are set after creation instead.
        preview.insert("visibility".into(), json!(visibility_word(visibility)));
    }

    // Odoo shifts a copied project to today and keeps the template's duration, but it
    // cannot be given a start date alone: passing one without an end is a server-side
    // error, so the end is worked out here from the template's own dates.
    match (&args.fields.start, &args.fields.end) {
        (Some(start), Some(end)) => {
            values.insert("date_start".into(), json!(start));
            values.insert("date".into(), json!(end));
            preview.insert("starts".into(), json!(start));
            preview.insert("ends".into(), json!(end));
        }
        (Some(start), None) => {
            let start_date = refs::date(start)?;
            let end_date = match (source.date_start, source.date) {
                (Some(from), Some(to)) => start_date + (to - from),
                _ => start_date,
            };
            values.insert("date_start".into(), json!(start_date.to_string()));
            values.insert("date".into(), json!(end_date.to_string()));
            preview.insert("starts".into(), json!(start_date.to_string()));
            preview.insert(
                "ends".into(),
                json!(format!("{end_date} (template duration)")),
            );
        }
        (None, Some(end)) => {
            values.insert("date".into(), json!(end));
            preview.insert("ends".into(), json!(end));
        }
        (None, None) => {}
    }

    if ctx.dry_run {
        return Ok(prompt::preview(
            &ctx.out,
            "create a project from the template",
            &preview.into(),
        )?);
    }

    let created = ctx
        .client
        .call(
            "project.project",
            "action_create_from_template",
            json!({ "ids": [template_id.get()], "values": Value::Object(values) }),
        )
        .await?;
    let new_id = created
        .as_array()
        .and_then(|ids| ids.first())
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            Failure::Odoo(hodoo::Error::UnexpectedResponse {
                status: 200,
                body: created.to_string(),
            })
        })?;

    // Fields a template copy does not carry.
    let mut after = ProjectFields::default();
    let mut extra = Map::new();
    let fields = ProjectFieldArgs {
        visibility: args.fields.visibility,
        manager: args.fields.manager.clone(),
        tags: args.fields.tags.clone(),
        description: args.fields.description.clone(),
        ..ProjectFieldArgs::default()
    };
    apply(ctx, &fields, &mut after, &mut extra).await?;
    if !extra.is_empty() {
        ctx.client.projects().update(Id::new(new_id), after).await?;
    }

    ctx.out.created(new_id)?;
    ctx.out.note(&format!(
        "created  project #{new_id}  {} (from {})",
        args.name, source.name
    ))?;
    ctx.out.hint(&format!(
        "hodoo board {new_id} — stages, tasks and dependencies came along"
    ))?;
    Ok(())
}

/// `hodoo project update <project>`
pub async fn update(ctx: &Ctx, text: &str, args: &ProjectFieldArgs) -> Result<(), Failure> {
    let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
    let mut fields = ProjectFields::default();
    let mut preview = Map::new();
    apply(ctx, args, &mut fields, &mut preview).await?;

    if preview.is_empty() {
        return Err(Failure::Usage(
            "nothing to change: pass a field, e.g. --visibility employees".to_owned(),
        ));
    }
    if ctx.dry_run {
        return Ok(prompt::preview(
            &ctx.out,
            "update the project",
            &preview.into(),
        )?);
    }
    ctx.client.projects().update(id, fields).await?;
    ctx.out.changed(id.get())?;
    ctx.out.note(&format!("updated  project #{}", id.get()))?;
    Ok(())
}

/// `hodoo project rm <project>`
pub async fn rm(ctx: &Ctx, text: &str, force: bool) -> Result<(), Failure> {
    let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
    let project = ctx.client.projects().get(id).await?;
    let action = format!(
        "delete project #{} \"{}\" and its {} tasks",
        id.get(),
        project.name,
        project.task_count
    );

    if ctx.dry_run {
        return Ok(ctx.out.note(&format!("would {action}"))?);
    }
    Ask::from_flags(force, ctx.no_input).destroy(&action)?;
    ctx.client.projects().delete(id).await?;
    ctx.out.removed(id.get())?;
    ctx.out.note(&format!(
        "deleted  project #{}  {}",
        id.get(),
        clip(&project.name, 40)
    ))?;
    Ok(())
}

/// `hodoo project stages <project>`
pub async fn stages(ctx: &Ctx, text: &str) -> Result<(), Failure> {
    let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
    let project = ctx.client.projects().get(id).await?;
    if ctx.out.mode() == Mode::Json {
        let stages = stage_list(ctx, id).await?;
        return Ok(ctx.out.print_json(&as_json(&stages))?);
    }
    if project.task_stages.is_empty() {
        return Ok(ctx.out.note(
            "no stages attached: a task cannot be staged until one is, so run \
             hodoo stage create --name Backlog --project <project>",
        )?);
    }
    stage_table(ctx, id).await
}

/// `hodoo project attach <project> --stage <stage>…`
pub async fn attach(ctx: &Ctx, text: &str, stages: &[String]) -> Result<(), Failure> {
    let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
    let mut ids = Vec::new();
    let mut said = Vec::new();
    for stage in stages {
        // Resolved without a project scope: the point is that it lives elsewhere.
        ids.push(refs::stage(&ctx.client, &Ref::parse(stage), None).await?);
        said.push(stage.clone());
    }
    if ctx.dry_run {
        return Ok(ctx.out.note(&format!(
            "would attach {} to project #{}",
            said.join(", "),
            id.get()
        ))?);
    }
    ctx.client.projects().attach_stages(id, &ids).await?;
    ctx.out.changed(id.get())?;
    ctx.out.note(&format!(
        "attached  {} to project #{}",
        said.join(", "),
        id.get()
    ))?;
    Ok(())
}

/// `hodoo project detach <project> --stage <stage>…`
pub async fn detach(ctx: &Ctx, text: &str, stages: &[String]) -> Result<(), Failure> {
    let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
    let mut ids = Vec::new();
    let mut said = Vec::new();
    for stage in stages {
        ids.push(refs::stage(&ctx.client, &Ref::parse(stage), Some(id)).await?);
        said.push(stage.clone());
    }
    if ctx.dry_run {
        return Ok(ctx.out.note(&format!(
            "would detach {} from project #{}",
            said.join(", "),
            id.get()
        ))?);
    }
    ctx.client.projects().detach_stages(id, &ids).await?;
    ctx.out.changed(id.get())?;
    ctx.out.note(&format!(
        "detached  {} from project #{}",
        said.join(", "),
        id.get()
    ))?;
    Ok(())
}

/// `hodoo project comment <project> --body <text>`
pub async fn comment(ctx: &Ctx, text: &str, body: &str, internal: bool) -> Result<(), Failure> {
    let id = refs::project(&ctx.client, &Ref::parse(text)).await?;
    let kind = if internal { "note" } else { "comment" };
    if ctx.dry_run {
        return Ok(ctx
            .out
            .note(&format!("would post a {kind} on project #{}", id.get()))?);
    }
    ctx.client.projects().comment(id, body, internal).await?;
    ctx.out.changed(id.get())?;
    ctx.out
        .note(&format!("posted  {kind} on project #{}", id.get()))?;
    Ok(())
}

/// The stages of a project, as read from Odoo.
///
/// # Errors
///
/// Any error of the underlying stage search.
pub async fn stage_list(ctx: &Ctx, project: ProjectId) -> Result<Vec<hodoo::TaskStage>, Failure> {
    Ok(ctx
        .client
        .stages()
        .list(hodoo::StageFilter {
            project: Some(project),
            limit: None,
            ..hodoo::StageFilter::default()
        })
        .await?)
}

/// Prints a project's stages with the open work in each.
async fn stage_table(ctx: &Ctx, project: ProjectId) -> Result<(), Failure> {
    let stages = stage_list(ctx, project).await?;
    if stages.is_empty() {
        return Ok(());
    }
    // One read of the project's tasks, counted here: cheaper and more honest than a
    // count call per stage.
    let tasks = ctx
        .client
        .tasks()
        .search(TaskFilter {
            project: Some(project),
            limit: None,
            ..TaskFilter::default()
        })
        .await?;
    let mut open: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for task in tasks.iter().filter(|task| !task.is_closed) {
        if let Some(stage) = task.stage {
            *open.entry(stage.get()).or_default() += 1;
        }
    }

    let mut table = Table::new(vec![
        Column::number("ID"),
        Column::text("STAGE").flexible(),
        Column::number("SEQ"),
        Column::text("FOLDED"),
        Column::number("OPEN"),
    ]);
    for stage in &stages {
        table.push([
            stage.id.to_string(),
            stage.name.clone(),
            stage.sequence.to_string(),
            if stage.fold { "yes" } else { "no" }.to_owned(),
            open.get(&stage.id.get())
                .copied()
                .unwrap_or_default()
                .to_string(),
        ]);
    }
    Ok(ctx.out.show(format!(
        "{}\n{}",
        ctx.out.bold("stages"),
        table.render(ctx.out).trim_end()
    ))?)
}

/// Turns the shared project flags into fields and a human preview.
async fn apply(
    ctx: &Ctx,
    args: &ProjectFieldArgs,
    fields: &mut ProjectFields,
    preview: &mut Map<String, Value>,
) -> Result<(), Failure> {
    if let Some(visibility) = args.visibility {
        fields.visibility = Some(match visibility {
            VisibilityArg::Followers => Visibility::Followers,
            VisibilityArg::InvitedUsers => Visibility::InvitedUsers,
            VisibilityArg::Employees => Visibility::Employees,
            VisibilityArg::Portal => Visibility::Portal,
        });
        preview.insert("visibility".into(), json!(visibility_word(visibility)));
    }
    if let Some(text) = &args.customer {
        let id = refs::partner(&ctx.client, &Ref::parse(text)).await?;
        fields.customer = Some(id);
        preview.insert("customer".into(), json!(format!("{text} (#{})", id.get())));
    }
    if let Some(text) = &args.manager {
        let id = refs::user(&ctx.client, &Ref::parse(text)).await?;
        fields.manager = Some(id);
        preview.insert("manager".into(), json!(format!("{text} (#{})", id.get())));
    }
    if let Some(text) = &args.stage {
        let id = project_stage(ctx, text).await?;
        fields.stage = Some(id);
        preview.insert("stage".into(), json!(format!("{text} (#{})", id.get())));
    }
    if let Some(description) = &args.description {
        fields.description = Some(description.clone());
        preview.insert("description".into(), json!(plain_text(description)));
    }
    if !args.tags.is_empty() {
        fields.tags = Some(tags_of(ctx, &args.tags).await?);
        preview.insert("tags".into(), json!(args.tags.join(", ")));
    }
    if let Some(start) = &args.start {
        let date = refs::date(start)?;
        fields.date_start = Some(date);
        preview.insert("starts".into(), json!(date.to_string()));
    }
    if let Some(end) = &args.end {
        let date = refs::date(end)?;
        fields.date = Some(date);
        preview.insert("ends".into(), json!(date.to_string()));
    }
    if args.milestones || args.no_milestones {
        let on = args.milestones && !args.no_milestones;
        fields.allow_milestones = Some(on);
        preview.insert("milestones".into(), json!(on));
    }
    if args.dependencies || args.no_dependencies {
        let on = args.dependencies && !args.no_dependencies;
        fields.allow_task_dependencies = Some(on);
        preview.insert("dependencies".into(), json!(on));
    }
    Ok(())
}

/// Resolves a `project.project.stage` by name or id.
async fn project_stage(ctx: &Ctx, text: &str) -> Result<ProjectStageId, Failure> {
    if let Ref::Id(id) = Ref::parse(text) {
        return Ok(Id::new(id));
    }
    let rows = ctx
        .client
        .call(
            "project.project.stage",
            "search_read",
            json!({ "domain": [["name", "ilike", text]], "fields": ["name"], "limit": 10 }),
        )
        .await?;
    rows.as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("id"))
        .and_then(Value::as_i64)
        .map(Id::new)
        .ok_or_else(|| {
            Failure::Usage(format!(
                "no project stage matches {text:?}. See them with: hodoo call \
                 project.project.stage search_read --body '{{\"fields\":[\"name\"]}}'"
            ))
        })
}

/// Resolves a project template by name or id.
async fn template_id(ctx: &Ctx, text: &str) -> Result<ProjectId, Failure> {
    if let Ref::Id(id) = Ref::parse(text) {
        return Ok(Id::new(id));
    }
    let rows = ctx
        .client
        .call(
            "project.project",
            "search_read",
            json!({
                "domain": [["is_template", "=", true], ["name", "ilike", text]],
                "fields": ["name"],
                "limit": 10,
            }),
        )
        .await?;
    let rows = rows.as_array().cloned().unwrap_or_default();
    match rows.len() {
        0 => Err(Failure::Usage(format!(
            "no project template matches {text:?}. Turn a project into one with: hodoo call \
             project.project write --ids <id> --body '{{\"vals\":{{\"is_template\":true}}}}'"
        ))),
        1 => Ok(Id::new(rows[0]["id"].as_i64().unwrap_or_default())),
        _ => Err(Failure::Usage(format!(
            "{text:?} matches {} templates; use an id",
            rows.len()
        ))),
    }
}

/// The Odoo value behind a visibility flag.
fn visibility_word(visibility: VisibilityArg) -> &'static str {
    match visibility {
        VisibilityArg::Followers => "followers",
        VisibilityArg::InvitedUsers => "invited-users",
        VisibilityArg::Employees => "employees",
        VisibilityArg::Portal => "portal",
    }
}

/// Resolves tag names, creating the ones that do not exist yet.
async fn tags_of(ctx: &Ctx, tags: &[String]) -> Result<Vec<Id<hodoo::Tag>>, Failure> {
    let mut ids = Vec::new();
    for tag in tags {
        ids.push(refs::tag(&ctx.client, tag).await?);
    }
    Ok(ids)
}

/// A name for an id, or a dash.
fn lookup(names: &std::collections::HashMap<i64, String>, id: Option<i64>) -> String {
    id.and_then(|id| names.get(&id).cloned())
        .unwrap_or_else(|| "-".to_owned())
}

/// A finished project is dimmed: it needs no attention.
fn finished(project: &Project) -> Option<Style> {
    (project.open_task_count == 0 && project.task_count > 0).then_some(Style::Dim)
}

/// Serializes a value, or null: a display detail must never fail a command.
fn as_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}
