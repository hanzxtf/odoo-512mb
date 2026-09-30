//! The command surface: names, flags, help text, examples.
//!
//! Nothing here talks to Odoo. It exists so that `hodoo --help`, `hodoo task create
//! --help` and `hodoo help task` explain the tool without anyone reading a README,
//! which is why every command carries an example and a sentence about what it does.

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

/// What the top-level help says after the flags.
const TOP_HELP: &str = "\
Getting started:
  hodoo version                          the server's Odoo version; no key needed
  hodoo whoami                           who the key is, and on which server
  hodoo project ls                       what is there
  hodoo task create --name \"x\" --project acme

Credentials, in order of precedence:
  --url/--api-key/--db  ·  $ODOO_URL/$ODOO_API_KEY/$ODOO_DB  ·  a .env above the
  working directory. Create a key in Odoo under Preferences > Account Security.

Output:
  A table for people, JSON for scripts. `-o json` (or HODOO_OUTPUT=json) makes every
  command machine-readable; `--no-headers` drops table headers for awk and grep.
  Errors and hints go to stderr; results go to stdout.

Exit codes:
  0 success · 1 the operation failed · 2 the invocation was wrong · 141 closed pipe

References:
  Anywhere a project, task, stage, milestone, user or tag is expected, an id or a
  name works: `--project 49`, `--project acme`, `--stage Review`, `--tag urgent`.
  A name that matches nothing says so; one that matches several lists them.

Stages:
  A task's stage (`--stage` on a task, `hodoo project task-stages`) is a kanban column
  of its project. A project's stage (`--stage` on a project, `hodoo project stages`) is
  one of a handful the whole server shares. Different models, both by name or id.";

/// Manage Odoo 19 projects, tasks and everything around them.
#[derive(Debug, Parser)]
#[command(
    name = "hodoo",
    version,
    about = "Manage Odoo 19 projects, tasks, task stages, milestones and tags",
    long_about = "Talk to an Odoo 19 server over its JSON-2 API: projects, tasks, task \
                  stages, tags, milestones, subtasks, dependencies and chatter.\n\n\
                  Reads are surgical (the client only asks for the fields it shows), writes \
                  are one call each, and anything without a command of its own is one \
                  `hodoo call` away.",
    after_help = TOP_HELP,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Command,
}

/// Options that apply to every command.
#[derive(Debug, Args)]
pub struct Global {
    /// Output format: a table for people, json for scripts
    #[arg(
        short = 'o',
        long,
        value_name = "table|json",
        env = "HODOO_OUTPUT",
        global = true
    )]
    pub output: Option<String>,

    /// Shorthand for -o json
    #[arg(long, global = true)]
    pub json: bool,

    /// Colour: auto (default), always, never
    #[arg(
        long,
        value_name = "auto|always|never",
        env = "HODOO_COLOR",
        global = true
    )]
    pub color: Option<String>,

    /// Do not print table headers, for awk and grep
    #[arg(long, global = true)]
    pub no_headers: bool,

    /// Indent JSON output
    #[arg(long, global = true)]
    pub pretty: bool,

    /// Print only what was asked for: no confirmations, no hints
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// Explain what is happening, and show Odoo's traceback on failure
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

    /// Show what a change would send, and send nothing
    #[arg(short = 'n', long, global = true)]
    pub dry_run: bool,

    /// Never prompt; fail instead when an answer is needed
    #[arg(long, global = true)]
    pub no_input: bool,

    /// Odoo base URL, e.g. https://odoo.example.com
    #[arg(long, env = "ODOO_URL", global = true)]
    pub url: Option<String>,

    /// API key of the user to act as: Preferences > Account Security > New API Key
    #[arg(long, env = "ODOO_API_KEY", global = true)]
    pub api_key: Option<String>,

    /// Send X-Odoo-Database; only needed with several databases behind one domain
    #[arg(long, env = "ODOO_DB", global = true)]
    pub db: Option<String>,

    /// Accept a self-signed or otherwise invalid certificate
    #[arg(long, global = true)]
    pub insecure: bool,

    /// Do not retry a read after a connection failure or a 502
    #[arg(long, global = true)]
    pub no_retry: bool,

    /// Per-request timeout, in seconds
    #[arg(long, global = true, default_value_t = 30)]
    pub timeout: u64,
}

/// What to do.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show the user this API key acts as, and the server it reaches
    #[command(
        long_about = "Shows the user behind the API key plus the server's Odoo \
                            version, so a first run proves url, certificate and key in one \
                            go.\n\nExamples:\n  hodoo whoami\n  hodoo whoami -o json"
    )]
    Whoami,

    /// Show the server's Odoo version; needs no API key
    Version,

    /// Projects
    #[command(subcommand)]
    Project(ProjectCmd),

    /// Tasks
    #[command(subcommand)]
    Task(TaskCmd),

    /// Milestones
    #[command(subcommand)]
    Milestone(MilestoneCmd),

    /// Tags
    #[command(subcommand)]
    Tag(TagCmd),

    /// Every open task of a project, grouped by stage
    #[command(
        long_about = "The kanban a founder actually looks at: one line per stage, the \
                            tasks in it, soonest deadline first.\n\nThat is the table. Its \
                            JSON is one object per project holding every open task in a \
                            single `tasks` array, so a script counts tasks rather than \
                            lines.\n\nExamples:\n  hodoo board\n  \
                            hodoo board acme\n  hodoo board 49 --mine"
    )]
    Board(BoardArgs),

    /// Call any Odoo model method; the escape hatch for what has no command
    #[command(
        long_about = "POSTs to /json/2/<model>/<method> with the body you give. Use it \
                            for models and methods hodoo does not wrap, and for rollups \
                            (search_count, read_group).\n\nExamples:\n  hodoo call res.partner \
                            search_read --body '{\"domain\":[],\"fields\":[\"name\"]}'\n  hodoo call \
                            project.task search_count --body '{\"domain\":[[\"is_closed\",\"=\",false]]}'\n  \
                            hodoo call project.task write --ids 31 --body '{\"vals\":{\"priority\":\"3\"}}'"
    )]
    Call(CallArgs),

    /// Print a shell completion script
    #[command(
        long_about = "Prints a completion script for the given shell.\n\nExamples:\n  \
                            hodoo completions bash > ~/.local/share/bash-completion/completions/hodoo\n  \
                            hodoo completions zsh > ~/.zfunc/_hodoo"
    )]
    Completions(CompletionsArgs),
}

/// Project commands.
#[derive(Debug, Subcommand)]
pub enum ProjectCmd {
    /// List projects
    #[command(
        long_about = "Lists projects as a table: customer, manager, visibility, how \
                            many tasks are open, and when they end.\n\nExamples:\n  hodoo project \
                            ls\n  hodoo project ls --mine\n  hodoo project ls --customer acme \
                            --json"
    )]
    Ls(ProjectLsArgs),

    /// Show one project: dates, stages, counts and recent chatter
    #[command(
        long_about = "Everything about one project on one screen.\n\nExamples:\n  hodoo \
                            project show acme\n  hodoo project show 49"
    )]
    Show {
        /// Project id or name
        #[arg(value_name = "PROJECT")]
        project: String,
    },

    /// Create a project, optionally from a template
    #[command(
        long_about = "Creates a project. With --template the project is copied from a \
                            template: tasks, subtasks, stages, tags and dependencies come \
                            along, the dates shift to keep the template's duration, and the \
                            customer is never copied unless you pass --customer.\n\nExamples:\n  \
                            hodoo project create --name \"Acme website\" --customer acme\n  hodoo \
                            project create --name \"Nordwind site\" --template website \
                            --start 2026-11-02"
    )]
    Create(ProjectCreateArgs),

    /// Change a project; only the flags you pass change
    #[command(
        long_about = "Examples:\n  hodoo project update acme --visibility employees \
                            --end 2026-12-20\n  hodoo project update 49 --milestones \
                            --dependencies"
    )]
    Update {
        /// Project id or name
        #[arg(value_name = "PROJECT")]
        project: String,
        #[command(flatten)]
        fields: ProjectFieldArgs,
    },

    /// Delete a project, its tasks, its chatter and its milestones
    #[command(
        long_about = "Deletes the project and everything Odoo cascades from it. Asks \
                            first, unless -f/--force is passed or stdin is not a terminal (a \
                            script must pass -f on purpose).\n\nExamples:\n  hodoo project rm \
                            acme\n  hodoo project rm 49 -f"
    )]
    Rm {
        /// Project id or name
        #[arg(value_name = "PROJECT")]
        project: String,
        /// Do not ask; delete
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// The stages a project can be in (`project.project.stage`), one per project
    #[command(
        long_about = "A project's stage is a different thing from a task's stage: it is \
                            global, not per project, and a project points at exactly one. Odoo \
                            ships four and a team rarely adds a fifth.\n\nMoving a project into one \
                            is `project update <project> --stage <stage>`, not a command here: the \
                            stage is a property of the project.\n\nExamples:\n  hodoo project stages \
                            ls\n  hodoo project stages create --name \"On Hold\" --sequence 17"
    )]
    Stages {
        #[command(subcommand)]
        command: ProjectStagesCmd,
    },

    /// The task stages a project offers (`project.task.type`)
    #[command(
        long_about = "The kanban columns of a project: in order, how many open tasks each \
                            holds, and whether it is folded. A task can only sit in a stage its \
                            project offers, which is why creating one attaches it.\n\nExamples:\n  \
                            hodoo project task-stages ls acme\n  hodoo project task-stages create \
                            --name \"Review\" --project acme --sequence 40"
    )]
    TaskStages {
        #[command(subcommand)]
        command: TaskStagesCmd,
    },

    /// Attach a shared task stage to a project
    #[command(
        long_about = "A task can only sit in a stage its project offers, so a stage \
                            shared with another project has to be attached first.\n\nExamples:\n  \
                            hodoo project attach acme --task-stage Review"
    )]
    Attach {
        /// Project id or name
        #[arg(value_name = "PROJECT")]
        project: String,
        /// Task stage id or name; repeatable
        #[arg(long = "task-stage", value_name = "STAGE", required = true)]
        stages: Vec<String>,
    },

    /// Detach a task stage from a project
    #[command(
        long_about = "Removes the stage from this project only; tasks already in it \
                            stay where they are.\n\nExamples:\n  hodoo project detach acme \
                            --task-stage Review"
    )]
    Detach {
        /// Project id or name
        #[arg(value_name = "PROJECT")]
        project: String,
        /// Task stage id or name; repeatable
        #[arg(long = "task-stage", value_name = "STAGE", required = true)]
        stages: Vec<String>,
    },

    /// Post a message on a project's chatter
    #[command(
        long_about = "Plain text; Odoo escapes it. --internal keeps it to internal \
                            users.\n\nExamples:\n  hodoo project comment acme --body \"Kickoff \
                            done\" --internal"
    )]
    Comment {
        /// Project id or name
        #[arg(value_name = "PROJECT")]
        project: String,
        /// The message
        #[arg(long, value_name = "TEXT")]
        body: String,
        /// A note only internal users see, instead of a comment
        #[arg(long)]
        internal: bool,
    },
}

/// Filters for `project ls`.
#[derive(Debug, Args)]
pub struct ProjectLsArgs {
    /// Only projects whose name contains this
    #[arg(value_name = "TEXT")]
    pub name: Option<String>,
    /// Only projects for this customer (id or name)
    #[arg(long, value_name = "CONTACT")]
    pub customer: Option<String>,
    /// Only projects this user manages (id, login or name)
    #[arg(long, value_name = "USER")]
    pub manager: Option<String>,
    /// Only projects in this stage (id or name)
    #[arg(long, value_name = "STAGE")]
    pub stage: Option<String>,
    /// Only projects carrying this tag; repeatable
    #[arg(long = "tag", value_name = "TAG")]
    pub tags: Vec<String>,
    /// Only projects I manage
    #[arg(long)]
    pub mine: bool,
    /// Odoo order string, e.g. "name desc"
    #[arg(long, value_name = "ORDER")]
    pub order: Option<String>,
    /// Maximum rows; 0 means all
    #[arg(short = 'l', long, default_value_t = 50)]
    pub limit: u32,
    /// Rows to skip
    #[arg(long, default_value_t = 0)]
    pub offset: u32,
}

/// Fields shared by `project create` and `project update`.
#[derive(Debug, Args, Default)]
pub struct ProjectFieldArgs {
    /// Who may reach the project: followers, invited-users, employees, portal
    #[arg(long, value_name = "VISIBILITY")]
    pub visibility: Option<VisibilityArg>,
    /// Customer (id or name)
    #[arg(long, value_name = "CONTACT")]
    pub customer: Option<String>,
    /// Project manager (id, login or name)
    #[arg(long, value_name = "USER")]
    pub manager: Option<String>,
    /// Project stage, e.g. "In Progress"
    #[arg(long, value_name = "STAGE")]
    pub stage: Option<String>,
    /// Description, as HTML
    #[arg(long, value_name = "HTML")]
    pub description: Option<String>,
    /// Tag (name or id); repeatable: replaces the project's tags
    #[arg(long = "tag", value_name = "TAG")]
    pub tags: Vec<String>,
    /// Start date, e.g. 2026-11-02
    #[arg(long, value_name = "DATE")]
    pub start: Option<String>,
    /// End date, e.g. 2026-12-20
    #[arg(long, value_name = "DATE")]
    pub end: Option<String>,
    /// Use milestones on this project
    #[arg(long, overrides_with = "no_milestones")]
    pub milestones: bool,
    /// Turn milestones off
    #[arg(long)]
    pub no_milestones: bool,
    /// Use task dependencies ("blocked by") on this project
    #[arg(long, overrides_with = "no_dependencies")]
    pub dependencies: bool,
    /// Turn task dependencies off
    #[arg(long)]
    pub no_dependencies: bool,
}

/// Project creation.
#[derive(Debug, Args)]
pub struct ProjectCreateArgs {
    /// Project name
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Copy this project template instead of starting empty
    #[arg(long, value_name = "TEMPLATE")]
    pub template: Option<String>,
    #[command(flatten)]
    pub fields: ProjectFieldArgs,
}

/// Project visibility.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum VisibilityArg {
    /// Invited internal users only
    Followers,
    /// Invited internal users and portal users
    InvitedUsers,
    /// All internal users
    Employees,
    /// All internal users, plus invited portal users
    Portal,
}

/// Task commands.
#[derive(Debug, Subcommand)]
pub enum TaskCmd {
    /// List tasks
    #[command(
        long_about = "Lists tasks as a table, soonest deadline first. Filters take \
                            names or ids.\n\nExamples:\n  hodoo task ls --project acme --open\n  \
                            hodoo task ls --mine --overdue\n  hodoo task ls --due-before +7d\n  \
                            hodoo task ls --tag urgent --json"
    )]
    Ls(TaskLsArgs),

    /// Show one task: state, who, when, what blocks it, and recent chatter
    #[command(
        long_about = "Everything about one task on one screen, including what it waits \
                            on and the last few chatter entries.\n\nExamples:\n  hodoo task show \
                            31\n  hodoo task show \"Build the CMS integration\""
    )]
    Show {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
    },

    /// Create a task
    #[command(
        long_about = "Odoo assigns the calling user to a new task unless an assignee is \
                            given. --depends-on makes the task wait for another one.\n\nExamples:\n  \
                            hodoo task create --name \"Write the copy\" --project acme --assignee \
                            me --due +7d --priority high --tag client\n  hodoo task create --name \
                            \"Audit\" --project acme --depends-on \"Homepage build\""
    )]
    Create(TaskCreateArgs),

    /// Change a task; only the flags you pass change
    #[command(
        long_about = "Examples:\n  hodoo task update 31 --priority urgent --due today\n  \
                            hodoo task update 31 --unassign\n  hodoo task update 31 --tag \
                            urgent"
    )]
    Update {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
        #[command(flatten)]
        fields: TaskFieldArgs,
    },

    /// Mark a task done
    #[command(long_about = "Examples:\n  hodoo task done 31")]
    Done {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
    },

    /// Cancel a task
    #[command(long_about = "Examples:\n  hodoo task cancel 31")]
    Cancel {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
    },

    /// Put a task back in progress
    #[command(long_about = "Examples:\n  hodoo task reopen 31")]
    Reopen {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
    },

    /// Move a task to another stage
    #[command(long_about = "Examples:\n  hodoo task move 31 --stage Review")]
    Move {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
        /// Where to move it: a stage name within its project, or an id
        #[arg(long, value_name = "STAGE", required = true)]
        stage: String,
    },

    /// Post a message on a task's chatter
    #[command(
        long_about = "Plain text; Odoo escapes it. --internal keeps it to internal \
                            users.\n\nExamples:\n  hodoo task comment 31 --body \"blocked on the \
                            certificate\" --internal"
    )]
    Comment {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
        /// The message
        #[arg(long, value_name = "TEXT")]
        body: String,
        /// A note only internal users see, instead of a comment
        #[arg(long)]
        internal: bool,
    },

    /// Show a task's chatter
    #[command(long_about = "Examples:\n  hodoo task messages 31 --limit 5")]
    Messages {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
        /// How many, newest last
        #[arg(short = 'l', long, default_value_t = 20)]
        limit: u32,
    },

    /// What a task waits on, and what waits on it
    #[command(long_about = "Examples:\n  hodoo task deps 31")]
    Deps {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
    },

    /// Delete a task
    #[command(
        long_about = "Asks first unless -f/--force is passed.\n\nExamples:\n  hodoo task rm \
                            31 -f"
    )]
    Rm {
        /// Task id or name
        #[arg(value_name = "TASK")]
        task: String,
        /// Do not ask; delete
        #[arg(short = 'f', long)]
        force: bool,
    },
}

/// Filters for `task ls`.
#[derive(Debug, Args)]
pub struct TaskLsArgs {
    /// Only tasks of this project
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
    /// Only tasks in this stage
    #[arg(long, value_name = "STAGE")]
    pub stage: Option<String>,
    /// Only tasks assigned to this user
    #[arg(long, value_name = "USER")]
    pub assignee: Option<String>,
    /// Only tasks assigned to me
    #[arg(long)]
    pub mine: bool,
    /// Only tasks that are neither done nor canceled
    #[arg(long)]
    pub open: bool,
    /// Only tasks in this state
    #[arg(long, value_name = "STATE")]
    pub state: Option<StateArg>,
    /// Only tasks whose name contains this
    #[arg(value_name = "TEXT")]
    pub name: Option<String>,
    /// Only tasks carrying this tag; repeatable
    #[arg(long = "tag", value_name = "TAG")]
    pub tags: Vec<String>,
    /// Only tasks due before this: today, +7d, 2026-12-01
    #[arg(long, value_name = "WHEN")]
    pub due_before: Option<String>,
    /// Only tasks whose deadline has passed
    #[arg(long)]
    pub overdue: bool,
    /// Only subtasks of this task
    #[arg(long, value_name = "TASK")]
    pub parent: Option<String>,
    /// Odoo order string, e.g. "priority desc"
    #[arg(long, value_name = "ORDER")]
    pub order: Option<String>,
    /// Maximum rows; 0 means all
    #[arg(short = 'l', long, default_value_t = 50)]
    pub limit: u32,
    /// Rows to skip
    #[arg(long, default_value_t = 0)]
    pub offset: u32,
}

/// Fields shared by `task create` and `task update`.
#[derive(Debug, Args, Default)]
pub struct TaskFieldArgs {
    /// Project (id or name)
    #[arg(long, value_name = "PROJECT")]
    pub project: Option<String>,
    /// Stage (id, or a name within the project)
    #[arg(long, value_name = "STAGE")]
    pub stage: Option<String>,
    /// State
    #[arg(long, value_name = "STATE")]
    pub state: Option<StateArg>,
    /// Priority: low, medium, high, urgent
    #[arg(long, value_name = "PRIORITY")]
    pub priority: Option<PriorityArg>,
    /// Assignee (id, login or name); repeatable: replaces the assignees
    #[arg(long = "assignee", value_name = "USER")]
    pub assignees: Vec<String>,
    /// Drop every assignee
    #[arg(long, conflicts_with = "assignees")]
    pub unassign: bool,
    /// Customer contact (id or name)
    #[arg(long, value_name = "CONTACT")]
    pub customer: Option<String>,
    /// Deadline: today, +3d, 2026-12-01 or "2026-12-01 09:00"
    #[arg(long, value_name = "WHEN")]
    pub due: Option<String>,
    /// Planned hours
    #[arg(long, value_name = "HOURS")]
    pub hours: Option<f64>,
    /// Tag (name or id); repeatable: replaces the task's tags
    #[arg(long = "tag", value_name = "TAG")]
    pub tags: Vec<String>,
    /// Drop every tag
    #[arg(long, conflicts_with = "tags")]
    pub clear_tags: bool,
    /// Make this a subtask of that task (id or name)
    #[arg(long, value_name = "TASK")]
    pub parent: Option<String>,
    /// Wait for that task (id or name); repeatable
    #[arg(long = "depends-on", value_name = "TASK")]
    pub depends_on: Vec<String>,
    /// Milestone (id, or a name within the project)
    #[arg(long, value_name = "MILESTONE")]
    pub milestone: Option<String>,
    /// Description, as HTML
    #[arg(long, value_name = "HTML")]
    pub description: Option<String>,
}

/// Task creation.
#[derive(Debug, Args)]
pub struct TaskCreateArgs {
    /// Task title
    #[arg(long, value_name = "NAME")]
    pub name: String,
    #[command(flatten)]
    pub fields: TaskFieldArgs,
}

/// Task states, as Odoo spells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StateArg {
    /// Being worked on
    InProgress,
    /// Waiting for changes
    ChangesRequested,
    /// Approved, not started
    Approved,
    /// Finished
    Done,
    /// Dropped
    Canceled,
    /// Waiting, usually because something it depends on is open
    Waiting,
}

/// Task priorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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

/// Task stage commands (`project.task.type`).
#[derive(Debug, Subcommand)]
pub enum TaskStagesCmd {
    /// List a project's task stages, with the open work in each
    #[command(
        long_about = "Examples:\n  hodoo project task-stages ls acme\n  hodoo project \
                            task-stages ls 49"
    )]
    Ls {
        /// Project id or name
        #[arg(value_name = "PROJECT")]
        project: String,
    },

    /// Create a task stage, attached to a project by default
    #[command(
        long_about = "A stage belongs to a project: without --project it is created \
                            unattached and no task can use it, so pass the project unless you \
                            mean to share the stage later.\n\nExamples:\n  hodoo project task-stages \
                            create --name \"Review\" --project acme --sequence 40"
    )]
    Create {
        /// Stage name
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Attach the new stage to this project (id or name)
        #[arg(long, value_name = "PROJECT")]
        project: Option<String>,
        /// Sort order within the kanban
        #[arg(long, value_name = "N")]
        sequence: Option<i64>,
        /// Fold the stage in the kanban, for the end of the line
        #[arg(long)]
        fold: bool,
    },

    /// Rename, reorder or fold a task stage, in every project that offers it
    #[command(
        long_about = "Only the flags you pass change. A task stage is one record shared \
                            between the projects that offer it, so a rename shows up in all of \
                            them.\n\nExamples:\n  hodoo project task-stages update \"Review\" --name \
                            \"Code review\" --sequence 45\n  hodoo project task-stages update \"Done\" \
                            --fold"
    )]
    Update {
        /// Task stage id or name
        #[arg(value_name = "STAGE")]
        stage: String,
        /// New name
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// New sort order
        #[arg(long, value_name = "N")]
        sequence: Option<i64>,
        /// Fold the stage in the kanban
        #[arg(long, conflicts_with = "unfold")]
        fold: bool,
        /// Unfold it again
        #[arg(long)]
        unfold: bool,
    },

    /// Delete a task stage; refused while a task is in it
    #[command(
        long_about = "Odoo refuses to delete a stage a task sits in, so this says so before \
                            asking: move those tasks with `hodoo task move <task> --stage <other>` \
                            first, or archive the stage instead. A delete detaches the stage from \
                            every project that offered it; `project detach` removes it from one.\n\n\
                            Examples:\n  hodoo project task-stages rm \"Review\"\n  hodoo project \
                            task-stages rm \"Review\" -f"
    )]
    Rm {
        /// Task stage id or name
        #[arg(value_name = "STAGE")]
        stage: String,
        /// Do not ask; delete
        #[arg(short = 'f', long)]
        force: bool,
    },
}

/// Project stage commands (`project.project.stage`).
#[derive(Debug, Subcommand)]
pub enum ProjectStagesCmd {
    /// List the project stages, with how many projects sit in each
    #[command(
        long_about = "Examples:\n  hodoo project stages ls\n  hodoo project stages ls --limit 0"
    )]
    Ls {
        /// Maximum rows; 0 means all
        #[arg(short = 'l', long, default_value_t = 0)]
        limit: u32,
    },

    /// Create a project stage; every project can use it
    #[command(
        long_about = "A project stage is global, so there is nothing to attach and no \
                            project uses it until `project update <project> --stage <stage>` says \
                            so. Odoo ships To Do, In Progress, Done and Cancelled; a team rarely \
                            needs a fifth.\n\nExamples:\n  hodoo project stages create --name \
                            \"On Hold\" --sequence 17"
    )]
    Create {
        /// Stage name
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Sort order within the kanban
        #[arg(long, value_name = "N")]
        sequence: Option<i64>,
        /// Fold the stage in the kanban, for the end of the line
        #[arg(long)]
        fold: bool,
    },

    /// Rename, reorder or fold a project stage
    #[command(
        long_about = "Only the flags you pass change.\n\nExamples:\n  hodoo project stages update \
                            \"In Progress\" --name \"WIP\" --sequence 20\n  hodoo project stages \
                            update \"On Hold\" --fold"
    )]
    Update {
        /// Project stage id or name
        #[arg(value_name = "STAGE")]
        stage: String,
        /// New name
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// New sort order
        #[arg(long, value_name = "N")]
        sequence: Option<i64>,
        /// Fold the stage in the kanban
        #[arg(long, conflicts_with = "unfold")]
        fold: bool,
        /// Unfold it again
        #[arg(long)]
        unfold: bool,
    },

    /// Delete a project stage; refused while a project is in it
    #[command(
        long_about = "Odoo refuses to delete a stage a project is in, so this says so \
                            before asking: move the projects with `project update <project> --stage \
                            <other>` first, or archive the stage instead.\n\nExamples:\n  hodoo \
                            project stages rm \"On Hold\"\n  hodoo project stages rm \"On Hold\" -f"
    )]
    Rm {
        /// Project stage id or name
        #[arg(value_name = "STAGE")]
        stage: String,
        /// Do not ask; delete
        #[arg(short = 'f', long)]
        force: bool,
    },
}

/// Milestone commands.
#[derive(Debug, Subcommand)]
pub enum MilestoneCmd {
    /// List a project's milestones
    #[command(long_about = "Examples:\n  hodoo milestone ls --project acme")]
    Ls {
        /// Project (id or name)
        #[arg(long, value_name = "PROJECT")]
        project: String,
    },

    /// Create a milestone
    #[command(
        long_about = "The project has to allow milestones; `hodoo project update \
                            --milestones` turns that on.\n\nExamples:\n  hodoo milestone create \
                            --project acme --name \"Beta\" --due 2026-12-01"
    )]
    Create {
        /// Project (id or name)
        #[arg(long, value_name = "PROJECT")]
        project: String,
        /// Milestone name
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Deadline, e.g. 2026-12-01
        #[arg(long, value_name = "WHEN")]
        due: Option<String>,
    },

    /// Mark a milestone reached, or unreached
    #[command(
        long_about = "Examples:\n  hodoo milestone reached 12\n  hodoo milestone reached 12 \
                            --undo"
    )]
    Reached {
        /// Milestone id
        #[arg(value_name = "MILESTONE")]
        milestone: i64,
        /// Mark it unreached again
        #[arg(long)]
        undo: bool,
    },

    /// Delete a milestone
    #[command(
        long_about = "Asks first unless -f/--force is passed.\n\nExamples:\n  hodoo milestone rm 12 -f"
    )]
    Rm {
        /// Milestone id
        #[arg(value_name = "MILESTONE")]
        milestone: i64,
        /// Do not ask; delete
        #[arg(short = 'f', long)]
        force: bool,
    },
}

/// Tag commands.
#[derive(Debug, Subcommand)]
pub enum TagCmd {
    /// List tags
    #[command(
        long_about = "Tags are created on demand by `--tag` on a task or project, so \
                            there is nothing to create here.\n\nExamples:\n  hodoo tag ls"
    )]
    Ls,
}

/// `hodoo board`.
#[derive(Debug, Args)]
pub struct BoardArgs {
    /// Project (id or name); omit it to see every project
    #[arg(value_name = "PROJECT")]
    pub project: Option<String>,
    /// Only tasks assigned to me
    #[arg(long)]
    pub mine: bool,
    /// Include finished tasks
    #[arg(long)]
    pub all: bool,
}

/// `hodoo call`.
#[derive(Debug, Args)]
pub struct CallArgs {
    /// Odoo model, e.g. project.task
    #[arg(value_name = "MODEL")]
    pub model: String,
    /// Method, e.g. search_read
    #[arg(value_name = "METHOD")]
    pub method: String,
    /// Named arguments, as a JSON object
    #[arg(long, value_name = "JSON")]
    pub body: Option<String>,
    /// Record ids to run it on, comma separated
    #[arg(long, value_delimiter = ',', value_name = "ID,ID")]
    pub ids: Vec<i64>,
}

/// `hodoo completions`.
#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Which shell to print a completion script for
    #[arg(value_name = "SHELL")]
    pub shell: Shell,
}
