# hodoo — Rust client + CLI for Odoo 19 projects (design)

Date: 2026-09-28
Status: approved design, ready for implementation planning

## Goal

A Rust library (`hodoo`) and CLI (`hodoo`) that drive Odoo 19 Community's project
management over the JSON-2 API: projects, tasks, stages, tags, milestones, subtasks,
dependencies, and chatter. The library is the product; the CLI is a thin consumer of it.
A Topcoat web app (`hodoo-web`) will become a third consumer later, so the library must
be runtime-friendly, cheap to clone, and free of global state.

## Non-goals

- No MCP server.
- No code generation from Odoo's `/doc` index (see Rejected alternatives).
- No Odoo-side module, no Odoo database access, no XML-RPC or `/jsonrpc`.
- No typed surface for models outside projects (contacts, CRM, calendar use `call`).
- No TUI, no table renderer, no config file format.

## Verified environment facts

Everything below was checked against the Odoo 19.0 source, not assumed.

**The endpoint.** `odoo/addons/rpc/controllers/json2.py` (module `rpc`,
`auto_install: True`, Community, no license gate) registers:

```
POST /json/2/<__model__>/<__method__>     auth='bearer', type='json2'
```

- Body is a JSON object: `ids` (list of ints, omitted for `@api.model` methods),
  `context` (object), plus the method's kwargs by name. Positional args are impossible.
- `ids` with an `@api.model` method is a `422`. Unknown kwargs are a `422` (the handler
  binds `inspect.signature(func)`). Unknown model or non-public method is a `404`
  (methods starting with `_` are not callable).
- Any non-POST verb on `/json/2/...` hits a catch-all that returns `404` with
  `Did you mean POST /json/2/<model>/<method>?`.
- Success: `200` with the method's return value as JSON. A returned recordset is
  reduced to its ids, so `create` returns an id (int) or list of ids.
- Failure: 4xx/5xx with `{name, message, arguments, context, debug}` where `name` is
  the exception's fully qualified Python name and `debug` is a traceback.
- Every call is its own transaction; nothing chains.
- `X-Odoo-Database` is optional and only needed when one server serves several
  databases behind a domain. Both this deployment (`db_name = odoo`, `list_db = False`)
  and the live instance are single-database, so it is not sent unless `db` is configured.
- API keys are per user, created at Preferences > Account Security > New API Key, valid
  at most 3 months, shown once. It is the only auth surface: there is no password login
  over JSON-2, so the CLI and the future web app each need a key.

**The live instance** (`https://odoo.jsmx.org`, Odoo 19.0 final, confirmed with
`GET /web/version`): the `/json/2` route family is live (a GET falls through to the
`Did you mean POST` handler), and the served certificate is a real Google Trust Services
one for `jsmx.org` / `*.jsmx.org` (valid to 2026-11-17). A self-signed certificate is
still what `configs/nginx-odoo.conf` generates by default, so `insecure_cert` stays in
the config for other boxes.

**Method discovery** is available at `/doc-bearer/index.json` and
`/doc-bearer/<model>.json` (`api_doc` module, `auto_install: True`, `auth='bearer'`).
Both raise `AccessError` unless the key's user is in `api_doc.group_allow_doc`, which is
implied by `base.group_system` (Settings). Used only by the drift test, not by the client.

## Layout and toolchain

Under this repository, as a two-crate workspace so `hodoo-web` joins later by adding a
member:

```
hodoo/
  Cargo.toml                 [workspace], members = ["crates/*"]
  crates/hodoo/              library crate, package name `hodoo`
    src/lib.rs               re-exports, Config, Client
    src/http.rs              transport: one POST, retry, error mapping
    src/error.rs             Error, Result
    src/id.rs                Id<T> newtype and marker types
    src/de.rs                Odoo's many2one/x2many shapes
    src/project.rs           Project, ProjectFields, ProjectFilter, Projects
    src/task.rs              Task, TaskFields, TaskFilter, Tasks, priority/state
    src/stage.rs             TaskStage, Stages
    src/milestone.rs         Milestone, Milestones
    src/tag.rs               Tag, Tags
  crates/hodoo-cli/          binary crate, package name `hodoo-cli`, binary `hodoo`
    src/main.rs              clap tree, env/config resolution, output, exit codes
```

- Edition 2024, `rust-version = "1.85"`. Local toolchain is rustc/cargo 1.98.1 under
  `~/.cargo/bin`, which is **not on `PATH`** in a plain shell: invoke `~/.cargo/bin/cargo`
  or export `PATH="$HOME/.cargo/bin:$PATH"` first.
- Dependencies: `reqwest` (`json`, `rustls-tls-native-roots`, default features off),
  `serde` (derive), `serde_json`, `thiserror`, `chrono` (serde). CLI adds `clap`
  (derive), `tokio` (`rt-multi-thread`, `macros`), `anyhow`. Dev: `wiremock`,
  `tokio` (`macros`, `rt`).
- `[lints]` in both crates: `unsafe_code = "forbid"`, `missing_docs = "warn"`,
  clippy `unwrap_used` and `expect_used` = "warn".

## Public API

### Config and Client

```rust
pub struct Config {
    base_url: reqwest::Url,   // parsed and validated in Config::new
    api_key: String,
    db: Option<String>,
    timeout: Duration,        // default 30s
    insecure_cert: bool,      // default false
    retry_reads: bool,        // default true
}

impl Config {
    pub fn new(base_url: impl AsRef<str>, api_key: impl Into<String>) -> Result<Config>;
    pub fn with_db(self, db: impl Into<String>) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
    pub fn insecure_cert(self, yes: bool) -> Self;
    pub fn retry_reads(self, yes: bool) -> Self;
}

impl Debug for Config { /* redacts api_key as "<redacted>" */ }

#[derive(Clone)]
pub struct Client { /* reqwest::Client + Arc<Config> */ }

impl Client {
    pub fn new(cfg: Config) -> Result<Self>;

    /// Raw JSON-2 call: the escape hatch, and the only place HTTP happens.
    pub async fn call(&self, model: &str, method: &str, body: Value) -> Result<Value>;

    /// `call` plus deserialization into a typed value.
    pub async fn call_as<T: DeserializeOwned>(&self, model: &str, method: &str, body: Value)
        -> Result<T>;

    /// GET /web/version, unauthenticated. Useful before a key exists.
    pub async fn version(&self) -> Result<String>;

    pub fn projects(&self) -> Projects<'_>;
    pub fn tasks(&self) -> Tasks<'_>;
    pub fn stages(&self) -> Stages<'_>;
    pub fn milestones(&self) -> Milestones<'_>;
    pub fn tags(&self) -> Tags<'_>;
}
```

`Client` is `Clone + Send + Sync` and holds no mutable state, so a Topcoat app registers
it once with `.app_context(client)`; a future multi-user mode constructs one per request
from that user's key. The scope accessors borrow `&self` and are zero-cost.

### Ids

Odoo ids are integers, and mixing a project id into a task field is the most likely
mistake a typed client can catch for free. Ids are a phantom newtype:

```rust
pub struct Id<T>(i64, PhantomData<fn() -> T>);   // fn() -> T keeps Send/Sync and covariance

impl<T> Id<T> {
    pub fn new(raw: i64) -> Self;
    pub fn get(self) -> i64;
}
// impl Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display
// impl From<i64>, Serialize (as a bare number), Deserialize (bare number,
// or Odoo's [id, display_name] pair)

pub struct Project;   pub struct ProjectStage;  pub struct Task;
pub struct TaskStage; pub struct Milestone;     pub struct Tag;
pub struct User;      pub struct Partner;

pub type ProjectId      = Id<Project>;
pub type ProjectStageId = Id<ProjectStage>;   // project.project.stage, not a task stage
pub type TaskId         = Id<Task>;
pub type TaskStageId    = Id<TaskStage>;      // project.task.type
pub type MilestoneId    = Id<Milestone>;
pub type TagId          = Id<Tag>;
pub type UserId         = Id<User>;
pub type PartnerId      = Id<Partner>;
```

The marker types live beside the model they name (`hodoo::project::Project`) and are
re-exported at the crate root for the aliases.

### Deserialization helpers (`de`)

Odoo's JSON shapes are irregular; they are handled in one module instead of per field:

- many2one is `null`, `false`, a bare int in some paths, or `[id, "Display Name"]`.
  `de::opt_id` accepts all four and is applied with
  `#[serde(default, deserialize_with = "crate::de::opt_id")]`.
- x2many is a list of ints, occasionally a list of `[id, name]` pairs. `de::id_list`
  accepts both.
- Every read struct field carries `#[serde(default)]` so an Odoo field we did not
  anticipate, or a field hidden by the user's groups, cannot fail the whole response.

Dates: Odoo returns naive UTC strings (`"2026-09-28 14:30:00"` for datetimes,
`"2026-09-28"` for dates) with no offset. The library deserializes datetimes to
`chrono::DateTime<Utc>` and dates to `chrono::NaiveDate` through explicit format
helpers, and serializes back in the same format. Handing raw strings to the caller would
push a silent timezone assumption onto every consumer.

### Field structs (writes)

One struct per model, public `Option` fields, `Default`, and a constructor that fills in
whatever Odoo requires. `None` means "not sent"; `Some(vec![])` means "clear this
x2many" (sent as `[[6, 0, []]]`), which a raw `json!` body cannot express as cleanly.

```rust
#[derive(Clone, Debug, Default, Serialize)]
pub struct TaskFields {
    pub name: Option<String>,
    pub project: Option<ProjectId>,          // -> project_id
    pub stage: Option<TaskStageId>,          // -> stage_id
    pub state: Option<TaskState>,            // -> state
    pub priority: Option<Priority>,          // -> priority
    pub assignees: Option<Vec<UserId>>,      // -> user_ids
    pub customer: Option<PartnerId>,         // -> partner_id
    pub deadline: Option<DateTime<Utc>>,     // -> date_deadline
    pub allocated_hours: Option<f64>,        // -> allocated_hours
    pub tags: Option<Vec<TagId>>,            // -> tag_ids
    pub parent: Option<TaskId>,              // -> parent_id
    pub milestone: Option<MilestoneId>,      // -> milestone_id
    pub description: Option<String>,         // -> description (HTML)
}

impl TaskFields { pub fn new(name: impl Into<String>) -> Self; }
impl From<TaskFields> for serde_json::Value;
```

`ProjectFields`, `StageFields`, `MilestoneFields` follow the same shape. `StageFields`
is `name` (required), `sequence`, `fold`, `color`, `active`. `MilestoneFields` is `name`
(required), `project` (required), `deadline`, `sequence`. Callers who want a field we do
not model use `call` with their own JSON; the structs are a convenience, not a fence.

Ergonomic construction is struct-update syntax, not a builder zoo:

```rust
let fields = TaskFields { name: Some("Ship it".into()), project: Some(pid), ..Default::default() };
```

Value enums, checked against the v19 source:

```rust
pub enum TaskState { InProgress, ChangesRequested, Approved, Done, Canceled, Waiting }
// -> "01_in_progress", "02_changes_requested", "03_approved", "1_done",
//    "1_canceled", "04_waiting_normal". Done/Canceled are the closed states.
impl TaskState { pub fn is_closed(self) -> bool; }

pub enum Priority { Low, Normal, High, VeryHigh }   // 0..3
```

### Read structs and filters

Read structs carry the fields the library requests and are plain data (`#[serde(default)]`
throughout, `Deserialize`). Only an explicit field list is requested, never Odoo's
"all fields" default, which would drag every computed column across the wire.

```rust
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub project: Option<ProjectId>,
    pub stage: Option<TaskStageId>,
    pub state: TaskState,
    pub priority: Priority,
    pub assignees: Vec<UserId>,     // user_ids
    pub customer: Option<PartnerId>,
    pub deadline: Option<DateTime<Utc>>,
    pub date_assign: Option<DateTime<Utc>>,
    pub date_last_stage_update: Option<DateTime<Utc>>,
    pub allocated_hours: Option<f64>,
    pub tags: Vec<TagId>,
    pub parent: Option<TaskId>,
    pub milestone: Option<MilestoneId>,
    pub description: Option<String>,
    pub is_closed: bool,
}
```

`Project` covers `project.project`: `id`, `name`, `description`, `active`, `customer`
(`partner_id`), `manager` (`user_id`), `stage` (`stage_id`, a project stage),
`date_start`, `date` (deadline), `privacy_visibility`, `tags`, `task_stages` (`type_ids`),
`task_count`, `open_task_count`.

Filters are typed, and translate to both domain and keyword arguments. `TaskFilter`:

```rust
#[derive(Clone, Debug, Default)]
pub struct TaskFilter {
    pub project: Option<ProjectId>,
    pub stage: Option<TaskStageId>,
    pub assignee: Option<UserId>,
    pub open_only: bool,
    pub name_contains: Option<String>,
    pub tags: Vec<TagId>,
    pub deadline_before: Option<DateTime<Utc>>,
    pub order: Option<String>,     // e.g. "priority desc, date_deadline"
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
```

No filter means no limit sent, which is Odoo's "return everything"; callers that care
set `limit`. The CLI sets one by default.

`ProjectFilter` is the same shape minus the task-only fields: `name_contains`, `customer`
(`partner_id`), `manager` (`user_id`), `stage` (`stage_id`, a `ProjectStageId`),
`active_only`, `tags`, `order`, `limit`, `offset`. `StageFilter` is only `project`
(`project_ids`) plus `active_only`, and is what `Stages::list` takes: passing a project
is a domain, not a different method.

### Scope methods

```rust
impl Projects<'_> {
    pub async fn search(&self, filter: ProjectFilter) -> Result<Vec<Project>>;
    pub async fn count(&self, filter: ProjectFilter) -> Result<u64>;
    pub async fn get(&self, id: ProjectId) -> Result<Project>;
    pub async fn create(&self, fields: ProjectFields) -> Result<ProjectId>;
    pub async fn update(&self, id: ProjectId, fields: ProjectFields) -> Result<()>;
    pub async fn delete(&self, id: ProjectId) -> Result<()>;
    /// Attach task stages to a project, i.e. write `type_ids`.
    pub async fn attach_stages(&self, id: ProjectId, stages: &[TaskStageId]) -> Result<()>;
}

impl Tasks<'_> {
    pub async fn search(&self, filter: TaskFilter) -> Result<Vec<Task>>;
    pub async fn count(&self, filter: TaskFilter) -> Result<u64>;
    pub async fn get(&self, id: TaskId) -> Result<Task>;
    /// Creates the task and, when no stage is given, the project's default stage.
    pub async fn create(&self, fields: TaskFields) -> Result<TaskId>;
    pub async fn update(&self, id: TaskId, fields: TaskFields) -> Result<()>;
    pub async fn delete(&self, id: TaskId) -> Result<()>;
    pub async fn comment(&self, id: TaskId, body: &str, internal: bool) -> Result<()>;
}

impl Stages<'_> {
    pub async fn list(&self, filter: StageFilter) -> Result<Vec<TaskStage>>;
    pub async fn create(&self, fields: StageFields) -> Result<TaskStageId>;
    /// Create the stage and attach it to the project in one step.
    pub async fn create_in(&self, project: ProjectId, fields: StageFields)
        -> Result<TaskStageId>;
}

/// `project.project.stage`, a different model: the stages a *project* moves
/// through. Global, so there is nothing to attach.
impl ProjectStages<'_> {
    pub async fn list(&self, filter: ProjectStageFilter) -> Result<Vec<ProjectStage>>;
    pub async fn create(&self, fields: StageFields) -> Result<ProjectStageId>;
    pub async fn update(&self, id: ProjectStageId, fields: StageFields) -> Result<()>;
    /// Odoo refuses this while a project is in the stage.
    pub async fn delete(&self, id: ProjectStageId) -> Result<()>;
}

impl Milestones<'_> {
    pub async fn list(&self, project: ProjectId) -> Result<Vec<Milestone>>;
    pub async fn create(&self, fields: MilestoneFields) -> Result<MilestoneId>;
    /// `project.milestone.toggle_is_reached(is_reached)`.
    pub async fn set_reached(&self, id: MilestoneId, reached: bool) -> Result<()>;
}

impl Tags<'_> {
    pub async fn list(&self) -> Result<Vec<Tag>>;
    /// Find by exact name, otherwise `name_create`. Project tags are shared, not
    /// per project, so this is the lazy path for "tag this task with X".
    pub async fn ensure(&self, name: &str) -> Result<TagId>;
}
```

`is_closed` on `Task` is Odoo's own computed field, so `open_only` filters by it rather
than hand-listing the open states.

### Errors

```rust
#[non_exhaustive]
pub enum Error {
    /// Config rejected before any request: bad URL, empty key, malformed body.
    Config { message: String },
    /// Transport-level failure, including the read-only retry being exhausted.
    Transport { source: reqwest::Error },
    /// Odoo answered with an error object.
    Odoo { status: u16, name: String, message: String, arguments: Vec<Value>, debug: Option<String> },
    /// A non-JSON error body, e.g. nginx's 502 page.
    UnexpectedResponse { status: u16, body: String },
    /// A response body that did not match the expected Rust type.
    Decode { source: serde_json::Error, body: Value },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn status(&self) -> Option<u16>;
    pub fn is_unauthorized(&self) -> bool;   // 401 or name werkzeug.exceptions.Unauthorized
    pub fn is_not_found(&self) -> bool;      // 404, e.g. unknown model or method
}
```

`Error::Odoo` keeps `name`, `status` and `debug`, so a CLI or web app can log the
traceback without re-parsing JSON. `debug` is never printed by the CLI unless
`--verbose`, because it contains server paths.

## Odoo v19 semantics the client encodes

Verified against the 19.0 source; each one would otherwise be found by trial and error.

| # | Behavior | Consequence in `hodoo` |
|---|---|---|
| 1 | `project.task.stage_id` points at `project.task.type`, while `project.project.stage_id` points at `project.project.stage`. Two unrelated "stage" concepts. | Distinct types: `TaskStageId` for tasks, `ProjectStageId` for projects; no method accepts a bare int, so a task stage can never be assigned to a project. |
| 2 | `project.task.type.project_ids` is the m2m linking stages to projects, and task `stage_id`'s domain is `[('project_ids', '=', project_id)]`. | `Stages::create_in` attaches on creation; `Projects::attach_stages` exists for the raw case. A stage created without a project is invisible to every task. |
| 3 | `project.project.create` does **not** create task stages; only `name_create` adds a "New" stage. | `Projects::create` alone yields a project with empty `type_ids`, so `Tasks::create` documents that the caller needs a stage attached first, and the CLI's `project create` prints `type_ids`. |
| 4 | `project.project.create` auto-picks `project.stage_id` when the user has `project.group_project_stages`. | Nothing to send; documented, and never asserted by tests. |
| 5 | `project.task.create` with `user_ids` also appends the calling user and stamps `date_assign`. | Documented on `TaskFields::assignees`; live tests expect the caller in `assignees`. |
| 6 | `project.task.state` closed values are `1_done` and `1_canceled` (not `done`/`cancel`). | `TaskState::Done` and `TaskState::Canceled` map to exactly those strings. |
| 7 | A task with no `project_id` is private, and `stage_id` is forced to `false` when no project is set. | `TaskFields::stage` without `project` is accepted but the stage is dropped server-side; documented. |
| 8 | `project.milestone.project_id` is required, and milestones only function when `project.allow_milestones` is true. | `Milestones::create` requires the project id; `MilestoneFields` documents the flag. |
| 9 | `project.task.description` is HTML (`Html` field, `sanitize_attributes=False`). | Typed as `Option<String>` with a doc comment saying HTML, never escaped or "cleaned" client-side. |
| 10 | Chatter: `message_post(body=..., subtype_xmlid=...)` on `project.task` / `project.project`, both `mail.thread` models. | `comment(id, body, internal)` sends `mail.mt_note` when internal, `mail.mt_comment` otherwise. |
| 11 | `search_read` returns every field when `fields` is omitted. | The library always sends an explicit `fields` list per model. |
| 12 | Odoo wraps errors in `{name, message, arguments, context, debug}` with a Python exception name. | `Error::Odoo` preserves all of it; `is_unauthorized`/`is_not_found` match on status and name. |

## Retry policy

Reads only (`search_read`, `read`, `search_count`), one retry, 500 ms apart, on 502/503/504
and connection errors. This deployment runs one Odoo process on a 512 MB box where the
OOM killer restarts Odoo and users see 502s, so a single retry on a read is worth five
lines. Writes are never retried: JSON-2 has no idempotency key, and a retried `create`
would duplicate a task.

## CLI

```
hodoo [GLOBAL] <command>

GLOBAL  --url <URL>  --api-key <KEY>  --db <NAME>  --insecure  --no-retry
        --timeout <SECS>  --pretty  --verbose
ENV     ODOO_URL, ODOO_API_KEY, ODOO_DB   (flags win)

whoami                      POST res.users/context_get; prints the resolved user
version                     GET /web/version (no key needed)
project ls|get|create|update|rm
project stages ls|create|update|rm    the stages a project moves through (project.project.stage)
project task-stages ls <id>           the task stages attached to this project (type_ids)
project task-stages create --name <N> [--project <id>] [--sequence <N>] [--fold]
project task-stages update <id> [--name <N>] [--sequence <N>] [--fold|--unfold]
project task-stages rm <id> [-f]
project attach <id> --task-stage <id>...   may repeat
project detach <id> --task-stage <id>...   may repeat
task ls|get|create|update|rm
task done <id>              state = 1_done
task cancel <id>            state = 1_canceled
task comment <id> --body <TEXT> [--internal]
milestone ls --project <id> | create --project <id> --name <N> [--deadline YYYY-MM-DD]
milestone reached <id> [--undo]
tag ls | ensure <NAME>
call <MODEL> <METHOD> [--json <BODY>] [--ids <ID,ID>]
```

- Output is JSON on stdout, always, so agents and scripts parse one shape; `--pretty`
  indents it. Errors go to stderr as `{"error": {"kind": "...", "status": ..., "message": "..."}}`.
- Exit codes: `0` success, `1` Odoo/transport error, `2` usage or configuration error
  (missing key, bad URL, unparsable body).
- `hodoo whoami` is the smoke test: it proves the URL, the certificate, and the key in
  one call before anything is created.
- `task ls` defaults to `--limit 50`; `--limit 0` means no limit. The library never
  injects a limit of its own.
- Flags mirror the typed fields but stay thin: `--assignee <id>` may repeat, `--tag <id>`
  may repeat (repeatable means `Command.set`).

## Testing

Three layers, all runnable without a live Odoo except the second.

1. **Unit + stub HTTP** (`crates/hodoo/tests/http.rs`, `wiremock`): asserts the exact
   request -- path `/json/2/project.task/search_read`, `Authorization: bearer <key>`,
   `X-Odoo-Database` only when configured, and the JSON body shape for `create`
   (including `[[6, 0, [id]]]` for `assignees` and `Some(vec![])` clearing `tags`).
   Error mapping is covered: 401 `Invalid apikey`, 404 unknown method, 422 bad kwarg,
   502 HTML body, and a malformed success body. `de::opt_id` gets table-driven tests for
   `null`, `false`, `7`, and `[7, "Acme"]`; `de::id_list` for `[1,2]` and `[[1,"a"]]`.
   Read structs are deserialized from a captured real response body, byte for byte.
2. **Live integration** (`crates/hodoo/tests/live.rs`, `#[ignore]`), run with
   `HODOO_LIVE=1 ODOO_URL=... ODOO_API_KEY=... cargo test -- --ignored --nocapture`.
   Sequence: `version`, `whoami`, create project, create stage in project, assert the
   project's `type_ids` contains it, create task, assign, set deadline and priority,
   comment (internal and not), create milestone and mark reached, add a subtask, search
   with `open_only` and a deadline filter, then delete task, project and stage. Cleans up
   what it creates even on failure, and refuses to run unless `HODOO_LIVE=1` is set.
3. **Schema drift** (`#[ignore]`, needs a Settings-level key): fetch
   `/doc-bearer/project.task.json` and `/doc-bearer/project.project.json`, and assert
   every field name the read structs declare still exists in Odoo's listing. This catches
   an Odoo rename at `cargo test -- --ignored` time instead of in production, without
   codegen.

Commands: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, plus the two ignored suites. The repo's `scripts/check.sh` is untouched:
it lints the deploy scripts, and `hodoo/` gets its own Cargo commands. A shell hook or
justfile recipe is not part of this design.

## Rust idioms held to

- No `unwrap`/`expect`/`panic!` outside tests; lints `unwrap_used` and `expect_used`
  enforced at warning level.
- `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`; every public item documented.
- Errors are one `#[non_exhaustive]` enum with `#[source]` chaining via `thiserror`,
  and a crate-level `Result<T>` alias. No `anyhow` in the library; the CLI uses it.
- No `async fn` in traits, no `dyn`, no interior mutability; `Client` is a plain
  `Clone` handle. The library does not spawn tasks and does not create a runtime.
- Borrowed parameters (`&str`, `&[T]`, `impl Into<String>`), `&self` on scope accessors,
  `#[must_use]` on constructors and builders.
- `serde` derives only; no hand-written `Serialize` except `Id<T>` (bare number) and the
  `From<*Fields> for Value` conversions, which exist because Odoo needs `[[6, 0, ids]]`.
- One HTTP call site (`http.rs`). No other module owns a `reqwest` type.

## Rejected alternatives

- **Fully typed structs per model, no generic `call`.** Would need a struct per Odoo
  field, drift every release, and the escape hatch was requested anyway.
- **Codegen the typed layer from `/doc-bearer/index.json`.** Adds a build-time network
  dependency. Its drift check is kept as an ignored test instead.
- **MCP server.** Dropped in favor of a library plus CLI; the web app consumes the
  library directly.
- **Rust workspace split per model (`hodoo-project`, `hodoo-task`, ...).** Ceremony with
  no consumer benefit; module boundaries inside one crate are enough.
- **Raw strings for Odoo datetimes.** Pushes a timezone assumption onto every caller.
- **Retrying writes.** No idempotency key in JSON-2; a retried `create` duplicates data.

## Prerequisite for implementation verification

A live API key for `https://odoo.jsmx.org`, created by a user with Settings access
(`base.group_system`) so the drift suite can read `/doc-bearer/*.json`. Keys expire in at
most three months and are displayed once. Without a key, layer 1 (`wiremock`) and the
whole build still verify; layers 2 and 3 stay `#[ignore]`d.

## As built: where the code departs from this spec

Recorded after implementation, so the spec does not quietly disagree with the crate.

- `Config::accept_invalid_certs(bool)` replaces `insecure_cert(bool)`: the latter would
  have collided with the crate-internal `insecure_cert()` getter. `retry_reads` kept its
  name, and its getter became `retries_reads()`.
- `Priority` variants are `Low`, `Medium`, `High`, `Urgent` (Odoo 19's own labels), not
  `Normal`/`VeryHigh`.
- `Visibility` was added for writes, with all four v19 values: `followers`,
  `invited_users`, `employees`, `portal`. The spec only mentioned the read field.
- `ProjectFields` gained `allow_milestones`, because a milestone cannot be created on a
  project that does not allow them.
- `Projects::attach_stages` is a read-modify-write union rather than a raw `type_ids`
  write, since Odoo has no append call that takes bare ids; `set_stages` and
  `detach_stages` are the "replace" and "remove" forms.
- Filters: `active_only` was dropped (Odoo already excludes archived records), and
  `TaskFilter` gained `state` and `parent`.
- `Tasks::messages` and `chatter::Message` were added: chatter was in scope, and reading
  the thread is what makes posting it verifiable.
- `Error::Missing` was added for "the record was not there", keeping `is_not_found` for
  the HTTP 404 case. Any non-JSON body, whatever its status, is
  `Error::UnexpectedResponse`; `Error::Decode` is raised only by `call_as`, where a
  successful JSON body fails to fit the Rust type.
- `Client::whoami` wraps `res.users/context_get`, which Odoo 19 documents as the way a
  key learns its own user id; the source confirms it appends `uid` to the context.
- CLI additions beyond the spec: `project comment`, `project attach` / `detach`,
  `milestone rm`, `--allow-milestones`. Renamed in the CLI UX rework: `stage …` became
  `project task-stages …`, `project stages <id>` became `project task-stages ls <id>`, and
  `project stages` now means `project.project.stage` (see
  `2026-09-28-hodoo-cli-ux-design.md`).
- `hodoo/README.md` was added, and `AGENTS.md` documents the workspace for agents.
- Verified against the live instance, with no records created: `hodoo version --url
  https://odoo.jsmx.org` answers `{"version":"19.0"}`; a missing key exits 2 with a usage
  error; a bogus key exits 1 with `{"kind":"odoo","status":401,"message":"Odoo error 401:
  Invalid apikey"}`, which is Odoo's real JSON-2 error object for this endpoint.

## Studied, deliberately not wrapped (2026-09-28): project templates

Odoo 19 community's project templates are reachable over JSON-2 with no change to this
crate: write `project.project.is_template`, then call
`action_create_from_template(values, role_to_users_mapping)`, which returns the new
project (so JSON-2 answers a list of ids). The skeleton of repeatable setups exists.

Verified against 19.0, and written for users in `hodoo/README.md` under "Project
templates": dates shift so the duration is preserved rather than copied; `partner_id` is
blacklisted; tasks, subtasks, stages, tags, assignees, planned hours, dependencies and
milestone links come across with the links remapped; milestone *deadlines* do not
(`deadline` is `copy=False`); task roles are cleared unless mapped; a task's
`is_template` flag is retained, which is what keeps library items out of the task list;
and passing `date_start` without `date` is a server-side 500 (`str + timedelta`).

Left unwrapped on purpose: it is one `call` today. Wrapping it means adding
`ProjectFields.is_template`, a `create_from_template` method whose `values` mirror the
wizard's whitelist, and a template project in the scenario. Worth doing when a caller
repeats a setup often enough to want it typed.

## Superseded in part: the CLI (2026-09-28)

The library in this document stands as built. The **CLI** it describes does not: the first
version printed JSON for everything and took only ids, which is a script's interface, not a
person's. It was reworked into a human-first CLI - tables, colours, human deadlines, names
instead of ids, confirmations on deletes, a dry run, shell completions - with `-o json` kept
as the machine contract. That work, and the guidelines behind each decision, is recorded in
`2026-09-28-hodoo-cli-ux-design.md`.
