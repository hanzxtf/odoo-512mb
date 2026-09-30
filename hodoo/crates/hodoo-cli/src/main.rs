//! `hodoo`: the CLI for the `hodoo` client.
//!
//! This file decides three things and nothing else: how arguments become a call, how a
//! failure becomes an exit code, and where each kind of message goes. Everything else
//! lives in a module with one job - the surface in [`cli`], rendering in [`output`],
//! resolution in [`refs`], questions in [`prompt`], and the commands in [`cmd`].

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

mod cli;
mod cmd;
mod output;
mod prompt;
mod refs;

use std::io::Write;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Command, ProjectCmd, TaskCmd};
use crate::cmd::Ctx;

/// A closed pipe is not a crash: it is what `| head` looks like.
const PIPE_CLOSED: u8 = 141;

/// How a run failed, and what the shell should be told.
#[derive(Debug)]
enum Failure {
    /// The invocation cannot work: exit 2.
    Usage(String),
    /// The user said no, or there was nobody to ask: exit 1.
    Aborted(String),
    /// Odoo or the network said no: exit 1.
    Odoo(hodoo::Error),
    /// A local read or write failed: exit 1.
    Io(std::io::Error),
}

impl Failure {
    fn code(&self) -> u8 {
        match self {
            Failure::Usage(_) => 2,
            Failure::Aborted(_) | Failure::Odoo(_) | Failure::Io(_) => 1,
        }
    }

    /// The message a person reads. Odoo's message is already a sentence; the others
    /// are written as one.
    fn message(&self) -> String {
        match self {
            Failure::Usage(message) | Failure::Aborted(message) => message.clone(),
            Failure::Odoo(error) => error.to_string(),
            Failure::Io(error) => format!("could not read or write: {error}"),
        }
    }

    /// The machine-readable form, for `-o json`.
    fn to_value(&self) -> serde_json::Value {
        match self {
            Failure::Usage(message) => {
                serde_json::json!({ "kind": "usage", "message": message })
            }
            Failure::Aborted(message) => {
                serde_json::json!({ "kind": "aborted", "message": message })
            }
            Failure::Io(error) => {
                serde_json::json!({ "kind": "io", "message": error.to_string() })
            }
            Failure::Odoo(error) => {
                let mut body = serde_json::Map::new();
                body.insert("kind".into(), serde_json::json!(error.kind()));
                body.insert("message".into(), serde_json::json!(error.to_string()));
                if let Some(status) = error.status() {
                    body.insert("status".into(), serde_json::json!(status));
                }
                if let hodoo::Error::Odoo { name, .. } = error {
                    body.insert("exception".into(), serde_json::json!(name));
                }
                serde_json::Value::Object(body)
            }
        }
    }

    /// Odoo's Python traceback, when it sent one and `-v` was passed.
    fn traceback(&self) -> Option<&str> {
        match self {
            Failure::Odoo(hodoo::Error::Odoo { debug, .. }) => debug.as_deref(),
            _ => None,
        }
    }
}

impl From<hodoo::Error> for Failure {
    fn from(error: hodoo::Error) -> Self {
        // A configuration error is the caller's argument, not Odoo's answer.
        match error {
            hodoo::Error::Config { message } => Failure::Usage(message),
            other => Failure::Odoo(other),
        }
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Failure::Io(error)
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Failure::Usage(message)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    // No arguments at all explains itself, on stderr and with exit 2. clap's
    // `arg_required_else_help` stopped doing that in 4.6: it now answers with its own
    // "requires a subcommand" error, which lists the subcommands but not how to start.
    // Rendering the long help here keeps the promise the help text makes.
    if std::env::args_os().len() == 1 {
        let help = Cli::command().render_long_help();
        eprint!("{help}");
        return ExitCode::from(2);
    }
    let args = Cli::parse();
    match run(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            let json = args.global.json || args.global.output.as_deref() == Some("json");
            if report(&failure, json, args.global.pretty, args.global.verbose).is_err() {
                return ExitCode::from(PIPE_CLOSED);
            }
            ExitCode::from(failure.code())
        }
    }
}

async fn run(args: &Cli) -> Result<(), Failure> {
    let global = &args.global;
    // Version needs no key, so it is answered before a client is built.
    if matches!(args.command, Command::Version) {
        let ctx = Ctx::new(global)?;
        return cmd::account::version(&ctx, &ctx.url()).await;
    }
    if let Command::Completions(args) = &args.command {
        return cmd::completions::run(args);
    }

    let ctx = Ctx::new(global)?;
    match &args.command {
        Command::Whoami => cmd::account::whoami(&ctx, &ctx.url()).await,
        Command::Version => unreachable!("handled above, before the client is built"),
        Command::Project(command) => project(&ctx, command).await,
        Command::Task(command) => task(&ctx, command).await,
        Command::Milestone(command) => cmd::milestone::run(&ctx, command).await,
        Command::Tag(command) => cmd::tag::run(&ctx, command).await,
        Command::Board(args) => cmd::board::run(&ctx, args).await,
        Command::Call(args) => cmd::call::run(&ctx, args).await,
        Command::Completions(_) => unreachable!("handled above"),
    }
}

async fn project(ctx: &Ctx, command: &ProjectCmd) -> Result<(), Failure> {
    match command {
        ProjectCmd::Ls(args) => cmd::project::ls(ctx, args).await,
        ProjectCmd::Show { project } => cmd::project::show(ctx, project).await,
        ProjectCmd::Create(args) => cmd::project::create(ctx, args).await,
        ProjectCmd::Update { project, fields } => cmd::project::update(ctx, project, fields).await,
        ProjectCmd::Rm { project, force } => cmd::project::rm(ctx, project, *force).await,
        ProjectCmd::Stages { command } => cmd::stage::run_project_stages(ctx, command).await,
        ProjectCmd::TaskStages { command } => cmd::stage::run_task_stages(ctx, command).await,
        ProjectCmd::Attach { project, stages } => cmd::project::attach(ctx, project, stages).await,
        ProjectCmd::Detach { project, stages } => cmd::project::detach(ctx, project, stages).await,
        ProjectCmd::Comment {
            project,
            body,
            internal,
        } => cmd::project::comment(ctx, project, body, *internal).await,
    }
}

async fn task(ctx: &Ctx, command: &TaskCmd) -> Result<(), Failure> {
    match command {
        TaskCmd::Ls(args) => cmd::task::ls(ctx, args).await,
        TaskCmd::Show { task } => cmd::task::show(ctx, task).await,
        TaskCmd::Create(args) => cmd::task::create(ctx, args).await,
        TaskCmd::Update { task, fields } => cmd::task::update(ctx, task, fields).await,
        TaskCmd::Done { task } => cmd::task::set_state(ctx, task, hodoo::TaskState::Done).await,
        TaskCmd::Cancel { task } => {
            cmd::task::set_state(ctx, task, hodoo::TaskState::Canceled).await
        }
        TaskCmd::Reopen { task } => {
            cmd::task::set_state(ctx, task, hodoo::TaskState::InProgress).await
        }
        TaskCmd::Move { task, stage } => cmd::task::move_to(ctx, task, stage).await,
        TaskCmd::Comment {
            task,
            body,
            internal,
        } => cmd::task::comment(ctx, task, body, *internal).await,
        TaskCmd::Messages { task, limit } => cmd::task::messages(ctx, task, *limit).await,
        TaskCmd::Deps { task } => cmd::task::deps(ctx, task).await,
        TaskCmd::Rm { task, force } => cmd::task::rm(ctx, task, *force).await,
    }
}

/// Reports a failure the way a person reads it: one sentence on stderr, the traceback
/// only when asked, and JSON instead when the caller asked for JSON.
fn report(failure: &Failure, json: bool, pretty: bool, verbose: bool) -> std::io::Result<()> {
    let mut stderr = std::io::stderr().lock();
    if json {
        let body = serde_json::json!({ "error": failure.to_value() });
        let text = if pretty {
            serde_json::to_string_pretty(&body)
        } else {
            serde_json::to_string(&body)
        }
        .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"));
        return writeln!(stderr, "{text}");
    }
    writeln!(stderr, "hodoo: {}", failure.message())?;
    if verbose && let Some(traceback) = failure.traceback() {
        writeln!(stderr, "{traceback}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn a_usage_failure_is_exit_two_and_the_rest_are_exit_one() {
        assert_eq!(Failure::Usage("x".into()).code(), 2);
        assert_eq!(Failure::Aborted("x".into()).code(), 1);
        assert_eq!(
            Failure::Odoo(hodoo::Error::Missing {
                model: "project.task".into(),
                raw_id: 1
            })
            .code(),
            1
        );
    }

    #[test]
    fn a_configuration_error_is_the_callers_argument_not_odos_answer() {
        let failure: Failure = hodoo::Error::Config {
            message: "no server".into(),
        }
        .into();
        assert_eq!(failure.code(), 2, "a bad invocation exits 2, not 1");
    }

    #[test]
    fn the_command_surface_is_internally_consistent() {
        // Catches duplicate flags, broken defaults and bad help wiring at test time
        // rather than when a user hits it.
        Cli::command().debug_assert();
    }

    #[test]
    fn the_help_text_carries_the_things_people_need() {
        let help = Cli::command().render_long_help().to_string();
        for expected in ["Getting started", "ODDO_URL", "Exit codes", "HODOO_OUTPUT"] {
            // ODOO_URL is spelled out in the help; the typo above would fail the test.
            let expected = expected.replace("ODDO_URL", "ODOO_URL");
            assert!(help.contains(&expected), "help is missing {expected:?}");
        }
        for command in ["project", "task", "call", "completions", "board"] {
            let sub = Cli::command().find_subcommand(command).cloned();
            assert!(sub.is_some(), "help has no {command} subcommand");
        }
    }

    #[test]
    fn every_subcommand_documents_an_example() {
        fn walk(command: &clap::Command, path: &str, missing: &mut Vec<String>) {
            for sub in command.get_subcommands() {
                let name = format!("{path} {}", sub.get_name());
                let long = sub
                    .get_long_about()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                if sub.get_subcommands().next().is_none()
                    && !long.contains("Examples:")
                    && !matches!(
                        sub.get_name(),
                        "help" | "version" | "completions" | "ls" | "whoami"
                    )
                {
                    missing.push(name.clone());
                }
                walk(sub, &name, missing);
            }
        }
        let mut missing = Vec::new();
        walk(&Cli::command(), "hodoo", &mut missing);
        assert!(missing.is_empty(), "no examples in: {missing:?}");
    }
}
