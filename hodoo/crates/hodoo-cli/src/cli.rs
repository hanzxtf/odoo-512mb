//! The command-line surface.

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Manage Odoo 19 projects, tasks, stages, tags and milestones.
///
/// Every command prints JSON on stdout; errors are a JSON object on stderr.
/// Credentials come from --url/--api-key or ODOO_URL/ODOO_API_KEY.
#[derive(Debug, Parser)]
#[command(name = "hodoo", version, about, long_about = None)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Command,
}

/// Options that apply to every command.
#[derive(Debug, Args)]
pub struct Global {
    /// Odoo base URL, e.g. https://odoo.example.com
    #[arg(long, env = "ODOO_URL", global = true)]
    pub url: Option<String>,
    /// API key of the Odoo user to act as, from Preferences > Account Security
    #[arg(long, env = "ODOO_API_KEY", global = true)]
    pub api_key: Option<String>,
    /// Send X-Odoo-Database; only needed with several databases behind one domain
    #[arg(long, env = "ODOO_DB", global = true)]
    pub db: Option<String>,
    /// Accept a self-signed certificate
    #[arg(long, global = true)]
    pub insecure: bool,
    /// Do not retry a read after a connection failure or a 502
    #[arg(long, global = true)]
    pub no_retry: bool,
    /// Per-request timeout, in seconds
    #[arg(long, global = true, default_value_t = 30)]
    pub timeout: u64,
    /// Indent the JSON output
    #[arg(long, global = true)]
    pub pretty: bool,
    /// Print Odoo's Python traceback when a call fails
    #[arg(long, short = 'v', global = true)]
    pub verbose: bool,
}

/// What to do.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show the user this API key acts as
    Whoami,
    /// Show the server's Odoo version; needs no API key
    Version,
    /// Projects
    #[command(subcommand)]
    Project(ProjectCmd),
    /// Tasks
    #[command(subcommand)]
    Task(TaskCmd),
    /// Task stages
    #[command(subcommand)]
    Stage(StageCmd),
    /// Milestones
    #[command(subcommand)]
    Milestone(MilestoneCmd),
    /// Tags
    #[command(subcommand)]
    Tag(TagCmd),
    /// Call any model method directly, e.g. `hodoo call res.partner search_read --json '{...}'`
    Call(CallArgs),
}

/// Project commands.
#[derive(Debug, Subcommand)]
pub enum ProjectCmd {
    /// List projects
    Ls(ProjectLsArgs),
    /// Show one project
    Get {
        /// Project id
        id: i64,
    },
    /// Create a project
    Create(ProjectCreateArgs),
    /// Update a project
    Update {
        /// Project id
        id: i64,
        #[command(flatten)]
        fields: ProjectFieldArgs,
    },
    /// Delete a project and everything Odoo cascades from it
    Rm {
        /// Project id
        id: i64,
    },
    /// List the task stages attached to a project
    Stages {
        /// Project id
        id: i64,
    },
    /// Attach task stages to a project, keeping the ones already there
    AttachStage {
        /// Project id
        id: i64,
        /// Stage id, repeatable
        #[arg(long = "stage", value_name = "ID", required = true)]
        stages: Vec<i64>,
    },
    /// Detach task stages from a project
    DetachStage {
        /// Project id
        id: i64,
        /// Stage id, repeatable
        #[arg(long = "stage", value_name = "ID", required = true)]
        stages: Vec<i64>,
    },
    /// Post a message on a project's chatter
    Comment {
        /// Project id
        id: i64,
        /// Message text; Odoo escapes it
        #[arg(long)]
        body: String,
        /// An internal note instead of a comment
        #[arg(long)]
        internal: bool,
    },
}

/// Project listing filters.
#[derive(Debug, Args)]
pub struct ProjectLsArgs {
    /// Substring of the name, case-insensitive
    #[arg(long)]
    pub name: Option<String>,
    /// Customer (res.partner id)
    #[arg(long)]
    pub customer: Option<i64>,
    /// Project manager (res.users id)
    #[arg(long)]
    pub manager: Option<i64>,
    /// Project stage (project.project.stage id)
    #[arg(long)]
    pub stage: Option<i64>,
    /// Tag id, repeatable: projects carrying any of them
    #[arg(long = "tag", value_name = "ID")]
    pub tags: Vec<i64>,
    /// Odoo order string, e.g. "name"
    #[arg(long)]
    pub order: Option<String>,
    /// Maximum records; 0 means no limit
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
    /// Records to skip
    #[arg(long, default_value_t = 0)]
    pub offset: u32,
}

/// Fields shared by project create and update.
#[derive(Debug, Args, Default)]
pub struct ProjectFieldArgs {
    /// Customer (res.partner id)
    #[arg(long)]
    pub customer: Option<i64>,
    /// Project manager (res.users id)
    #[arg(long)]
    pub manager: Option<i64>,
    /// Project stage (project.project.stage id)
    #[arg(long)]
    pub stage: Option<i64>,
    /// Description, as HTML
    #[arg(long)]
    pub description: Option<String>,
    /// Who can reach the project
    #[arg(long, value_enum)]
    pub visibility: Option<VisibilityArg>,
    /// Tag id, repeatable: replaces the project's tags
    #[arg(long = "tag", value_name = "ID")]
    pub tags: Vec<i64>,
    /// Start date, YYYY-MM-DD
    #[arg(long)]
    pub date_start: Option<String>,
    /// Expiration date, YYYY-MM-DD
    #[arg(long)]
    pub date: Option<String>,
    /// Use milestones on this project
    #[arg(long)]
    pub allow_milestones: bool,
    /// Use task dependencies on this project
    #[arg(long)]
    pub allow_task_dependencies: bool,
}

/// Project creation.
#[derive(Debug, Args)]
pub struct ProjectCreateArgs {
    /// Project name
    #[arg(long)]
    pub name: String,
    #[command(flatten)]
    pub fields: ProjectFieldArgs,
}

/// Project visibility.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum VisibilityArg {
    /// Invited internal users only
    Followers,
    /// Invited internal and portal users
    InvitedUsers,
    /// All internal users
    Employees,
    /// All internal users and invited portal users
    Portal,
}

/// Task commands.
#[derive(Debug, Subcommand)]
pub enum TaskCmd {
    /// List tasks
    Ls(TaskLsArgs),
    /// Show one task
    Get {
        /// Task id
        id: i64,
    },
    /// Create a task
    Create(TaskCreateArgs),
    /// Update a task
    Update {
        /// Task id
        id: i64,
        #[command(flatten)]
        fields: TaskFieldArgs,
    },
    /// Delete a task
    Rm {
        /// Task id
        id: i64,
    },
    /// Mark a task done
    Done {
        /// Task id
        id: i64,
    },
    /// Mark a task canceled
    Cancel {
        /// Task id
        id: i64,
    },
    /// Post a message on a task's chatter
    Comment {
        /// Task id
        id: i64,
        /// Message text; Odoo escapes it
        #[arg(long)]
        body: String,
        /// An internal note instead of a comment
        #[arg(long)]
        internal: bool,
    },
    /// Show a task's chatter
    Messages {
        /// Task id
        id: i64,
        /// Maximum messages, oldest first
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Show the tasks a task waits on
    Deps {
        /// Task id
        id: i64,
    },
}

/// Task listing filters.
#[derive(Debug, Args)]
pub struct TaskLsArgs {
    /// Only this project
    #[arg(long)]
    pub project: Option<i64>,
    /// Only this stage
    #[arg(long)]
    pub stage: Option<i64>,
    /// Only this assignee (res.users id)
    #[arg(long)]
    pub assignee: Option<i64>,
    /// Only tasks that are not done or canceled
    #[arg(long)]
    pub open: bool,
    /// Only this state
    #[arg(long, value_enum)]
    pub state: Option<StateArg>,
    /// Substring of the title, case-insensitive
    #[arg(long)]
    pub name: Option<String>,
    /// Tag id, repeatable: tasks carrying any of them
    #[arg(long = "tag", value_name = "ID")]
    pub tags: Vec<i64>,
    /// Only tasks due before this date or datetime
    #[arg(long)]
    pub deadline_before: Option<String>,
    /// Only subtasks of this task
    #[arg(long)]
    pub parent: Option<i64>,
    /// Odoo order string, e.g. "priority desc, date_deadline"
    #[arg(long)]
    pub order: Option<String>,
    /// Maximum records; 0 means no limit
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
    /// Records to skip
    #[arg(long, default_value_t = 0)]
    pub offset: u32,
}

/// Fields shared by task create and update.
#[derive(Debug, Args, Default)]
pub struct TaskFieldArgs {
    /// Project id
    #[arg(long)]
    pub project: Option<i64>,
    /// Stage id, which must be attached to the project
    #[arg(long)]
    pub stage: Option<i64>,
    /// State
    #[arg(long, value_enum)]
    pub state: Option<StateArg>,
    /// Priority
    #[arg(long, value_enum)]
    pub priority: Option<PriorityArg>,
    /// Assignee (res.users id), repeatable; replaces the assignees
    #[arg(long = "assignee", value_name = "ID")]
    pub assignees: Vec<i64>,
    /// Clear the assignees (Odoo assigns the calling user on create otherwise)
    #[arg(long, conflicts_with = "assignees")]
    pub unassign: bool,
    /// Customer contact (res.partner id)
    #[arg(long)]
    pub customer: Option<i64>,
    /// Deadline, YYYY-MM-DD or YYYY-MM-DD HH:MM:SS (UTC)
    #[arg(long)]
    pub deadline: Option<String>,
    /// Planned hours
    #[arg(long)]
    pub allocated_hours: Option<f64>,
    /// Tag id, repeatable; replaces the task's tags
    #[arg(long = "tag", value_name = "ID")]
    pub tags: Vec<i64>,
    /// Clear the task's tags
    #[arg(long, conflicts_with = "tags")]
    pub clear_tags: bool,
    /// Parent task id, making this a subtask
    #[arg(long)]
    pub parent: Option<i64>,
    /// Milestone id
    #[arg(long)]
    pub milestone: Option<i64>,
    /// Task id this one waits on, repeatable; replaces the dependencies
    #[arg(long = "depends-on", value_name = "ID")]
    pub depends_on: Vec<i64>,
    /// Description, as HTML
    #[arg(long)]
    pub description: Option<String>,
}

/// Task creation.
#[derive(Debug, Args)]
pub struct TaskCreateArgs {
    /// Task title
    #[arg(long)]
    pub name: String,
    #[command(flatten)]
    pub fields: TaskFieldArgs,
}

/// Task states.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StateArg {
    /// In progress
    InProgress,
    /// Changes requested
    ChangesRequested,
    /// Approved
    Approved,
    /// Done
    Done,
    /// Canceled
    Canceled,
    /// Waiting
    Waiting,
}

/// Task priorities.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PriorityArg {
    /// Low
    Low,
    /// Medium
    Medium,
    /// High
    High,
    /// Urgent
    Urgent,
}

/// Stage commands.
#[derive(Debug, Subcommand)]
pub enum StageCmd {
    /// List task stages
    Ls {
        /// Only stages attached to this project
        #[arg(long)]
        project: Option<i64>,
        /// Maximum records; 0 means no limit
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Create a task stage, attached to a project when --project is given
    Create {
        /// Stage name
        #[arg(long)]
        name: String,
        /// Project id; without it the stage belongs to no project and no task can use it
        #[arg(long)]
        project: Option<i64>,
        /// Kanban order
        #[arg(long)]
        sequence: Option<i64>,
        /// Fold it in the kanban
        #[arg(long)]
        fold: bool,
    },
}

/// Milestone commands.
#[derive(Debug, Subcommand)]
pub enum MilestoneCmd {
    /// List a project's milestones
    Ls {
        /// Project id
        #[arg(long)]
        project: i64,
    },
    /// Create a milestone
    Create {
        /// Project id
        #[arg(long)]
        project: i64,
        /// Milestone name
        #[arg(long)]
        name: String,
        /// Deadline, YYYY-MM-DD
        #[arg(long)]
        deadline: Option<String>,
    },
    /// Mark a milestone reached, or not reached
    Reached {
        /// Milestone id
        id: i64,
        /// Mark it unreached instead
        #[arg(long)]
        undo: bool,
    },
    /// Delete a milestone
    Rm {
        /// Milestone id
        id: i64,
    },
}

/// Tag commands.
#[derive(Debug, Subcommand)]
pub enum TagCmd {
    /// List tags
    Ls,
    /// The id of a tag by exact name, creating it if it does not exist
    Ensure {
        /// Tag name
        name: String,
    },
}

/// A raw JSON-2 call.
#[derive(Debug, Args)]
pub struct CallArgs {
    /// Odoo model, e.g. project.task
    pub model: String,
    /// Method, e.g. search_read
    pub method: String,
    /// Arguments, as a JSON object
    #[arg(long)]
    pub json: Option<String>,
    /// Record ids to run the method on, comma separated
    #[arg(long, value_delimiter = ',')]
    pub ids: Vec<i64>,
}
