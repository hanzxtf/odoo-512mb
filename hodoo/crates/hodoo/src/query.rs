//! Body builders for the handful of ORM methods this crate calls.
//!
//! JSON-2 takes named arguments at the top level of the request body, so these
//! functions exist to keep that shape in one place instead of in every scope
//! module.

use serde_json::{Map, Value, json};

/// Builds a `search_read` (or `search`) body.
///
/// `fields` is always sent: Odoo returns *every* field when it is omitted,
/// computed columns included, which is a lot of bytes to throw away.
pub(crate) fn search(
    domain: Value,
    fields: &[&str],
    limit: Option<u32>,
    offset: Option<u32>,
    order: Option<&str>,
) -> Value {
    let mut body = Map::new();
    body.insert("domain".into(), domain);
    body.insert("fields".into(), json!(fields));
    if let Some(limit_) = limit {
        body.insert("limit".into(), json!(limit_));
    }
    if let Some(offset_) = offset {
        body.insert("offset".into(), json!(offset_));
    }
    if let Some(order_) = order {
        body.insert("order".into(), json!(order_));
    }
    Value::Object(body)
}

/// Builds a `search_count` body.
pub(crate) fn count(domain: Value) -> Value {
    json!({ "domain": domain })
}

/// Builds a `create` body. `create` takes `vals_list` and accepts a single
/// element for a single record.
pub(crate) fn create(vals: Value) -> Value {
    json!({ "vals_list": [vals] })
}

/// Builds a `write` body.
pub(crate) fn write(raw_id: i64, vals: Value) -> Value {
    json!({ "ids": [raw_id], "vals": vals })
}

/// Builds an `unlink` body.
pub(crate) fn unlink(raw_id: i64) -> Value {
    json!({ "ids": [raw_id] })
}

/// Builds a `read` body.
pub(crate) fn read(raw_id: i64, fields: &[&str]) -> Value {
    json!({ "ids": [raw_id], "fields": fields })
}

/// A domain, with the unset parts dropped.
///
/// Filters build one candidate per field and push `Value::Null` for the ones
/// they do not set; dropping them here keeps that out of every filter.
pub(crate) fn domain(parts: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(parts.into_iter().filter(|part| !part.is_null()).collect())
}

/// Odoo's "replace the whole collection" command, `[[6, 0, ids]]`.
///
/// Used for every x2many this crate writes, so `Some(vec![])` clears a field
/// and `Some(vec![1, 2])` replaces it, without three-way merge surprises.
pub(crate) fn replace<I: IntoIterator<Item = i64>>(ids: I) -> Value {
    json!([[6, 0, ids.into_iter().collect::<Vec<_>>()]])
}

/// The id in a `create` response.
///
/// JSON-2 reduces a returned recordset to its ids, so `create` answers `[7]`,
/// not `7`.
pub(crate) fn created_id(value: &Value) -> Option<i64> {
    crate::de::id_from_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_omits_what_the_caller_left_unset() {
        let body = search(json!([["id", "=", 1]]), &["id", "name"], None, None, None);
        assert_eq!(
            body,
            json!({ "domain": [["id", "=", 1]], "fields": ["id", "name"] })
        );

        let body = search(json!([]), &["id"], Some(10), Some(20), Some("id desc"));
        assert_eq!(
            body,
            json!({
                "domain": [],
                "fields": ["id"],
                "limit": 10,
                "offset": 20,
                "order": "id desc"
            })
        );
    }

    #[test]
    fn a_domain_drops_the_unset_parts() {
        let built = domain([json!(["project_id", "=", 3]), Value::Null, json!(null)]);
        assert_eq!(built, json!([["project_id", "=", 3]]));
    }

    #[test]
    fn collections_are_replaced_outright() {
        assert_eq!(replace([1, 2]), json!([[6, 0, [1, 2]]]));
        assert_eq!(replace(Vec::<i64>::new()), json!([[6, 0, []]]));
    }

    #[test]
    fn create_answers_a_list_of_ids() {
        assert_eq!(created_id(&json!([7])), Some(7));
        assert_eq!(created_id(&json!(7)), Some(7));
        assert_eq!(created_id(&json!([])), None);
    }
}
