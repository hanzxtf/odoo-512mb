//! Deserialization of Odoo's irregular JSON shapes.
//!
//! Odoo is not consistent about how it spells a relational value: a many2one
//! can arrive as `null`, `false`, a bare number, or `[id, "Display Name"]`, and
//! an x2many as a list of numbers or a list of pairs. Rather than repeat that
//! tolerance in every struct, it lives here as two `deserialize_with` helpers.

use serde::Deserialize;
use serde::de::{Deserializer, Error as _};
use serde_json::Value;

use crate::id::Id;

/// Pulls an id out of any of the shapes Odoo uses for a relational value.
pub(crate) fn id_from_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        // A many2one read as a pair, `[id, display_name]`. Nested arrays are
        // tolerated too: only the first element matters.
        Value::Array(items) => items.first().and_then(id_from_value),
        // Rare, but `read_group` and a few overridden read methods hand back
        // numbers as strings.
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// `deserialize_with` for a many2one whose id is not worth tagging with a type.
pub fn opt_raw_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Bool(false)) => Ok(None),
        Some(value) => id_from_value(&value)
            .map(Some)
            .ok_or_else(|| D::Error::custom(format!("expected an id or [id, name], got {value}"))),
    }
}

/// Deserializes an optional text field, where Odoo's `false` means "empty".
///
/// Odoo sends `false` rather than `null` for an unset `Char`, `Text` or `Html`
/// field, so a plain `Option<String>` would fail on it.
pub fn opt_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Bool(false)) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(other) => Err(D::Error::custom(format!("expected a string, got {other}"))),
    }
}

/// `serde(default = ...)` for a boolean field whose absence means "yes", such
/// as `active`: reporting a missing flag as `false` would call a live record
/// archived.
pub fn default_true() -> bool {
    true
}

/// Deserializes a many2one into `Option<Id<T>>`.
///
/// Use as `#[serde(default, deserialize_with = "crate::de::opt_id")]`; the tag
/// `T` is inferred from the field's type.
pub fn opt_id<'de, D, T>(deserializer: D) -> Result<Option<Id<T>>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Bool(false)) => Ok(None),
        Some(value) => id_from_value(&value)
            .map(|raw| Some(Id::new(raw)))
            .ok_or_else(|| D::Error::custom(format!("expected an id or [id, name], got {value}"))),
    }
}

/// Deserializes an x2many into `Vec<Id<T>>`. Missing, `null` and `false` all
/// mean the empty list.
pub fn id_list<'de, D, T>(deserializer: D) -> Result<Vec<Id<T>>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Bool(false)) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                id_from_value(item).map(Id::new).ok_or_else(|| {
                    D::Error::custom(format!("expected a list of ids, got {item} in {items:?}"))
                })
            })
            .collect(),
        Some(single) => id_from_value(&single)
            .map(|raw| vec![Id::new(raw)])
            .ok_or_else(|| D::Error::custom(format!("expected a list of ids, got {single}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ProjectId;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(default, deserialize_with = "opt_id")]
        many: Option<ProjectId>,
        #[serde(default, deserialize_with = "id_list")]
        many_many: Vec<ProjectId>,
    }

    fn parse(json: &str) -> Holder {
        serde_json::from_str(json).expect("holder")
    }

    #[test]
    fn a_many2one_accepts_every_shape_odoo_uses() {
        assert_eq!(parse(r#"{"many": null}"#).many, None);
        assert_eq!(parse(r#"{"many": false}"#).many, None);
        assert_eq!(parse("{}").many, None);
        assert_eq!(parse(r#"{"many": 7}"#).many, Some(Id::new(7)));
        assert_eq!(parse(r#"{"many": "7"}"#).many, Some(Id::new(7)));
        assert_eq!(parse(r#"{"many": [7, "Acme"]}"#).many, Some(Id::new(7)));
    }

    #[test]
    fn an_x2many_accepts_numbers_and_pairs() {
        assert!(parse(r#"{"many_many": []}"#).many_many.is_empty());
        assert!(parse(r#"{"many_many": false}"#).many_many.is_empty());
        assert!(parse("{}").many_many.is_empty());
        assert_eq!(parse(r#"{"many_many": [1, 2]}"#).many_many.len(), 2);
        assert_eq!(
            parse(r#"{"many_many": [[1, "a"], [2, "b"]]}"#).many_many,
            vec![Id::new(1), Id::new(2)]
        );
        // A single id where a list was expected is tolerated rather than fatal.
        assert_eq!(parse(r#"{"many_many": 3}"#).many_many, vec![Id::new(3)]);
    }

    #[test]
    fn a_nonsense_relational_value_is_an_error_not_a_silent_none() {
        assert!(serde_json::from_str::<Holder>(r#"{"many": "abc"}"#).is_err());
        assert!(serde_json::from_str::<Holder>(r#"{"many_many": ["abc"]}"#).is_err());
    }
}
