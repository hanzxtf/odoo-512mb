//! A `.env` file, for the credentials a shell does not export.
//!
//! The environment always wins: a value set in the process environment (and
//! therefore a command-line flag, or the shell's `export`) is never overridden
//! by the file. [`resolve`] is the layering, in one place.
//!
//! Nothing here mutates the process environment: `std::env::set_var` is unsafe
//! in edition 2024 and this crate forbids unsafe code, so a file is read into a
//! map and consulted explicitly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How far up from the starting directory to look for a `.env`.
const MAX_DEPTH: usize = 4;

/// The variables this crate knows about.
pub const KEYS: [&str; 3] = ["ODOO_URL", "ODOO_API_KEY", "ODOO_DB"];

/// The value for `key`: the process environment first, then the file.
#[must_use]
pub fn resolve(key: &str, file: &BTreeMap<String, String>) -> Option<String> {
    std::env::var(key).ok().or_else(|| file.get(key).cloned())
}

/// Parses `.env` text.
///
/// Understands `KEY=value`, an optional `export ` prefix, `#` comments, blank
/// lines, and a value wrapped in single or double quotes. Anything else is
/// ignored rather than guessed at: a malformed line is not worth failing a
/// command over.
#[must_use]
pub fn parse(text: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        values.insert(key.to_owned(), unquote(value.trim()));
    }
    values
}

/// Reads a `.env` file.
///
/// # Errors
///
/// Any [`std::io::Error`] from reading the file.
pub fn read(path: impl AsRef<Path>) -> std::io::Result<BTreeMap<String, String>> {
    std::fs::read_to_string(path).map(|text| parse(&text))
}

/// The nearest `.env` at or above `start`, up to `MAX_DEPTH` levels.
#[must_use]
pub fn find(start: impl AsRef<Path>) -> Option<PathBuf> {
    let start = start.as_ref();
    let mut directory = Some(start);
    for _ in 0..=MAX_DEPTH {
        let current = directory?;
        let candidate = current.join(".env");
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = current.parent();
    }
    None
}

/// Reads the nearest `.env`, if there is one.
///
/// # Errors
///
/// Any [`std::io::Error`] from reading a file that was found.
pub fn load(start: impl AsRef<Path>) -> std::io::Result<Option<BTreeMap<String, String>>> {
    match find(start) {
        Some(path) => read(path).map(Some),
        None => Ok(None),
    }
}

fn unquote(value: &str) -> String {
    // Only a matching opening and closing quote is a delimiter; a value that
    // merely contains or starts with a quote keeps it, rather than losing a
    // character to a half-applied rule.
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return value[1..value.len() - 1].to_owned();
    }
    value.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_yields_its_keys_and_ignores_the_rest() {
        let parsed = parse(
            "# a comment\n\
             \n\
             ODOO_URL=https://odoo.example.com\n\
             export ODOO_API_KEY=\"secret value\"\n\
             ODOO_DB='odoo'\n\
             not a variable\n\
             =novalue\n",
        );
        assert_eq!(
            parsed.get("ODOO_URL").map(String::as_str),
            Some("https://odoo.example.com")
        );
        assert_eq!(
            parsed.get("ODOO_API_KEY").map(String::as_str),
            Some("secret value")
        );
        assert_eq!(parsed.get("ODOO_DB").map(String::as_str), Some("odoo"));
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn the_process_environment_wins_over_the_file() {
        // `PATH` is always set, so it proves precedence without mutating the
        // environment (which needs `unsafe` in edition 2024, and this crate
        // forbids unsafe code).
        let file = BTreeMap::from([("PATH".to_owned(), "from the file".to_owned())]);
        let resolved = resolve("PATH", &file).expect("PATH");
        assert_eq!(Some(resolved), std::env::var("PATH").ok());

        // A key only the file carries is taken from the file.
        let file = BTreeMap::from([("HODOO_NOT_IN_ENV".to_owned(), "from the file".to_owned())]);
        assert_eq!(
            resolve("HODOO_NOT_IN_ENV", &file).as_deref(),
            Some("from the file")
        );
        assert_eq!(resolve("HODOO_NOT_ANYWHERE", &file), None);
    }

    #[test]
    fn the_nearest_file_upwards_is_the_one_found() {
        let root = std::env::temp_dir().join(format!("hodoo-dotenv-{}", std::process::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("temp dirs");
        std::fs::write(root.join(".env"), "ODOO_URL=from-the-root\n").expect("write");

        let found = find(&nested).expect("found");
        assert_eq!(found, root.join(".env"));
        assert_eq!(
            read(&found)
                .expect("read")
                .get("ODOO_URL")
                .map(String::as_str),
            Some("from-the-root")
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
