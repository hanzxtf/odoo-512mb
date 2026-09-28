//! `project.task.type`: the stages a task moves through.
//!
//! A stage is shared between projects through `project_ids`. A task's stage must
//! be one of its project's stages, so a stage has to be attached to a project
//! before any task in that project can sit in it.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::Client;
use crate::de;
use crate::error::{Error, Result};
use crate::id::{ProjectId, TaskStageId};
use crate::query;

/// The fields this crate reads from `project.task.type`.
pub const FIELDS: &[&str] = &["id", "name", "sequence", "fold", "color", "active"];

/// A task stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStage {
    /// `project.task.type.id`.
    pub id: TaskStageId,
    /// Stage name.
    #[serde(default)]
    pub name: String,
    /// Sort order within the kanban.
    #[serde(default)]
    pub sequence: i64,
    /// Whether the stage is folded in the kanban.
    #[serde(default)]
    pub fold: bool,
    /// Colour index.
    #[serde(default)]
    pub color: i64,
    /// `false` once the stage is archived.
    #[serde(default = "de::default_true")]
    pub active: bool,
}

/// A stage filter.
#[derive(Debug, Clone, Default)]
pub struct StageFilter {
    /// Only stages attached to this project.
    pub project: Option<ProjectId>,
    /// Odoo's `order` string. Odoo's default is `sequence, id`.
    pub order: Option<String>,
    /// Maximum records to return.
    pub limit: Option<u32>,
    /// Records to skip.
    pub offset: Option<u32>,
}

impl StageFilter {
    fn domain(&self) -> Value {
        query::domain([self
            .project
            .map_or(Value::Null, |id| json!(["project_ids", "in", [id.get()]]))])
    }
}

/// The stage fields a caller may write.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StageFields {
    /// Stage name. Required by Odoo on create.
    pub name: Option<String>,
    /// Sort order within the kanban.
    pub sequence: Option<i64>,
    /// Whether the stage is folded in the kanban. The last stage of a project
    /// is usually folded.
    pub fold: Option<bool>,
    /// Colour index.
    pub color: Option<i64>,
}

impl StageFields {
    /// The fields Odoo requires to create a stage.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
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
        if let Some(sequence) = self.sequence {
            vals.insert("sequence".into(), json!(sequence));
        }
        if let Some(fold) = self.fold {
            vals.insert("fold".into(), json!(fold));
        }
        if let Some(color) = self.color {
            vals.insert("color".into(), json!(color));
        }
        Value::Object(vals)
    }
}

impl From<StageFields> for Value {
    fn from(fields: StageFields) -> Self {
        fields.to_value()
    }
}

/// Access to `project.task.type`.
#[derive(Debug, Clone, Copy)]
pub struct Stages<'a> {
    client: &'a Client,
}

impl<'a> Stages<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists stages.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if a record does
    /// not match [`TaskStage`].
    pub async fn list(&self, filter: StageFilter) -> Result<Vec<TaskStage>> {
        self.client
            .call_as(
                "project.task.type",
                "search_read",
                query::search(
                    filter.domain(),
                    FIELDS,
                    filter.limit,
                    filter.offset,
                    filter.order.as_deref(),
                ),
            )
            .await
    }

    /// Creates a stage, unattached. Use
    /// [`create_in`](Stages::create_in) unless you are sharing one stage with
    /// several projects, because a stage no project uses cannot hold a task.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn create(&self, fields: StageFields) -> Result<TaskStageId> {
        let created = self
            .client
            .call("project.task.type", "create", query::create(fields.into()))
            .await?;
        query::created_id(&created)
            .map(TaskStageId::new)
            .ok_or_else(|| Error::UnexpectedResponse {
                status: 200,
                body: Error::truncate(created.to_string()),
            })
    }

    /// Creates a stage and attaches it to a project, which is what every
    /// "add a stage" in the Odoo UI does.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`]. If the attach fails the created stage is
    /// left attached to nothing.
    pub async fn create_in(&self, project: ProjectId, fields: StageFields) -> Result<TaskStageId> {
        let stage = self.create(fields).await?;
        self.client
            .projects()
            .attach_stages(project, &[stage])
            .await?;
        Ok(stage)
    }
}
