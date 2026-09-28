//! Typed identifiers.
//!
//! Odoo ids are plain integers, so nothing stops a task id from being passed
//! where a project id belongs. [`Id`] tags an id with the entity it identifies,
//! and that mix-up becomes a compile error.
//!
//! The tag is any type: this crate uses its own model structs ([`Project`],
//! [`Task`]) and, for the models it does not model, an empty enum
//! ([`User`], [`Partner`], [`ProjectStage`]).
//!
//! [`Project`]: crate::Project
//! [`Task`]: crate::Task

use std::any::type_name;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

use serde::de::{Deserialize, Deserializer, Error as _};
use serde::{Serialize, Serializer};

use crate::de;

/// An Odoo database id, tagged with the entity it belongs to.
///
/// `Id<T>` is `Copy` and cheap; the phantom tag is a function pointer so the id
/// stays `Send` and `Sync` whatever `T` is.
pub struct Id<T>(i64, PhantomData<fn() -> T>);

impl<T> Id<T> {
    /// Wraps a raw Odoo id.
    #[must_use]
    pub const fn new(raw: i64) -> Self {
        Self(raw, PhantomData)
    }

    /// The raw id, as Odoo uses it.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Id<T> {}

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The tag's full path only adds noise: `Project(7)` reads better than
        // `hodoo::project::Project(7)`.
        let tag = type_name::<T>().rsplit("::").next().unwrap_or("Id");
        write!(f, "{tag}({})", self.0)
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<T> From<i64> for Id<T> {
    fn from(raw: i64) -> Self {
        Self::new(raw)
    }
}

impl<T> From<Id<T>> for i64 {
    fn from(id: Id<T>) -> Self {
        id.get()
    }
}

impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Odoo answers with a bare number, or with `[id, "Display Name"]` when
        // the field is read as a pair. Both mean the same id.
        let value = serde_json::Value::deserialize(deserializer)?;
        de::id_from_value(&value)
            .map(Self::new)
            .ok_or_else(|| D::Error::custom(format!("expected an id, got {value}")))
    }
}

/// Tag for `res.users` ids. This crate does not model the user record.
pub enum User {}

/// Tag for `res.partner` ids. This crate does not model the partner record.
pub enum Partner {}

/// Tag for `project.project.stage` ids, the stages a *project* moves through.
///
/// Not to be confused with [`TaskStage`](crate::TaskStage): a task's stage is a
/// different model entirely.
pub enum ProjectStage {}

/// `project.project` id.
pub type ProjectId = Id<crate::Project>;
/// `project.project.stage` id.
pub type ProjectStageId = Id<ProjectStage>;
/// `project.task` id.
pub type TaskId = Id<crate::Task>;
/// `project.task.type` id, the stage a task sits in.
pub type TaskStageId = Id<crate::TaskStage>;
/// `project.milestone` id.
pub type MilestoneId = Id<crate::Milestone>;
/// `project.tags` id.
pub type TagId = Id<crate::Tag>;
/// `res.users` id.
pub type UserId = Id<User>;
/// `res.partner` id.
pub type PartnerId = Id<Partner>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_ordered_by_raw_value_and_print_their_tag() {
        let low: ProjectId = Id::new(3);
        let high: ProjectId = Id::new(9);
        assert!(low < high);
        assert_eq!(low.to_string(), "3");
        assert_eq!(format!("{low:?}"), "Project(3)");
        assert_eq!(i64::from(low), 3);
    }

    #[test]
    fn an_id_deserializes_from_a_number_or_a_pair() {
        let bare: ProjectId = serde_json::from_str("12").expect("bare id");
        let pair: ProjectId = serde_json::from_str(r#"[12, "Acme"]"#).expect("pair");
        assert_eq!(bare, pair);
        assert!(serde_json::from_str::<ProjectId>(r#""12""#).is_ok());
        assert!(serde_json::from_str::<ProjectId>("null").is_err());
    }

    #[test]
    fn an_id_serializes_as_a_bare_number() {
        let id: TaskId = Id::new(7);
        assert_eq!(serde_json::to_string(&id).expect("serialize"), "7");
    }
}
