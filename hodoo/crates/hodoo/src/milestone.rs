//! `project.milestone`.
//!
//! Milestones only exist on a project whose `allow_milestones` is set, which
//! Odoo turns on when the project settings ask for them.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::Client;
use crate::datetime;
use crate::de;
use crate::error::{Error, Result};
use crate::id::{MilestoneId, ProjectId};
use crate::query;

/// The fields this crate reads from `project.milestone`.
pub const FIELDS: &[&str] = &[
    "id",
    "name",
    "project_id",
    "deadline",
    "is_reached",
    "reached_date",
    "task_count",
];

/// A milestone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Milestone {
    /// `project.milestone.id`.
    pub id: MilestoneId,
    /// Milestone name.
    #[serde(default)]
    pub name: String,
    /// The project it belongs to (`project_id`).
    #[serde(default, rename = "project_id", deserialize_with = "de::opt_id")]
    pub project: Option<ProjectId>,
    /// Deadline (`deadline`), a date.
    #[serde(
        default,
        deserialize_with = "datetime::opt_date",
        serialize_with = "datetime::ser_opt_date"
    )]
    pub deadline: Option<NaiveDate>,
    /// Whether it has been reached (`is_reached`).
    #[serde(default, rename = "is_reached")]
    pub is_reached: bool,
    /// When it was reached (`reached_date`), set by Odoo.
    #[serde(
        default,
        rename = "reached_date",
        deserialize_with = "datetime::opt_date",
        serialize_with = "datetime::ser_opt_date"
    )]
    pub reached_date: Option<NaiveDate>,
    /// How many tasks the milestone covers (`task_count`).
    #[serde(default, rename = "task_count")]
    pub task_count: i64,
}

/// The milestone fields a caller may write.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MilestoneFields {
    /// Milestone name. Required by Odoo on create.
    pub name: Option<String>,
    /// The project. Required by Odoo on create.
    pub project: Option<ProjectId>,
    /// Deadline (`deadline`), a date.
    pub deadline: Option<NaiveDate>,
    /// Sort order within the project.
    pub sequence: Option<i64>,
}

impl MilestoneFields {
    /// The fields Odoo requires to create a milestone.
    #[must_use]
    pub fn new(project: ProjectId, name: impl Into<String>) -> Self {
        Self {
            project: Some(project),
            name: Some(name.into()),
            ..Self::default()
        }
    }

    /// Renders the fields as Odoo `vals`.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut vals = Map::new();
        if let Some(name) = &self.name {
            vals.insert("name".into(), json!(name));
        }
        if let Some(project) = self.project {
            vals.insert("project_id".into(), json!(project.get()));
        }
        if let Some(deadline) = self.deadline {
            vals.insert("deadline".into(), json!(datetime::format_date(deadline)));
        }
        if let Some(sequence) = self.sequence {
            vals.insert("sequence".into(), json!(sequence));
        }
        Value::Object(vals)
    }
}

impl From<MilestoneFields> for Value {
    fn from(fields: MilestoneFields) -> Self {
        fields.to_value()
    }
}

/// Access to `project.milestone`.
#[derive(Debug, Clone, Copy)]
pub struct Milestones<'a> {
    client: &'a Client,
}

impl<'a> Milestones<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists a project's milestones, in Odoo's order (`sequence, id`).
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if a record does
    /// not match [`Milestone`].
    pub async fn list(&self, project: ProjectId) -> Result<Vec<Milestone>> {
        let domain = query::domain([json!(["project_id", "=", project.get()])]);
        self.client
            .call_as(
                "project.milestone",
                "search_read",
                query::search(domain, FIELDS, None, None, None),
            )
            .await
    }

    /// Creates a milestone.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn create(&self, fields: MilestoneFields) -> Result<MilestoneId> {
        let created = self
            .client
            .call("project.milestone", "create", query::create(fields.into()))
            .await?;
        query::created_id(&created)
            .map(MilestoneId::new)
            .ok_or_else(|| Error::UnexpectedResponse {
                status: 200,
                body: Error::truncate(created.to_string()),
            })
    }

    /// Marks a milestone reached, or unmarks it.
    ///
    /// Odoo's `toggle_is_reached` is the method the UI calls; writing
    /// `is_reached` directly would skip the `reached_date` bookkeeping.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn set_reached(&self, id: MilestoneId, reached: bool) -> Result<()> {
        let payload = json!({ "ids": [id.get()], "is_reached": reached });
        self.client
            .call("project.milestone", "toggle_is_reached", payload)
            .await
            .map(|_| ())
    }

    /// Deletes a milestone.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn delete(&self, id: MilestoneId) -> Result<()> {
        self.client
            .call("project.milestone", "unlink", query::unlink(id.get()))
            .await
            .map(|_| ())
    }
}
