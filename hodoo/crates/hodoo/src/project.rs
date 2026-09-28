//! `project.project`.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::Client;
use crate::chatter;
use crate::datetime;
use crate::de;
use crate::error::{Error, Result};
use crate::id::{PartnerId, ProjectId, ProjectStageId, TagId, TaskStageId, UserId};
use crate::query;

/// The fields this crate reads from `project.project`.
///
/// Odoo returns every field when `fields` is omitted, computed columns
/// included, so reads always name what they want.
pub const FIELDS: &[&str] = &[
    "id",
    "name",
    "description",
    "active",
    "partner_id",
    "user_id",
    "stage_id",
    "date_start",
    "date",
    "privacy_visibility",
    "tag_ids",
    "type_ids",
    "task_count",
    "open_task_count",
];

/// Who can reach a project and its tasks.
///
/// Odoo's default is [`Visibility::Portal`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// Invited internal users only; assignees get access automatically.
    #[serde(rename = "followers")]
    Followers,
    /// Invited internal and portal users.
    #[serde(rename = "invited_users")]
    InvitedUsers,
    /// All internal users.
    #[serde(rename = "employees")]
    Employees,
    /// All internal users, plus invited portal users.
    #[default]
    #[serde(rename = "portal")]
    Portal,
}

impl Visibility {
    /// The value Odoo stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Visibility::Followers => "followers",
            Visibility::InvitedUsers => "invited_users",
            Visibility::Employees => "employees",
            Visibility::Portal => "portal",
        }
    }
}

/// A project, as read from Odoo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// `project.project.id`.
    pub id: ProjectId,
    /// Project name.
    #[serde(default)]
    pub name: String,
    /// Description, as HTML.
    #[serde(default, deserialize_with = "de::opt_string")]
    pub description: Option<String>,
    /// `false` for an archived project.
    #[serde(default = "de::default_true")]
    pub active: bool,
    /// Customer (`partner_id`).
    #[serde(default, rename = "partner_id", deserialize_with = "de::opt_id")]
    pub customer: Option<PartnerId>,
    /// Project manager (`user_id`).
    #[serde(default, rename = "user_id", deserialize_with = "de::opt_id")]
    pub manager: Option<UserId>,
    /// The project's stage (`stage_id`), a `project.project.stage`. Not a task
    /// stage: see [`TaskStage`](crate::TaskStage).
    #[serde(default, rename = "stage_id", deserialize_with = "de::opt_id")]
    pub stage: Option<ProjectStageId>,
    /// Start date (`date_start`).
    #[serde(
        default,
        rename = "date_start",
        deserialize_with = "datetime::opt_date",
        serialize_with = "datetime::ser_opt_date"
    )]
    pub date_start: Option<NaiveDate>,
    /// Expiration date (`date`).
    #[serde(
        default,
        rename = "date",
        deserialize_with = "datetime::opt_date",
        serialize_with = "datetime::ser_opt_date"
    )]
    pub date: Option<NaiveDate>,
    /// Who may access the project (`privacy_visibility`).
    #[serde(default, rename = "privacy_visibility")]
    pub visibility: Visibility,
    /// Tags (`tag_ids`).
    #[serde(default, rename = "tag_ids", deserialize_with = "de::id_list")]
    pub tags: Vec<TagId>,
    /// The task stages attached to this project (`type_ids`). A task's stage
    /// must be one of these, which is why a fresh project cannot hold a staged
    /// task until a stage is attached.
    #[serde(default, rename = "type_ids", deserialize_with = "de::id_list")]
    pub task_stages: Vec<TaskStageId>,
    /// How many tasks the project holds (`task_count`).
    #[serde(default, rename = "task_count")]
    pub task_count: i64,
    /// How many of those are open (`open_task_count`).
    #[serde(default, rename = "open_task_count")]
    pub open_task_count: i64,
}

/// A project filter, translated into an Odoo domain.
///
/// Odoo excludes archived records from a search unless the call context says
/// otherwise, so `active` is deliberately not a filter here.
#[derive(Debug, Clone, Default)]
pub struct ProjectFilter {
    /// Substring match on the name, case-insensitive.
    pub name_contains: Option<String>,
    /// Exactly this customer.
    pub customer: Option<PartnerId>,
    /// Exactly this project manager.
    pub manager: Option<UserId>,
    /// Exactly this project stage.
    pub stage: Option<ProjectStageId>,
    /// Projects carrying any of these tags.
    pub tags: Vec<TagId>,
    /// Odoo's `order` string, e.g. `"name, id desc"`. Odoo's default is
    /// `sequence, name`.
    pub order: Option<String>,
    /// Maximum records to return. Unset means Odoo's default, which is *all*
    /// matching records.
    pub limit: Option<u32>,
    /// Records to skip.
    pub offset: Option<u32>,
}

impl ProjectFilter {
    fn domain(&self) -> Value {
        query::domain([
            self.name_contains
                .as_ref()
                .map_or(Value::Null, |name| json!(["name", "ilike", name])),
            self.customer
                .map_or(Value::Null, |id| json!(["partner_id", "=", id.get()])),
            self.manager
                .map_or(Value::Null, |id| json!(["user_id", "=", id.get()])),
            self.stage
                .map_or(Value::Null, |id| json!(["stage_id", "=", id.get()])),
            if self.tags.is_empty() {
                Value::Null
            } else {
                json!([
                    "tag_ids",
                    "in",
                    self.tags.iter().map(|tag| tag.get()).collect::<Vec<_>>()
                ])
            },
        ])
    }
}

/// The project fields a caller may write. `None` means "leave alone".
///
/// `Some(vec![])` on an x2many means "clear it", which is why these are
/// `Option`s rather than empty collections.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectFields {
    /// Project name. Required by Odoo on create.
    pub name: Option<String>,
    /// Description, as HTML.
    pub description: Option<String>,
    /// Customer (`partner_id`).
    pub customer: Option<PartnerId>,
    /// Project manager (`user_id`).
    pub manager: Option<UserId>,
    /// Project stage (`stage_id`).
    pub stage: Option<ProjectStageId>,
    /// Start date (`date_start`).
    pub date_start: Option<NaiveDate>,
    /// Expiration date (`date`).
    pub date: Option<NaiveDate>,
    /// Visibility (`privacy_visibility`).
    pub visibility: Option<Visibility>,
    /// Tags (`tag_ids`).
    pub tags: Option<Vec<TagId>>,
    /// Whether the project uses milestones (`allow_milestones`). Without it a
    /// milestone cannot be created on the project.
    pub allow_milestones: Option<bool>,
    /// Whether the project uses task dependencies (`allow_task_dependencies`).
    pub allow_task_dependencies: Option<bool>,
}

impl ProjectFields {
    /// The fields Odoo requires to create a project.
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
        if let Some(description) = &self.description {
            vals.insert("description".into(), json!(description));
        }
        if let Some(customer) = self.customer {
            vals.insert("partner_id".into(), json!(customer.get()));
        }
        if let Some(manager) = self.manager {
            vals.insert("user_id".into(), json!(manager.get()));
        }
        if let Some(stage) = self.stage {
            vals.insert("stage_id".into(), json!(stage.get()));
        }
        if let Some(date_start) = self.date_start {
            vals.insert(
                "date_start".into(),
                json!(datetime::format_date(date_start)),
            );
        }
        if let Some(date) = self.date {
            vals.insert("date".into(), json!(datetime::format_date(date)));
        }
        if let Some(visibility) = self.visibility {
            vals.insert("privacy_visibility".into(), json!(visibility.as_str()));
        }
        if let Some(tags) = &self.tags {
            vals.insert(
                "tag_ids".into(),
                query::replace(tags.iter().map(|tag| tag.get())),
            );
        }
        if let Some(allow) = self.allow_milestones {
            vals.insert("allow_milestones".into(), json!(allow));
        }
        if let Some(allow) = self.allow_task_dependencies {
            vals.insert("allow_task_dependencies".into(), json!(allow));
        }
        Value::Object(vals)
    }
}

impl From<ProjectFields> for Value {
    fn from(fields: ProjectFields) -> Self {
        fields.to_value()
    }
}

/// Access to `project.project`.
#[derive(Debug, Clone, Copy)]
pub struct Projects<'a> {
    client: &'a Client,
}

impl<'a> Projects<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Searches projects.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if a record does
    /// not match [`Project`].
    pub async fn search(&self, filter: ProjectFilter) -> Result<Vec<Project>> {
        self.client
            .call_as(
                "project.project",
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

    /// Counts matching projects without fetching them.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if the answer is
    /// not a number.
    pub async fn count(&self, filter: ProjectFilter) -> Result<u64> {
        self.client
            .call_as(
                "project.project",
                "search_count",
                query::count(filter.domain()),
            )
            .await
    }

    /// Reads one project.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], or [`Error::Missing`] if the id no longer
    /// exists.
    pub async fn get(&self, id: ProjectId) -> Result<Project> {
        let projects: Vec<Project> = self
            .client
            .call_as("project.project", "read", query::read(id.get(), FIELDS))
            .await?;
        projects.into_iter().next().ok_or(Error::Missing {
            model: "project.project".into(),
            raw_id: id.get(),
        })
    }

    /// Creates a project.
    ///
    /// Odoo assigns the project stage itself when the user may see stages, and
    /// it does **not** create task stages: a new project has an empty
    /// [`Project::task_stages`], so attach one with
    /// [`attach_stages`](Projects::attach_stages) before creating a staged task.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn create(&self, fields: ProjectFields) -> Result<ProjectId> {
        let created = self
            .client
            .call("project.project", "create", query::create(fields.into()))
            .await?;
        query::created_id(&created)
            .map(ProjectId::new)
            .ok_or_else(|| Error::UnexpectedResponse {
                status: 200,
                body: Error::truncate(created.to_string()),
            })
    }

    /// Writes fields on one project.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn update(&self, id: ProjectId, fields: ProjectFields) -> Result<()> {
        self.client
            .call(
                "project.project",
                "write",
                query::write(id.get(), fields.into()),
            )
            .await
            .map(|_| ())
    }

    /// Deletes a project, and everything Odoo cascades from it.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn delete(&self, id: ProjectId) -> Result<()> {
        self.client
            .call("project.project", "unlink", query::unlink(id.get()))
            .await
            .map(|_| ())
    }

    /// Replaces the project's task stages outright.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn set_stages(&self, id: ProjectId, stages: &[TaskStageId]) -> Result<()> {
        let vals = json!({
            "type_ids": query::replace(stages.iter().map(|stage| stage.get())),
        });
        self.client
            .call("project.project", "write", query::write(id.get(), vals))
            .await
            .map(|_| ())
    }

    /// Adds task stages to a project, keeping the ones already attached.
    ///
    /// This reads the project first and writes the union back: Odoo has no
    /// "append to x2many" call that takes a bare id list, and the alternative,
    /// a three-way merge command, needs the current value anyway.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn attach_stages(&self, id: ProjectId, stages: &[TaskStageId]) -> Result<()> {
        let mut ids: Vec<TaskStageId> = self.get(id).await?.task_stages;
        for stage in stages {
            if !ids.contains(stage) {
                ids.push(*stage);
            }
        }
        self.set_stages(id, &ids).await
    }

    /// Removes task stages from a project, keeping the rest.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn detach_stages(&self, id: ProjectId, stages: &[TaskStageId]) -> Result<()> {
        let remaining: Vec<TaskStageId> = self
            .get(id)
            .await?
            .task_stages
            .into_iter()
            .filter(|attached| !stages.contains(attached))
            .collect();
        self.set_stages(id, &remaining).await
    }

    /// Posts a message on the project's chatter.
    ///
    /// `body` is plain text: Odoo escapes it. An internal note is only visible
    /// to internal users, a comment is visible to followers.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn comment(&self, id: ProjectId, body: &str, internal: bool) -> Result<()> {
        chatter::comment(self.client, "project.project", id.get(), body, internal).await
    }
}
