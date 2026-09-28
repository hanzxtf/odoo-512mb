//! `hodoo completions <shell>`.

use clap::CommandFactory;
use clap_complete::generate;

use crate::Failure;
use crate::cli::{Cli, CompletionsArgs};

/// Prints a completion script for the given shell.
///
/// # Errors
///
/// The I/O error of writing the script to stdout.
pub fn run(args: &CompletionsArgs) -> Result<(), Failure> {
    let mut command = Cli::command();
    let name = command.get_name().to_owned();
    let mut stdout = std::io::stdout();
    generate(args.shell, &mut command, name, &mut stdout);
    Ok(())
}
