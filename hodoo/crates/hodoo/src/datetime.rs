//! Odoo's date and time formats.
//!
//! Odoo stores datetimes as naive UTC and sends them without an offset
//! (`2026-09-28 14:30:00`), and dates as `2026-09-28`. This module is the only
//! place that knows those formats: fields deserialize to
//! [`DateTime<Utc>`](chrono::DateTime) and [`NaiveDate`] and serialize back the
//! same way, so a caller never has to guess a timezone.

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde::Deserialize;
use serde::de::{Deserializer, Error as _};
use serde::ser::Serializer;
use serde_json::Value;

use crate::error::{Error, Result};

const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Parses what Odoo sends or accepts for a datetime field: naive UTC
/// (`2026-09-28 14:30:00`), an RFC 3339 stamp, or a bare date (midnight UTC).
///
/// # Errors
///
/// Returns [`Error::Config`] when the text is not one of those forms.
pub fn parse_datetime(text: &str) -> Result<DateTime<Utc>> {
    let text = text.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(parsed.with_timezone(&Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(text, DATETIME_FORMAT) {
        return Ok(naive.and_utc());
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, DATE_FORMAT) {
        return Ok(date.and_time(NaiveTime::MIN).and_utc());
    }
    Err(Error::Config {
        message: format!(
            "{text:?} is not a date or datetime; expected \"YYYY-MM-DD\" or \
             \"YYYY-MM-DD HH:MM:SS\" (Odoo datetimes are UTC)"
        ),
    })
}

/// Parses what Odoo sends or accepts for a date field, taking the date part of
/// a datetime when that is what it handed back.
///
/// # Errors
///
/// Returns [`Error::Config`] when the text is not a date or datetime.
pub fn parse_date(text: &str) -> Result<NaiveDate> {
    let text = text.trim();
    if let Ok(date) = NaiveDate::parse_from_str(text, DATE_FORMAT) {
        return Ok(date);
    }
    parse_datetime(text).map(|datetime| datetime.date_naive())
}

/// Formats a datetime the way Odoo expects to receive it.
#[must_use]
pub fn format_datetime(value: DateTime<Utc>) -> String {
    value.naive_utc().format(DATETIME_FORMAT).to_string()
}

/// Formats a date the way Odoo expects to receive it.
#[must_use]
pub fn format_date(value: NaiveDate) -> String {
    value.format(DATE_FORMAT).to_string()
}

/// `deserialize_with` for `Option<DateTime<Utc>>`.
pub fn opt_datetime<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<DateTime<Utc>>, D::Error> {
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Bool(false)) => Ok(None),
        Some(Value::String(text)) => parse_datetime(&text).map(Some).map_err(D::Error::custom),
        Some(other) => Err(D::Error::custom(format!(
            "expected a datetime string, got {other}"
        ))),
    }
}

/// `serialize_with` for `Option<DateTime<Utc>>`.
pub fn ser_opt_datetime<S: Serializer>(
    value: &Option<DateTime<Utc>>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    match value {
        Some(datetime) => serializer.serialize_str(&format_datetime(*datetime)),
        None => serializer.serialize_none(),
    }
}

/// `deserialize_with` for `Option<NaiveDate>`.
pub fn opt_date<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<NaiveDate>, D::Error> {
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Bool(false)) => Ok(None),
        Some(Value::String(text)) => parse_date(&text).map(Some).map_err(D::Error::custom),
        Some(other) => Err(D::Error::custom(format!(
            "expected a date string, got {other}"
        ))),
    }
}

/// `serialize_with` for `Option<NaiveDate>`.
pub fn ser_opt_date<S: Serializer>(
    value: &Option<NaiveDate>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    match value {
        Some(date) => serializer.serialize_str(&format_date(*date)),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetimes_round_trip_through_odoos_naive_utc_format() {
        let parsed = parse_datetime("2026-09-28 14:30:00").expect("naive utc");
        assert_eq!(format_datetime(parsed), "2026-09-28 14:30:00");
    }

    #[test]
    fn an_rfc3339_stamp_is_understood_and_normalised() {
        let parsed = parse_datetime("2026-09-28T16:30:00+02:00").expect("offset");
        assert_eq!(format_datetime(parsed), "2026-09-28 14:30:00");
    }

    #[test]
    fn a_bare_date_is_midnight_utc() {
        assert_eq!(
            format_datetime(parse_datetime("2026-09-28").expect("date")),
            "2026-09-28 00:00:00"
        );
        assert_eq!(
            format_date(parse_date("2026-09-28 14:30:00").expect("datetime as date")),
            "2026-09-28"
        );
    }

    #[test]
    fn nonsense_is_a_config_error_with_the_expected_format_in_it() {
        let error = parse_datetime("yesterday").expect_err("not a date");
        assert!(matches!(error, Error::Config { .. }));
        assert!(error.to_string().contains("YYYY-MM-DD"));
    }
}
