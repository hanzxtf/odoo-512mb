//! `project.project.stage`: the stages a *project* moves through.
//!
//! Not to be confused with [`project.task.type`](crate::stage), a task's stage. A
//! project stage is global -- no project owns it, and a project points at exactly one
//! through `stage_id` -- so Odoo ships a handful (`To Do`, `In Progress`, `Done`,
//! `Cancelled`) and a team rarely adds another. The fields a caller writes are the same
//! as a task stage's, so [`StageFields`](crate::StageFields) serves both.

use serde::{Deserialize, Serialize};

use crate::Client;
use crate::de;
use crate::error::{Error, Result};
use crate::id::ProjectStageId;
use crate::query;
use crate::stage::StageFields;

/// The fields this crate reads from `project.project.stage`.
pub const FIELDS: &[&str] = &["id", "name", "sequence", "fold", "color", "active"];

/// A project stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectStage {
    /// `project.project.stage.id`.
    pub id: ProjectStageId,
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

/// A project stage filter.
#[derive(Debug, Clone, Default)]
pub struct ProjectStageFilter {
    /// Odoo's `order` string. Odoo's default is `sequence, id`.
    pub order: Option<String>,
    /// Maximum records to return.
    pub limit: Option<u32>,
    /// Records to skip.
    pub offset: Option<u32>,
}

/// Access to `project.project.stage`.
#[derive(Debug, Clone, Copy)]
pub struct ProjectStages<'a> {
    client: &'a Client,
}

impl<'a> ProjectStages<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists the project stages. There is nothing to filter on: a project stage
    /// exists once and every project may use it.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if a record does not
    /// match [`ProjectStage`].
    pub async fn list(&self, filter: ProjectStageFilter) -> Result<Vec<ProjectStage>> {
        self.client
            .call_as(
                "project.project.stage",
                "search_read",
                query::search(
                    query::domain([]),
                    FIELDS,
                    filter.limit,
                    filter.offset,
                    filter.order.as_deref(),
                ),
            )
            .await
    }

    /// Creates a project stage. Every project can use it immediately: nothing is
    /// attached and no project points at it until a write sets `stage_id`.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn create(&self, fields: StageFields) -> Result<ProjectStageId> {
        let created = self
            .client
            .call(
                "project.project.stage",
                "create",
                query::create(fields.into()),
            )
            .await?;
        query::created_id(&created)
            .map(ProjectStageId::new)
            .ok_or_else(|| Error::UnexpectedResponse {
                status: 200,
                body: Error::truncate(created.to_string()),
            })
    }

    /// Renames, reorders, folds or archives a project stage. Only the fields set
    /// are written.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn update(&self, id: ProjectStageId, fields: StageFields) -> Result<()> {
        self.client
            .call(
                "project.project.stage",
                "write",
                query::write(id.get(), fields.into()),
            )
            .await
            .map(|_| ())
    }

    /// Deletes a project stage.
    ///
    /// Odoo refuses this while a project is in the stage (a `ValidationError` naming
    /// the project model), so a caller that wants a friendly refusal has to count the
    /// projects first. `StageFields { active: Some(false), .. }` is the archive
    /// alternative Odoo suggests.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], including Odoo's refusal.
    pub async fn delete(&self, id: ProjectStageId) -> Result<()> {
        self.client
            .call("project.project.stage", "unlink", query::unlink(id.get()))
            .await
            .map(|_| ())
    }
}
