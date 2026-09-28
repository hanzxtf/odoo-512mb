//! `project.task`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::Client;
use crate::chatter;
use crate::datetime;
use crate::de;
use crate::error::{Error, Result};
use crate::id::{MilestoneId, PartnerId, ProjectId, TagId, TaskId, TaskStageId, UserId};
use crate::query;

/// The fields this crate reads from `project.task`.
pub const FIELDS: &[&str] = &[
    "id",
    "name",
    "description",
    "project_id",
    "stage_id",
    "state",
    "priority",
    "user_ids",
    "partner_id",
    "date_deadline",
    "date_assign",
    "date_last_stage_update",
    "allocated_hours",
    "tag_ids",
    "parent_id",
    "milestone_id",
    "is_closed",
];

/// Where a task stands.
///
/// Odoo's closed values are `1_done` and `1_canceled`, which is why this cannot
/// be spelled with `FromStr` on a tidied-up name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TaskState {
    /// `01_in_progress`.
    #[default]
    #[serde(rename = "01_in_progress")]
    InProgress,
    /// `02_changes_requested`.
    #[serde(rename = "02_changes_requested")]
    ChangesRequested,
    /// `03_approved`.
    #[serde(rename = "03_approved")]
    Approved,
    /// `1_done`: finished.
    #[serde(rename = "1_done")]
    Done,
    /// `1_canceled`.
    #[serde(rename = "1_canceled")]
    Canceled,
    /// `04_waiting_normal`.
    #[serde(rename = "04_waiting_normal")]
    Waiting,
}

impl TaskState {
    /// The value Odoo stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TaskState::InProgress => "01_in_progress",
            TaskState::ChangesRequested => "02_changes_requested",
            TaskState::Approved => "03_approved",
            TaskState::Done => "1_done",
            TaskState::Canceled => "1_canceled",
            TaskState::Waiting => "04_waiting_normal",
        }
    }

    /// Whether Odoo counts this state as closed. `Done` and `Canceled` do.
    #[must_use]
    pub const fn is_closed(self) -> bool {
        matches!(self, TaskState::Done | TaskState::Canceled)
    }
}

/// A task's priority, Odoo's `0` to `3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Priority {
    /// `0`.
    #[default]
    #[serde(rename = "0")]
    Low,
    /// `1`.
    #[serde(rename = "1")]
    Medium,
    /// `2`.
    #[serde(rename = "2")]
    High,
    /// `3`.
    #[serde(rename = "3")]
    Urgent,
}

impl Priority {
    /// The value Odoo stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Priority::Low => "0",
            Priority::Medium => "1",
            Priority::High => "2",
            Priority::Urgent => "3",
        }
    }
}

/// A task, as read from Odoo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// `project.task.id`.
    pub id: TaskId,
    /// Title.
    #[serde(default)]
    pub name: String,
    /// Description, as HTML.
    #[serde(default, deserialize_with = "de::opt_string")]
    pub description: Option<String>,
    /// The project (`project_id`). `None` means a private task, which is only
    /// visible to its assignees and cannot hold a stage.
    #[serde(default, rename = "project_id", deserialize_with = "de::opt_id")]
    pub project: Option<ProjectId>,
    /// The stage (`stage_id`), one of the project's
    /// [`task_stages`](crate::Project::task_stages).
    #[serde(default, rename = "stage_id", deserialize_with = "de::opt_id")]
    pub stage: Option<TaskStageId>,
    /// Where the task stands (`state`).
    #[serde(default)]
    pub state: TaskState,
    /// Priority (`priority`).
    #[serde(default)]
    pub priority: Priority,
    /// Assignees (`user_ids`).
    #[serde(default, rename = "user_ids", deserialize_with = "de::id_list")]
    pub assignees: Vec<UserId>,
    /// Customer contact (`partner_id`).
    #[serde(default, rename = "partner_id", deserialize_with = "de::opt_id")]
    pub customer: Option<PartnerId>,
    /// Deadline (`date_deadline`), UTC.
    #[serde(
        default,
        rename = "date_deadline",
        deserialize_with = "datetime::opt_datetime",
        serialize_with = "datetime::ser_opt_datetime"
    )]
    pub deadline: Option<DateTime<Utc>>,
    /// When someone was last assigned (`date_assign`).
    #[serde(
        default,
        rename = "date_assign",
        deserialize_with = "datetime::opt_datetime",
        serialize_with = "datetime::ser_opt_datetime"
    )]
    pub date_assign: Option<DateTime<Utc>>,
    /// When the task last changed stage (`date_last_stage_update`).
    #[serde(
        default,
        rename = "date_last_stage_update",
        deserialize_with = "datetime::opt_datetime",
        serialize_with = "datetime::ser_opt_datetime"
    )]
    pub date_last_stage_update: Option<DateTime<Utc>>,
    /// Planned time in hours (`allocated_hours`).
    #[serde(default, rename = "allocated_hours")]
    pub allocated_hours: Option<f64>,
    /// Tags (`tag_ids`).
    #[serde(default, rename = "tag_ids", deserialize_with = "de::id_list")]
    pub tags: Vec<TagId>,
    /// Parent task, for subtasks (`parent_id`).
    #[serde(default, rename = "parent_id", deserialize_with = "de::opt_id")]
    pub parent: Option<TaskId>,
    /// Milestone (`milestone_id`), when the project has milestones enabled.
    #[serde(default, rename = "milestone_id", deserialize_with = "de::opt_id")]
    pub milestone: Option<MilestoneId>,
    /// Odoo's own "this state is closed" flag (`is_closed`).
    #[serde(default, rename = "is_closed")]
    pub is_closed: bool,
}

/// A task filter, translated into an Odoo domain.
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    /// Only tasks of this project.
    pub project: Option<ProjectId>,
    /// Only tasks in this stage.
    pub stage: Option<TaskStageId>,
    /// Only tasks assigned to this user.
    pub assignee: Option<UserId>,
    /// Only tasks whose state is not closed. Combines with `state` as an `and`.
    pub open_only: bool,
    /// Only tasks in exactly this state.
    pub state: Option<TaskState>,
    /// Substring match on the title, case-insensitive.
    pub name_contains: Option<String>,
    /// Tasks carrying any of these tags.
    pub tags: Vec<TagId>,
    /// Only tasks due strictly before this instant.
    pub deadline_before: Option<DateTime<Utc>>,
    /// Only children of this task.
    pub parent: Option<TaskId>,
    /// Odoo's `order` string, e.g. `"priority desc, date_deadline"`. Odoo's
    /// default is `priority desc, sequence, id desc`.
    pub order: Option<String>,
    /// Maximum records to return. Unset means Odoo's default, which is *all*
    /// matching records.
    pub limit: Option<u32>,
    /// Records to skip.
    pub offset: Option<u32>,
}

impl TaskFilter {
    fn domain(&self) -> Value {
        query::domain([
            self.project
                .map_or(Value::Null, |id| json!(["project_id", "=", id.get()])),
            self.stage
                .map_or(Value::Null, |id| json!(["stage_id", "=", id.get()])),
            self.assignee
                .map_or(Value::Null, |id| json!(["user_ids", "in", [id.get()]])),
            if self.open_only {
                json!(["is_closed", "=", false])
            } else {
                Value::Null
            },
            self.state
                .map_or(Value::Null, |state| json!(["state", "=", state.as_str()])),
            self.name_contains
                .as_ref()
                .map_or(Value::Null, |name| json!(["name", "ilike", name])),
            if self.tags.is_empty() {
                Value::Null
            } else {
                json!([
                    "tag_ids",
                    "in",
                    self.tags.iter().map(|tag| tag.get()).collect::<Vec<_>>()
                ])
            },
            self.deadline_before.map_or(Value::Null, |deadline| {
                json!(["date_deadline", "<", datetime::format_datetime(deadline)])
            }),
            self.parent
                .map_or(Value::Null, |id| json!(["parent_id", "=", id.get()])),
        ])
    }
}

/// The task fields a caller may write. `None` means "leave alone".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TaskFields {
    /// Title. Required by Odoo on create.
    pub name: Option<String>,
    /// Description, as HTML.
    pub description: Option<String>,
    /// Project (`project_id`). Without it the task is private and `stage` is
    /// dropped by Odoo.
    pub project: Option<ProjectId>,
    /// Stage (`stage_id`), normally one of the project's
    /// [`task_stages`](crate::Project::task_stages).
    ///
    /// Odoo does **not** enforce that: a stage belonging to another project is
    /// accepted over the API and the task then sits somewhere its kanban does
    /// not show. Attach the stage first (see
    /// [`Stages::create_in`](crate::stage::Stages::create_in)).
    pub stage: Option<TaskStageId>,
    /// State (`state`).
    pub state: Option<TaskState>,
    /// Priority (`priority`).
    pub priority: Option<Priority>,
    /// Assignees (`user_ids`).
    ///
    /// Odoo assigns the *calling* user to every task it creates over the API,
    /// and adds that user to the list when one is set here; a subtask (one with
    /// a `parent`) is the exception. `Some(vec![])` clears the assignees.
    pub assignees: Option<Vec<UserId>>,
    /// Customer contact (`partner_id`).
    pub customer: Option<PartnerId>,
    /// Deadline (`date_deadline`), UTC.
    pub deadline: Option<DateTime<Utc>>,
    /// Planned time in hours (`allocated_hours`).
    pub allocated_hours: Option<f64>,
    /// Tags (`tag_ids`). `Some(vec![])` clears them.
    pub tags: Option<Vec<TagId>>,
    /// Parent task (`parent_id`), i.e. "make this a subtask of".
    pub parent: Option<TaskId>,
    /// Milestone (`milestone_id`).
    pub milestone: Option<MilestoneId>,
    /// Tasks this one waits on (`depend_on_ids`). Odoo surfaces dependencies
    /// only when the project has `allow_task_dependencies` set.
    ///
    /// Odoo derives [`TaskState::Waiting`] from this list, but only recomputes
    /// it when the field is written, so a task *created* with dependencies still
    /// reads `01_in_progress` until they are written again or the state is set.
    pub depends_on: Option<Vec<TaskId>>,
}

impl TaskFields {
    /// The fields Odoo requires to create a task.
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
        if let Some(project) = self.project {
            vals.insert("project_id".into(), json!(project.get()));
        }
        if let Some(stage) = self.stage {
            vals.insert("stage_id".into(), json!(stage.get()));
        }
        if let Some(state) = self.state {
            vals.insert("state".into(), json!(state.as_str()));
        }
        if let Some(priority) = self.priority {
            vals.insert("priority".into(), json!(priority.as_str()));
        }
        if let Some(assignees) = &self.assignees {
            vals.insert(
                "user_ids".into(),
                query::replace(assignees.iter().map(|user| user.get())),
            );
        }
        if let Some(customer) = self.customer {
            vals.insert("partner_id".into(), json!(customer.get()));
        }
        if let Some(deadline) = self.deadline {
            vals.insert(
                "date_deadline".into(),
                json!(datetime::format_datetime(deadline)),
            );
        }
        if let Some(hours) = self.allocated_hours {
            vals.insert("allocated_hours".into(), json!(hours));
        }
        if let Some(tags) = &self.tags {
            vals.insert(
                "tag_ids".into(),
                query::replace(tags.iter().map(|tag| tag.get())),
            );
        }
        if let Some(parent) = self.parent {
            vals.insert("parent_id".into(), json!(parent.get()));
        }
        if let Some(milestone) = self.milestone {
            vals.insert("milestone_id".into(), json!(milestone.get()));
        }
        if let Some(depends_on) = &self.depends_on {
            vals.insert(
                "depend_on_ids".into(),
                query::replace(depends_on.iter().map(|task| task.get())),
            );
        }
        Value::Object(vals)
    }
}

impl From<TaskFields> for Value {
    fn from(fields: TaskFields) -> Self {
        fields.to_value()
    }
}

/// The slice of `project.task` that [`Tasks::dependencies`] reads.
#[derive(serde::Deserialize)]
struct Dependencies {
    #[serde(default, rename = "depend_on_ids", deserialize_with = "de::id_list")]
    depends_on: Vec<TaskId>,
}

/// Access to `project.task`.
#[derive(Debug, Clone, Copy)]
pub struct Tasks<'a> {
    client: &'a Client,
}

impl<'a> Tasks<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Searches tasks.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if a record does
    /// not match [`Task`].
    pub async fn search(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        self.client
            .call_as(
                "project.task",
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

    /// Counts matching tasks without fetching them.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if the answer is
    /// not a number.
    pub async fn count(&self, filter: TaskFilter) -> Result<u64> {
        self.client
            .call_as(
                "project.task",
                "search_count",
                query::count(filter.domain()),
            )
            .await
    }

    /// Reads one task.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], or [`Error::Missing`] if the id no longer
    /// exists.
    pub async fn get(&self, id: TaskId) -> Result<Task> {
        let tasks: Vec<Task> = self
            .client
            .call_as("project.task", "read", query::read(id.get(), FIELDS))
            .await?;
        tasks.into_iter().next().ok_or(Error::Missing {
            model: "project.task".into(),
            raw_id: id.get(),
        })
    }

    /// Creates a task.
    ///
    /// Odoo picks the project's default stage when `stage` is unset.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn create(&self, fields: TaskFields) -> Result<TaskId> {
        let created = self
            .client
            .call("project.task", "create", query::create(fields.into()))
            .await?;
        query::created_id(&created)
            .map(TaskId::new)
            .ok_or_else(|| Error::UnexpectedResponse {
                status: 200,
                body: Error::truncate(created.to_string()),
            })
    }

    /// Writes fields on one task.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn update(&self, id: TaskId, fields: TaskFields) -> Result<()> {
        self.client
            .call(
                "project.task",
                "write",
                query::write(id.get(), fields.into()),
            )
            .await
            .map(|_| ())
    }

    /// Moves a task to a state.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn set_state(&self, id: TaskId, state: TaskState) -> Result<()> {
        self.update(
            id,
            TaskFields {
                state: Some(state),
                ..TaskFields::default()
            },
        )
        .await
    }

    /// Deletes a task.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn delete(&self, id: TaskId) -> Result<()> {
        self.client
            .call("project.task", "unlink", query::unlink(id.get()))
            .await
            .map(|_| ())
    }

    /// Posts a message on the task's chatter.
    ///
    /// `body` is plain text: Odoo escapes it. An internal note is only visible
    /// to internal users, a comment is visible to followers.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn comment(&self, id: TaskId, body: &str, internal: bool) -> Result<()> {
        chatter::comment(self.client, "project.task", id.get(), body, internal).await
    }

    /// The messages on a task's chatter, oldest first.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn messages(&self, id: TaskId, limit: Option<u32>) -> Result<Vec<chatter::Message>> {
        chatter::messages(self.client, "project.task", id.get(), limit).await
    }

    /// The tasks this one waits on (`depend_on_ids`).
    ///
    /// Read on its own instead of on every [`Task`]: dependencies are rarely
    /// wanted and would widen every search result.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], or [`Error::Missing`] if the id is gone.
    pub async fn dependencies(&self, id: TaskId) -> Result<Vec<TaskId>> {
        let rows: Vec<Dependencies> = self
            .client
            .call_as(
                "project.task",
                "read",
                query::read(id.get(), &["depend_on_ids"]),
            )
            .await?;
        rows.into_iter()
            .next()
            .map(|row| row.depends_on)
            .ok_or(Error::Missing {
                model: "project.task".into(),
                raw_id: id.get(),
            })
    }
}
