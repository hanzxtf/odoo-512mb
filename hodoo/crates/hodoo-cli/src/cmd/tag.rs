//! `hodoo tag ls`

use crate::Failure;
use crate::cli::TagCmd;
use crate::cmd::{Ctx, as_json};
use crate::output::{Column, Mode, Table};

/// Dispatches the tag subcommands.
///
/// # Errors
///
/// Any error of the underlying tag search.
pub async fn run(ctx: &Ctx, command: &TagCmd) -> Result<(), Failure> {
    match command {
        TagCmd::Ls => {
            let tags = ctx.client.tags().list().await?;
            if ctx.out.mode() == Mode::Json {
                return Ok(ctx.out.print_json(&as_json(&tags))?);
            }
            if tags.is_empty() {
                return Ok(ctx
                    .out
                    .note("no tags yet. `--tag <name>` on a task creates one on the spot")?);
            }
            let mut table = Table::new(vec![Column::number("ID"), Column::text("TAG")]);
            for tag in &tags {
                table.push([tag.id.to_string(), tag.name.clone()]);
            }
            Ok(ctx.out.print_table(&table)?)
        }
    }
}
