//! Confirmation before destructive work, and the dry-run view.
//!
//! Destroying a project takes its tasks, its chatter and its milestones with it, so
//! it asks first - unless told not to. The rules are clig.dev's: never *require* a
//! prompt (there is always `-f`), never prompt when stdin is not a terminal, and
//! never prompt at all with `--no-input`.

use std::io::{IsTerminal, Write};

use serde_json::Value;

use crate::Failure;
use crate::output::{Column, Output, Table};

/// Whether the user is willing to be asked.
#[derive(Debug, Clone, Copy)]
pub struct Ask {
    force: bool,
    no_input: bool,
    stdin_is_tty: bool,
}

impl Ask {
    /// Reads `-f`/`--force` and `--no-input`, and checks whether stdin is a terminal.
    #[must_use]
    pub fn from_flags(force: bool, no_input: bool) -> Self {
        Self {
            force,
            no_input,
            stdin_is_tty: std::io::stdin().is_terminal(),
        }
    }

    /// Asks before destroying something.
    ///
    /// `action` names what is about to happen (`delete project #49 and its 8 tasks`),
    /// so the question says something useful.
    ///
    /// # Errors
    ///
    /// [`Failure::Aborted`] when the answer was no, and [`Failure::Usage`] when there
    /// is nobody to ask: an unattended run has to pass `-f` on purpose.
    pub fn destroy(self, action: &str) -> Result<(), Failure> {
        if self.force {
            return Ok(());
        }
        if self.no_input || !self.stdin_is_tty {
            return Err(Failure::Usage(format!(
                "refusing to {action} without confirmation. Pass -f/--force to do it anyway"
            )));
        }

        let mut stderr = std::io::stderr().lock();
        write!(stderr, "about to {action}\ncontinue? [y/N] ")?;
        stderr.flush()?;
        drop(stderr);

        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer).is_err() {
            return Err(Failure::Aborted(
                "could not read the answer; nothing changed".to_owned(),
            ));
        }
        match answer.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => Ok(()),
            _ => Err(Failure::Aborted("aborted; nothing changed".to_owned())),
        }
    }
}

/// Shows what a mutation *would* send, and sends nothing.
///
/// # Errors
///
/// The I/O error of writing to stdout.
pub fn preview(out: &Output, action: &str, fields: &Value) -> std::io::Result<()> {
    let mut table = Table::new(vec![
        Column::text("FIELD"),
        Column::text("VALUE").flexible(),
    ]);
    if let Some(object) = fields.as_object() {
        for (field, value) in object {
            table.push([field.clone(), render(value)]);
        }
    }
    let mut stdout = std::io::stdout().lock();
    writeln!(
        stdout,
        "{}\n{}{}",
        out.bold(&format!("would {action}")),
        table.render(*out),
        out.dim("nothing was sent; drop --dry-run to apply")
    )
}

fn render(value: &Value) -> String {
    match value {
        Value::Null => "-".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) if text.is_empty() => "-".to_owned(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(render).collect::<Vec<_>>().join(", "),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| format!("{key}: {}", render(value)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}
