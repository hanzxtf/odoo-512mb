//! Rendering: how a result reaches the terminal.
//!
//! Two modes, and the choice matters to whoever is reading:
//!
//! - [`Mode::Table`] (the default) is for people: aligned columns, colour when the
//!   terminal supports it, deadlines as "in 3d" rather than a timestamp.
//! - [`Mode::Json`] is a contract for scripts: Odoo's own field names, every field
//!   the client read, no colour, no decoration, errors as JSON on stderr.
//!
//! `stdout` carries the result only; hints and errors go to stderr, so a pipe sees a
//! clean record stream.

use std::io::{IsTerminal, Write};

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};

/// How results are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A table for people.
    Table,
    /// JSON for machines.
    Json,
}

impl Mode {
    /// Parses `-o`/`--output` and `HODOO_OUTPUT`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "table" | "text" => Some(Mode::Table),
            "json" => Some(Mode::Json),
            _ => None,
        }
    }
}

/// Whether colour is allowed, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Colour when the stream is a terminal (the default).
    Auto,
    /// Always colour, even into a pipe.
    Always,
    /// Never colour.
    Never,
}

/// Everything the renderer needs to know about the place it is writing to.
#[derive(Debug, Clone, Copy)]
pub struct Output {
    mode: Mode,
    color: bool,
    pretty: bool,
    headers: bool,
    quiet: bool,
    /// Width to lay tables out for, when it is known.
    width: usize,
}

impl Output {
    /// Resolves the settings: explicit flags first, then the environment, then the
    /// terminal. `stdout` being a pipe does not change the *mode* (a pipe should see
    /// what a human sees, `grep` included), but it does turn colour off.
    pub fn resolve(
        output_flag: Option<&str>,
        json_flag: bool,
        color_flag: Option<ColorChoice>,
        no_headers: bool,
        quiet: bool,
        pretty: bool,
    ) -> Result<Self, String> {
        let mode = if json_flag {
            Mode::Json
        } else if let Some(text) = output_flag {
            Mode::parse(text)
                .ok_or_else(|| format!("unknown output format {text:?}: try table or json"))?
        } else if let Ok(text) = std::env::var("HODOO_OUTPUT") {
            Mode::parse(&text).ok_or_else(|| {
                format!("HODOO_OUTPUT={text:?} is not a format: try table or json")
            })?
        } else {
            Mode::Table
        };

        let stdout_is_tty = std::io::stdout().is_terminal();
        // Precedence follows the conventions a terminal already honours: an
        // explicit flag wins, then NO_COLOR/TERM=dumb suppress and CLICOLOR_FORCE
        // forces, and only then does "is stdout a terminal" decide.
        let color = match color_flag.unwrap_or(ColorChoice::Auto) {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                let requested = std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty());
                let suppressed = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
                    || std::env::var("TERM").is_ok_and(|term| term == "dumb");
                requested || (stdout_is_tty && !suppressed)
            }
        };

        Ok(Self {
            mode,
            color: color && mode == Mode::Table,
            pretty,
            headers: !no_headers,
            quiet,
            width: terminal_width(stdout_is_tty),
        })
    }

    /// The selected mode.
    #[must_use]
    pub fn mode(self) -> Mode {
        self.mode
    }

    /// Prints a result of unknown shape: JSON for a machine, and for a person
    /// whatever fits the data - a table of records, a key/value block for one
    /// record, or a bare value. This is what `hodoo call` renders, where the shape
    /// belongs to Odoo rather than to us.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout, so a closed pipe can be reported rather
    /// than panicked over.
    pub fn print_auto(self, value: &Value) -> std::io::Result<()> {
        if self.mode == Mode::Json {
            return self.print_json(value);
        }
        match value {
            Value::Array(rows) => self.print_rows(rows),
            Value::Object(fields) => {
                if fields.is_empty() {
                    return writeln!(std::io::stdout().lock(), "{{}}");
                }
                let mut table = Table::new(vec![
                    Column::text("FIELD"),
                    Column::text("VALUE").flexible(),
                ]);
                for (key, value) in fields {
                    table.push([key.clone(), inline(value)]);
                }
                self.print_table(&table)
            }
            Value::Null => writeln!(std::io::stdout().lock()),
            other => writeln!(std::io::stdout().lock(), "{}", inline(other)),
        }
    }

    /// Prints an array of records as a table, one column per key. Records with
    /// nothing in common collapse to a single `ID` column, which is what Odoo's
    /// `search` returns and what a person wants to see anyway.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout.
    pub fn print_rows(self, rows: &[Value]) -> std::io::Result<()> {
        if rows.is_empty() {
            return writeln!(std::io::stdout().lock(), "{}", self.dim("nothing to show"));
        }
        let mut titles: Vec<String> = Vec::new();
        for row in rows {
            if let Some(fields) = row.as_object() {
                for key in fields.keys() {
                    if !titles.iter().any(|title| title == key) {
                        titles.push(key.clone());
                    }
                }
            } else if titles.is_empty() {
                titles.push("VALUE".to_owned());
            }
        }
        let columns = titles
            .iter()
            .map(|title| {
                let numeric = rows
                    .iter()
                    .filter_map(|row| row.get(title))
                    .all(|value| value.is_number());
                let column = if numeric {
                    Column::number(title)
                } else {
                    Column::text(title)
                };
                if title == "name" || title == "body" {
                    column.flexible()
                } else {
                    column
                }
            })
            .collect();
        let mut table = Table::new(columns);
        for row in rows {
            table.push(
                titles
                    .iter()
                    .map(|title| row.get(title).map(inline).unwrap_or_else(|| "-".to_owned()))
                    .collect::<Vec<_>>(),
            );
        }
        self.print_table(&table)
    }

    /// Prints JSON, one line unless `--pretty`.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout.
    pub fn print_json(self, value: &Value) -> std::io::Result<()> {
        let text = if self.pretty {
            serde_json::to_string_pretty(value)
        } else {
            serde_json::to_string(value)
        }
        .unwrap_or_else(|error| format!("{{\"error\":\"could not render JSON: {error}\"}}"));
        writeln!(std::io::stdout().lock(), "{text}")
    }

    /// Prints a table.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout.
    pub fn print_table(self, table: &Table) -> std::io::Result<()> {
        write!(std::io::stdout().lock(), "{}", table.render(self))
    }

    /// Prints a pre-rendered block (tables, detail views) to stdout.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout, so a closed pipe is reported rather than
    /// panicked over by `println!`.
    pub fn show(self, text: impl AsRef<str>) -> std::io::Result<()> {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{}", text.as_ref())
    }

    /// The machine-readable result of a change, for `-o json`.
    ///
    /// A person is told what happened on stderr; a script gets the outcome on stdout,
    /// so `id=$(hodoo task create … | jq .id)` works in JSON mode and a human run is
    /// not polluted with a payload nobody reads.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout.
    pub fn result(self, value: &Value) -> std::io::Result<()> {
        if self.mode == Mode::Json {
            return self.print_json(value);
        }
        Ok(())
    }

    /// `{"id": N}`: something was created.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout.
    pub fn created(self, id: i64) -> std::io::Result<()> {
        self.result(&json!({ "id": id }))
    }

    /// `{"id": N, "ok": true}`: something was changed.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout.
    pub fn changed(self, id: i64) -> std::io::Result<()> {
        self.result(&json!({ "id": id, "ok": true }))
    }

    /// `{"deleted": N, "ok": true}`: something is gone.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stdout.
    pub fn removed(self, id: i64) -> std::io::Result<()> {
        self.result(&json!({ "deleted": id, "ok": true }))
    }

    /// A short confirmation, e.g. `created  task #31  Write the launch email`.
    /// Suppressed by `-q`.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stderr.
    pub fn note(self, text: &str) -> std::io::Result<()> {
        if self.quiet || self.mode == Mode::Json {
            return Ok(());
        }
        let mut stderr = std::io::stderr().lock();
        writeln!(stderr, "{}", self.dim(text))
    }

    /// A follow-up suggestion for a human, e.g. `run hodoo task show 31 to see it`.
    /// Never printed in JSON mode, and never for a quiet run.
    ///
    /// # Errors
    ///
    /// The I/O error of writing to stderr.
    pub fn hint(self, text: &str) -> std::io::Result<()> {
        if self.quiet || self.mode == Mode::Json {
            return Ok(());
        }
        let mut stderr = std::io::stderr().lock();
        writeln!(stderr, "{}", self.dim(&format!("hint: {text}")))
    }

    /// Paints text, and does nothing when colour is off.
    #[must_use]
    pub fn paint(self, text: &str, style: Style) -> String {
        if !self.color {
            return text.to_owned();
        }
        format!("{}{text}\x1b[0m", style.code())
    }

    /// Dim text, for hints and finished work.
    #[must_use]
    pub fn dim(self, text: &str) -> String {
        self.paint(text, Style::Dim)
    }

    /// Bold text, for table headers and titles.
    #[must_use]
    pub fn bold(self, text: &str) -> String {
        self.paint(text, Style::Bold)
    }

    /// The table layout width: the terminal's, or `None` when nothing is a terminal.
    #[must_use]
    pub fn width(self) -> Option<usize> {
        if self.width == 0 {
            None
        } else {
            Some(self.width)
        }
    }
}

/// A colour intensity for [`Output::paint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Bold.
    Bold,
    /// Faint.
    Dim,
    /// Red.
    Red,
    /// Yellow.
    Yellow,
    /// Green.
    Green,
}

impl Style {
    fn code(self) -> &'static str {
        match self {
            Style::Bold => "\x1b[1m",
            Style::Dim => "\x1b[2m",
            Style::Red => "\x1b[31m",
            Style::Yellow => "\x1b[33m",
            Style::Green => "\x1b[32m",
        }
    }
}

fn terminal_width(stdout_is_tty: bool) -> usize {
    if !stdout_is_tty {
        // A pipe or a file has no width to fit, and 0 tells the renderer to lay
        // the table out for its content rather than guess at a terminal.
        return 0;
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|columns| columns.trim().parse().ok())
        .unwrap_or(100)
}

/// One cell: text, and optionally its own colour.
#[derive(Debug, Clone)]
pub struct Cell {
    /// What the cell says.
    pub text: String,
    /// Colour for this cell alone, which wins over its column's and its row's.
    pub style: Option<Style>,
}

impl Cell {
    /// A plain cell.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
        }
    }

    /// A cell coloured only when a style is given.
    #[must_use]
    pub fn maybe(text: impl Into<String>, style: Option<Style>) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// How a column is aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Left, for text.
    Left,
    /// Right, for numbers and dates.
    Right,
}

/// One column of a [`Table`].
#[derive(Debug, Clone)]
pub struct Column {
    /// Header text.
    pub title: String,
    /// Alignment.
    pub align: Align,
    /// Colour to apply to every cell in this column, if any.
    pub style: Option<Style>,
    /// Whether this column may be truncated to fit the terminal.
    pub flexible: bool,
}

impl Column {
    /// A left-aligned text column.
    #[must_use]
    pub fn text(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            align: Align::Left,
            style: None,
            flexible: false,
        }
    }

    /// A right-aligned column, for numbers.
    #[must_use]
    pub fn number(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            align: Align::Right,
            style: None,
            flexible: false,
        }
    }

    /// Marks this column as the one that gives up width first.
    #[must_use]
    pub fn flexible(mut self) -> Self {
        self.flexible = true;
        self
    }
}

/// A table: headers, rows, and the rules for laying them out.
#[derive(Debug, Clone, Default)]
pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
    /// Per-row colour override, applied to cells whose column sets none.
    row_styles: Vec<Option<Style>>,
    title: Option<String>,
}

impl Table {
    /// A table with the given columns.
    #[must_use]
    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            row_styles: Vec::new(),
            title: None,
        }
    }

    /// Adds a row. Cells beyond the column count are ignored, missing cells are empty.
    pub fn push<I: IntoIterator<Item = String>>(&mut self, row: I) {
        self.push_styled(row, None);
    }

    /// Adds a row with a colour applied to cells that have none of their own.
    pub fn push_styled<I: IntoIterator<Item = String>>(&mut self, row: I, style: Option<Style>) {
        self.push_cells(row.into_iter().map(Cell::text), style);
    }

    /// Adds a row of cells, which may carry their own colours.
    pub fn push_cells<I: IntoIterator<Item = Cell>>(&mut self, row: I, style: Option<Style>) {
        let mut cells: Vec<Cell> = row.into_iter().collect();
        cells.truncate(self.columns.len());
        while cells.len() < self.columns.len() {
            cells.push(Cell::text(""));
        }
        self.rows.push(cells);
        self.row_styles.push(style);
    }

    /// Renders the table as text, honouring colour, headers and terminal width.
    #[must_use]
    pub fn render(&self, out: Output) -> String {
        if self.columns.is_empty() {
            return String::new();
        }
        let widths = self.widths(out);
        let mut text = String::new();
        if let Some(title) = &self.title {
            text.push_str(&out.bold(title));
            text.push_str("\n\n");
        }
        if out.headers {
            let headers: Vec<String> = self
                .columns
                .iter()
                .zip(&widths)
                .map(|(column, width)| pad(&column.title, *width, column.align))
                .collect();
            text.push_str(&out.bold(headers.join("  ").trim_end()));
            text.push('\n');
        }
        for (row, style) in self.rows.iter().zip(&self.row_styles) {
            let line = row
                .iter()
                .zip(&widths)
                .zip(&self.columns)
                .map(|((cell, width), column)| {
                    let padded = pad(&cell.text, *width, column.align);
                    match (cell.style, column.style, *style) {
                        (Some(style), _, _)
                        | (None, Some(style), _)
                        | (None, None, Some(style)) => out.paint(&padded, style),
                        (None, None, None) => padded,
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            text.push_str(line.trim_end());
            text.push('\n');
        }
        text
    }

    /// Column widths: content-driven, then squeezed into the terminal if needed.
    fn widths(&self, out: Output) -> Vec<usize> {
        let mut widths: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let header = column.title.chars().count();
                let cells = self
                    .rows
                    .iter()
                    .map(|row| row[index].text.chars().count())
                    .max()
                    .unwrap_or(0);
                header.max(cells)
            })
            .collect();

        let Some(limit) = out.width() else {
            return widths;
        };
        let gaps = 2 * (widths.len().saturating_sub(1));
        let free = limit.saturating_sub(gaps + 2 + widths.iter().sum::<usize>());
        if free > 0 {
            return widths;
        }
        // Give up the flexible columns' width before anything else, keeping enough
        // for a name to stay recognisable.
        let over = limit.saturating_sub(gaps + 2);
        let mut total: usize = widths.iter().sum();
        while total > over {
            let Some(index) = self
                .columns
                .iter()
                .enumerate()
                .filter(|(index, column)| column.flexible && widths[*index] > MIN_FLEXIBLE)
                .map(|(index, _)| index)
                .next()
            else {
                break;
            };
            widths[index] -= 1;
            total -= 1;
        }
        widths
    }
}

/// Columns never shrink below this, so a truncated name still means something.
const MIN_FLEXIBLE: usize = 20;

/// Pads or truncates a cell to `width`, keeping multi-byte text intact.
fn pad(text: &str, width: usize, align: Align) -> String {
    let text = truncate(text, width);
    let length = text.chars().count();
    let padding = width.saturating_sub(length);
    match align {
        Align::Left => format!("{text}{}", " ".repeat(padding)),
        Align::Right => format!("{}{text}", " ".repeat(padding)),
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let keep = width.saturating_sub(1);
    let mut cut: String = text.chars().take(keep).collect();
    cut.push('…');
    cut
}

/// One JSON value on one line, for a table cell.
fn inline(value: &Value) -> String {
    match value {
        Value::Null => "-".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(inline).collect::<Vec<_>>().join(", "),
        Value::Object(_) => value.to_string(),
    }
}

/// A human reading of a deadline: `2d ago`, `today`, `in 3d`, and the date once it is
/// far away. `None` means no deadline was set.
#[must_use]
pub fn human_due(deadline: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<(String, Style)> {
    let deadline = deadline?;
    let days = (deadline.date_naive() - now.date_naive()).num_days();
    let text = match days {
        d if d < -30 => deadline.format("%Y-%m-%d").to_string(),
        -1 => "yesterday".to_owned(),
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        d if d < 0 => format!("{}d ago", -d),
        d if d <= 30 => format!("in {d}d"),
        _ => deadline.format("%Y-%m-%d").to_string(),
    };
    let style = match days {
        d if d < 0 => Style::Red,
        0..=2 => Style::Yellow,
        _ => Style::Dim,
    };
    Some((text, style))
}

/// A date, or a dash when there is none.
#[must_use]
pub fn show_date(date: Option<NaiveDate>) -> String {
    date.map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "-".to_owned())
}

/// Everything a person needs to see a record's description, without HTML in the way.
#[must_use]
pub fn plain_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for character in html.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn output(color: bool, width: usize) -> Output {
        Output {
            mode: Mode::Table,
            color,
            pretty: false,
            headers: true,
            quiet: false,
            width,
        }
    }

    #[test]
    fn a_table_aligns_columns_and_pads_numbers_to_the_right() {
        let mut table = Table::new(vec![Column::number("ID"), Column::text("NAME")]);
        table.push(["31".to_owned(), "Write the copy".to_owned()]);
        table.push(["7".to_owned(), "Short".to_owned()]);

        assert_eq!(
            table.render(output(false, 0)),
            "ID  NAME\n31  Write the copy\n 7  Short\n"
        );
    }

    #[test]
    fn headers_can_be_turned_off_for_awk() {
        let mut table = Table::new(vec![Column::text("ID")]);
        table.push(["31".to_owned()]);
        let mut quiet = output(false, 0);
        quiet.headers = false;
        assert_eq!(table.render(quiet), "31\n");
    }

    #[test]
    fn colour_wraps_only_the_padded_cells() {
        let mut table = Table::new(vec![Column::text("NAME")]);
        table.push_cells([Cell::maybe("x", Some(Style::Red))], None);
        assert_eq!(
            table.render(output(true, 0)),
            "\x1b[1mNAME\x1b[0m\n\x1b[31mx   \x1b[0m\n"
        );
        assert_eq!(table.render(output(false, 0)), "NAME\nx\n");
    }

    #[test]
    fn a_flexible_column_is_the_one_that_gives_up_width() {
        let mut table = Table::new(vec![
            Column::number("ID"),
            Column::text("NAME").flexible(),
            Column::text("STAGE"),
        ]);
        table.push([
            "31".to_owned(),
            "A name that is far too long for the terminal to show in full".to_owned(),
            "Design".to_owned(),
        ]);
        let rendered = table.render(output(true, 40));
        for line in rendered.lines() {
            assert!(visible(line).chars().count() <= 40, "line too wide: {line}");
        }
        assert!(rendered.contains('…'), "the flexible column was truncated");
        assert!(rendered.contains("Design"), "the other columns survived");
    }

    /// What a terminal counts: the printed characters, not the colour codes.
    fn visible(line: &str) -> String {
        let mut out = String::new();
        let mut in_escape = false;
        for character in line.chars() {
            match character {
                '\u{1b}' => in_escape = true,
                'm' if in_escape => in_escape = false,
                _ if !in_escape => out.push(character),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn something_readable_sits_in_the_right() {
        let now = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let at = |days: i64| {
            Some(
                (now + chrono::Duration::days(days))
                    .date_naive()
                    .and_hms_opt(9, 0, 0)
                    .unwrap()
                    .and_utc(),
            )
        };

        assert_eq!(human_due(at(0), now).unwrap().0, "today");
        assert_eq!(human_due(at(1), now).unwrap().0, "tomorrow");
        assert_eq!(human_due(at(-1), now).unwrap().0, "yesterday");
        assert_eq!(human_due(at(-3), now).unwrap().0, "3d ago");
        assert_eq!(human_due(at(9), now).unwrap().0, "in 9d");
        assert_eq!(human_due(None, now), None);
        assert_eq!(human_due(at(-3), now).unwrap().1, Style::Red);
        assert_eq!(human_due(at(1), now).unwrap().1, Style::Yellow);
    }

    #[test]
    fn html_becomes_one_line_of_text() {
        assert_eq!(
            plain_text("<p>Hero, navigation,<br/> footer.</p>"),
            "Hero, navigation, footer."
        );
    }

    #[test]
    fn the_mode_comes_from_the_flag_then_the_environment() {
        assert_eq!(Mode::parse("json"), Some(Mode::Json));
        assert_eq!(Mode::parse("TABLE"), Some(Mode::Table));
        assert_eq!(Mode::parse("yaml"), None);
    }
}
