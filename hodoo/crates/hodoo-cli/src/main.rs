//! `hodoo`: the CLI for the `hodoo` client.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod cli;

use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use hodoo::{
    Client, Config, Error, Id, MilestoneFields, MilestoneId, PartnerId, Priority, ProjectFields,
    ProjectFilter, ProjectId, ProjectStageId, StageFields, StageFilter, TagId, TaskFields,
    TaskFilter, TaskId, TaskStageId, TaskState, UserId, Visibility,
};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::cli::{
    CallArgs, Cli, Command, Global, MilestoneCmd, PriorityArg, ProjectCmd, ProjectFieldArgs,
    ProjectLsArgs, StageCmd, StateArg, TagCmd, TaskCmd, TaskFieldArgs, TaskLsArgs, VisibilityArg,
};

/// A failure, and how loudly to report it.
enum Failure {
    /// The command line itself is wrong: exit 2.
    Usage(String),
    /// Odoo, the network or the response was the problem: exit 1.
    Odoo(Error),
}

impl Failure {
    fn code(&self) -> u8 {
        match self {
            Failure::Usage(_) => 2,
            Failure::Odoo(_) => 1,
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Failure::Usage(message) => json!({ "kind": "usage", "message": message }),
            Failure::Odoo(error) => {
                let mut body = Map::new();
                body.insert("kind".into(), json!(error.kind()));
                body.insert("message".into(), json!(error.to_string()));
                if let Some(status) = error.status() {
                    body.insert("status".into(), json!(status));
                }
                if let Error::Odoo { name, .. } = error {
                    body.insert("exception".into(), json!(name));
                }
                Value::Object(body)
            }
        }
    }

    /// The Python traceback, when Odoo sent one.
    fn debug(&self) -> Option<&str> {
        match self {
            Failure::Odoo(Error::Odoo { debug, .. }) => debug.as_deref(),
            _ => None,
        }
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        // A config error is the caller's argument, not Odoo's answer.
        match error {
            Error::Config { message } => Failure::Usage(message),
            other => Failure::Odoo(other),
        }
    }
}

type CliResult = std::result::Result<Value, Failure>;

#[tokio::main]
async fn main() -> ExitCode {
    let args = Cli::parse();
    match run(&args).await {
        Ok(value) => {
            if print(&value, args.global.pretty).is_err() {
                // A closed pipe (`hodoo task ls | head`) is not a crash: exit the
                // way a SIGPIPE'd process would, and say nothing.
                return ExitCode::from(PIPE_CLOSED);
            }
            ExitCode::SUCCESS
        }
        Err(failure) => {
            let error = json!({ "error": failure.to_value() });
            // The report goes to stderr, so a piped stdout stays parseable JSON.
            if report(&error, args.global.pretty).is_err() {
                return ExitCode::from(PIPE_CLOSED);
            }
            if args.global.verbose
                && let Some(traceback) = failure.debug()
                && report(&Value::String(traceback.to_owned()), false).is_err()
            {
                return ExitCode::from(PIPE_CLOSED);
            }
            ExitCode::from(failure.code())
        }
    }
}

async fn run(args: &Cli) -> CliResult {
    let global = &args.global;
    match &args.command {
        Command::Whoami => client(global)?.whoami().await.map_err(Failure::from),
        Command::Version => {
            let version = client(global)?.version().await.map_err(Failure::from)?;
            Ok(json!({ "version": version }))
        }
        Command::Project(command) => project(global, command).await,
        Command::Task(command) => task(global, command).await,
        Command::Stage(command) => stage(global, command).await,
        Command::Milestone(command) => milestone(global, command).await,
        Command::Tag(command) => tag(global, command).await,
        Command::Call(args) => call(global, args).await,
    }
}

fn client(global: &Global) -> std::result::Result<Client, Failure> {
    // A `.env` at or above the working directory fills in whatever the flags
    // and the shell did not provide. The environment wins over the file.
    let file = hodoo::dotenv::load(".")
        .map_err(|error| Failure::Usage(format!("could not read a .env file: {error}")))?
        .unwrap_or_default();

    let url = global
        .url
        .clone()
        .or_else(|| hodoo::dotenv::resolve("ODOO_URL", &file))
        .ok_or_else(|| {
            Failure::Usage(
                "no server: pass --url, set ODOO_URL, or put ODOO_URL in a .env".to_owned(),
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

    let mut config = Config::new(url, api_key)?
        .with_timeout(Duration::from_secs(global.timeout))
        .accept_invalid_certs(global.insecure)
        .retry_reads(!global.no_retry);
    if let Some(db) = db {
        config = config.with_db(db);
    }
    Client::new(config).map_err(Failure::from)
}

async fn project(global: &Global, command: &ProjectCmd) -> CliResult {
    let client = client(global)?;
    let projects = client.projects();
    match command {
        ProjectCmd::Ls(args) => {
            let found = projects
                .search(project_filter(args))
                .await
                .map_err(Failure::from)?;
            to_value(&found)
        }
        ProjectCmd::Get { id } => {
            let found = projects.get(ProjectId::new(*id)).await?;
            to_value(&found)
        }
        ProjectCmd::Create(args) => {
            let id = projects
                .create(project_fields(Some(args.name.clone()), &args.fields)?)
                .await?;
            Ok(json!({ "id": id.get() }))
        }
        ProjectCmd::Update { id, fields } => {
            projects
                .update(ProjectId::new(*id), project_fields(None, fields)?)
                .await?;
            Ok(ok())
        }
        ProjectCmd::Rm { id } => {
            projects.delete(ProjectId::new(*id)).await?;
            Ok(ok())
        }
        ProjectCmd::Stages { id } => {
            let project = projects.get(ProjectId::new(*id)).await?;
            let stages = client
                .stages()
                .list(StageFilter {
                    project: Some(project.id),
                    ..StageFilter::default()
                })
                .await?;
            to_value(&stages)
        }
        ProjectCmd::AttachStage { id, stages } => {
            projects
                .attach_stages(ProjectId::new(*id), &stage_ids(stages))
                .await?;
            Ok(ok())
        }
        ProjectCmd::DetachStage { id, stages } => {
            projects
                .detach_stages(ProjectId::new(*id), &stage_ids(stages))
                .await?;
            Ok(ok())
        }
        ProjectCmd::Comment { id, body, internal } => {
            projects
                .comment(ProjectId::new(*id), body, *internal)
                .await?;
            Ok(ok())
        }
    }
}

async fn task(global: &Global, command: &TaskCmd) -> CliResult {
    let client = client(global)?;
    let tasks = client.tasks();
    match command {
        TaskCmd::Ls(args) => {
            let found = tasks.search(task_filter(args)?).await?;
            to_value(&found)
        }
        TaskCmd::Get { id } => {
            let found = tasks.get(TaskId::new(*id)).await?;
            to_value(&found)
        }
        TaskCmd::Create(args) => {
            let id = tasks
                .create(task_fields(Some(args.name.clone()), &args.fields)?)
                .await?;
            Ok(json!({ "id": id.get() }))
        }
        TaskCmd::Update { id, fields } => {
            tasks
                .update(TaskId::new(*id), task_fields(None, fields)?)
                .await?;
            Ok(ok())
        }
        TaskCmd::Rm { id } => {
            tasks.delete(TaskId::new(*id)).await?;
            Ok(ok())
        }
        TaskCmd::Done { id } => {
            tasks.set_state(TaskId::new(*id), TaskState::Done).await?;
            Ok(ok())
        }
        TaskCmd::Cancel { id } => {
            tasks
                .set_state(TaskId::new(*id), TaskState::Canceled)
                .await?;
            Ok(ok())
        }
        TaskCmd::Comment { id, body, internal } => {
            tasks.comment(TaskId::new(*id), body, *internal).await?;
            Ok(ok())
        }
        TaskCmd::Messages { id, limit } => {
            let messages = tasks.messages(TaskId::new(*id), Some(*limit)).await?;
            to_value(&messages)
        }
        TaskCmd::Deps { id } => {
            let dependencies = tasks.dependencies(TaskId::new(*id)).await?;
            to_value(&dependencies)
        }
    }
}

async fn stage(global: &Global, command: &StageCmd) -> CliResult {
    let client = client(global)?;
    match command {
        StageCmd::Ls { project, limit } => {
            let found = client
                .stages()
                .list(StageFilter {
                    project: project.map(ProjectId::new),
                    limit: limit_of(*limit),
                    ..StageFilter::default()
                })
                .await?;
            to_value(&found)
        }
        StageCmd::Create {
            name,
            project,
            sequence,
            fold,
        } => {
            let fields = StageFields {
                name: Some(name.clone()),
                sequence: *sequence,
                fold: fold.then_some(true),
                ..StageFields::default()
            };
            let id = match project {
                Some(project) => {
                    client
                        .stages()
                        .create_in(ProjectId::new(*project), fields)
                        .await?
                }
                None => client.stages().create(fields).await?,
            };
            Ok(json!({ "id": id.get() }))
        }
    }
}

async fn milestone(global: &Global, command: &MilestoneCmd) -> CliResult {
    let client = client(global)?;
    let milestones = client.milestones();
    match command {
        MilestoneCmd::Ls { project } => {
            let found = milestones.list(ProjectId::new(*project)).await?;
            to_value(&found)
        }
        MilestoneCmd::Create {
            project,
            name,
            deadline,
        } => {
            let fields = MilestoneFields {
                name: Some(name.clone()),
                project: Some(ProjectId::new(*project)),
                deadline: date_of(deadline.as_deref())?,
                ..MilestoneFields::default()
            };
            let id = milestones.create(fields).await?;
            Ok(json!({ "id": id.get() }))
        }
        MilestoneCmd::Reached { id, undo } => {
            milestones.set_reached(MilestoneId::new(*id), !undo).await?;
            Ok(ok())
        }
        MilestoneCmd::Rm { id } => {
            milestones.delete(MilestoneId::new(*id)).await?;
            Ok(ok())
        }
    }
}

async fn tag(global: &Global, command: &TagCmd) -> CliResult {
    let client = client(global)?;
    match command {
        TagCmd::Ls => to_value(&client.tags().list().await?),
        TagCmd::Ensure { name } => {
            let id = client.tags().ensure(name).await?;
            Ok(json!({ "id": id.get() }))
        }
    }
}

async fn call(global: &Global, args: &CallArgs) -> CliResult {
    let client = client(global)?;
    let mut body = match &args.json {
        Some(text) => serde_json::from_str::<Value>(text)
            .map_err(|error| Failure::Usage(format!("--json is not valid JSON: {error}")))?,
        None => json!({}),
    };
    let object = body.as_object_mut().ok_or_else(|| {
        Failure::Usage("--json must be a JSON object of named arguments".to_owned())
    })?;
    if !args.ids.is_empty() {
        object.insert("ids".into(), json!(args.ids));
    }
    client
        .call(&args.model, &args.method, body)
        .await
        .map_err(Failure::from)
}

fn project_filter(args: &ProjectLsArgs) -> ProjectFilter {
    ProjectFilter {
        name_contains: args.name.clone(),
        customer: args.customer.map(PartnerId::new),
        manager: args.manager.map(UserId::new),
        stage: args.stage.map(ProjectStageId::new),
        tags: tag_ids(&args.tags),
        order: args.order.clone(),
        limit: limit_of(args.limit),
        offset: if args.offset == 0 {
            None
        } else {
            Some(args.offset)
        },
    }
}

fn project_fields(
    name: Option<String>,
    args: &ProjectFieldArgs,
) -> std::result::Result<ProjectFields, Failure> {
    Ok(ProjectFields {
        name,
        description: args.description.clone(),
        customer: args.customer.map(PartnerId::new),
        manager: args.manager.map(UserId::new),
        stage: args.stage.map(ProjectStageId::new),
        date_start: date_of(args.date_start.as_deref())?,
        date: date_of(args.date.as_deref())?,
        visibility: args.visibility.map(visibility),
        tags: optional_ids(&args.tags),
        allow_milestones: args.allow_milestones.then_some(true),
        allow_task_dependencies: args.allow_task_dependencies.then_some(true),
    })
}

fn task_filter(args: &TaskLsArgs) -> std::result::Result<TaskFilter, Failure> {
    Ok(TaskFilter {
        project: args.project.map(ProjectId::new),
        stage: args.stage.map(TaskStageId::new),
        assignee: args.assignee.map(UserId::new),
        open_only: args.open,
        state: args.state.map(state),
        name_contains: args.name.clone(),
        tags: tag_ids(&args.tags),
        deadline_before: args
            .deadline_before
            .as_deref()
            .map(hodoo::datetime::parse_datetime)
            .transpose()?,
        parent: args.parent.map(TaskId::new),
        order: args.order.clone(),
        limit: limit_of(args.limit),
        offset: if args.offset == 0 {
            None
        } else {
            Some(args.offset)
        },
    })
}

fn task_fields(
    name: Option<String>,
    args: &TaskFieldArgs,
) -> std::result::Result<TaskFields, Failure> {
    Ok(TaskFields {
        name,
        description: args.description.clone(),
        project: args.project.map(ProjectId::new),
        stage: args.stage.map(TaskStageId::new),
        state: args.state.map(state),
        priority: args.priority.map(priority),
        assignees: if args.unassign {
            Some(Vec::new())
        } else {
            optional_ids(&args.assignees)
        },
        customer: args.customer.map(PartnerId::new),
        deadline: args
            .deadline
            .as_deref()
            .map(hodoo::datetime::parse_datetime)
            .transpose()?,
        allocated_hours: args.allocated_hours,
        tags: if args.clear_tags {
            Some(Vec::new())
        } else {
            optional_ids(&args.tags)
        },
        parent: args.parent.map(TaskId::new),
        milestone: args.milestone.map(MilestoneId::new),
        depends_on: optional_ids(&args.depends_on),
    })
}

/// A repeated `--flag <id>` becomes `Some(ids)`, and its absence `None`, so a
/// write never clears a collection the caller did not mention.
fn optional_ids<T>(raw: &[i64]) -> Option<Vec<Id<T>>> {
    if raw.is_empty() {
        None
    } else {
        Some(raw.iter().copied().map(Id::new).collect())
    }
}

fn tag_ids(raw: &[i64]) -> Vec<TagId> {
    raw.iter().copied().map(TagId::new).collect()
}

fn stage_ids(raw: &[i64]) -> Vec<TaskStageId> {
    raw.iter().copied().map(TaskStageId::new).collect()
}

/// `--limit 0` is the CLI's spelling of "no limit".
fn limit_of(limit: u32) -> Option<u32> {
    if limit == 0 { None } else { Some(limit) }
}

fn date_of(raw: Option<&str>) -> std::result::Result<Option<chrono::NaiveDate>, Failure> {
    raw.map(hodoo::datetime::parse_date)
        .transpose()
        .map_err(Failure::from)
}

fn visibility(arg: VisibilityArg) -> Visibility {
    match arg {
        VisibilityArg::Followers => Visibility::Followers,
        VisibilityArg::InvitedUsers => Visibility::InvitedUsers,
        VisibilityArg::Employees => Visibility::Employees,
        VisibilityArg::Portal => Visibility::Portal,
    }
}

fn state(arg: StateArg) -> TaskState {
    match arg {
        StateArg::InProgress => TaskState::InProgress,
        StateArg::ChangesRequested => TaskState::ChangesRequested,
        StateArg::Approved => TaskState::Approved,
        StateArg::Done => TaskState::Done,
        StateArg::Canceled => TaskState::Canceled,
        StateArg::Waiting => TaskState::Waiting,
    }
}

fn priority(arg: PriorityArg) -> Priority {
    match arg {
        PriorityArg::Low => Priority::Low,
        PriorityArg::Medium => Priority::Medium,
        PriorityArg::High => Priority::High,
        PriorityArg::Urgent => Priority::Urgent,
    }
}

fn ok() -> Value {
    json!({ "ok": true })
}

fn to_value<T: Serialize>(value: &T) -> CliResult {
    serde_json::to_value(value)
        .map_err(|error| Failure::Usage(format!("could not render the result as JSON: {error}")))
}

/// A closed pipe, reported the way a SIGPIPE'd process would be.
const PIPE_CLOSED: u8 = 141;

/// Renders one JSON document.
fn render(value: &Value, pretty: bool) -> String {
    let rendered = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    };
    rendered.unwrap_or_else(|error| format!("{{\"error\":\"could not render JSON: {error}\"}}"))
}

/// Writes one JSON document to stdout, one line unless `pretty`.
///
/// Returns the I/O error instead of panicking: `println!` panics on a closed
/// pipe, which would turn `hodoo task ls | head` into a backtrace.
fn print(value: &Value, pretty: bool) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    std::io::Write::write_all(&mut stdout, render(value, pretty).as_bytes())?;
    std::io::Write::write_all(&mut stdout, b"\n")
}

/// Writes the error report to stderr, tolerating a closed pipe there too.
fn report(value: &Value, pretty: bool) -> std::io::Result<()> {
    let mut stderr = std::io::stderr().lock();
    std::io::Write::write_all(&mut stderr, render(value, pretty).as_bytes())?;
    std::io::Write::write_all(&mut stderr, b"\n")
}
