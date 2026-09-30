# hodoo

A Rust client and CLI for **Odoo 19's JSON-2 API**, aimed at project
management: projects, tasks, stages, tags, milestones, subtasks, dependencies
and chatter.

`hodoo` is the library. `hodoo-cli` builds the `hodoo` binary. Both live in this
workspace so that `hodoo-web` (a Topcoat application) can join them later by
adding one member.

It talks to a *running* Odoo; the sibling `deploy.sh` in this repository is what
puts one on a 512 MB VM, and `.env*` is gitignored, so the key stays out of git.
Start with [Getting a key](#getting-a-key), then the CLI section, then
[What it encodes](#what-it-encodes-so-callers-do-not-rediscover-it) — that last
one is the list of Odoo behaviours that will otherwise cost you an afternoon.

## Why JSON-2

Odoo 19 schedules `/xmlrpc` and `/jsonrpc` for removal in Odoo 22 and offers
one replacement: `POST /json/2/<model>/<method>`, authenticated with a per-user
API key as a bearer token, arguments named at the top level of the body, and
errors answered as an HTTP status plus a JSON object. This crate speaks that and
nothing else.

## Getting a key

*Preferences > Account Security > New API Key*. A key belongs to a user and
inherits that user's access rights and record rules; it lasts at most three
months and is shown once. For the `/doc-bearer` endpoints (used by the drift
test) the user needs the Settings group.

## CLI

The CLI is for people first: tables, colour, human dates, names instead of ids. JSON
is one flag away for scripts. Everything below works with a name wherever it says a
project, task, stage, milestone, user or tag.

```sh
# From the repository root, `just` builds it first and passes arguments through:
just hodoo -- task ls --project acme
# The recipes: hodoo-check (fmt + clippy + tests), hodoo-test, hodoo-live-test,
# hodoo-prod-build, hodoo-install, hodoo-doctor, hodoo-clean.

# Either export them, or put them in a .env at (or above) the working directory --
# a .env at the repository root covers every command below.
export ODOO_URL=https://odoo.example.com
export ODOO_API_KEY=...

hodoo version                       # needs no key at all
hodoo whoami                        # proves url, certificate and key in one call
hodoo project ls                    # what is there, as a table
hodoo project stages ls             # the project stages, and how many projects sit in each
hodoo project ls --mine
hodoo project create --name "Acme website" --customer acme --milestones
hodoo project task-stages create --name "Review" --project acme --sequence 40
hodoo task create --name "Write the copy" --project acme --stage Backlog \
    --assignee me --due +7d --priority high --tag client
hodoo task ls --project acme --open
hodoo board acme                    # the kanban, grouped by stage
hodoo task show 31                  # state, who, when, what blocks it, chatter
hodoo task move 31 --stage Review
hodoo task done 31
hodoo task comment 31 --body "blocked on the certificate" --internal
hodoo task deps 31                  # what it waits on, and what waits on it
hodoo call res.partner search_read --body '{"domain":[],"fields":["name"]}'
hodoo completions bash > ~/.local/share/bash-completion/completions/hodoo
```

What it looks like:

```text
$ hodoo task ls --project acme --open
 ID  PRI     STAGE   NAME                        WHO              DUE
 22  high    Design  Design the homepage         Hanz             in 5d
 23  medium  Build   Build the CMS integration   Hanz             2d ago
 24  low     Build   Write the launch email      -                -

$ hodoo board acme
Acme Manufacturing - Website (scenario) (#83)  6 open of 8
  Backlog (scenario)          0
  Design (scenario)           2  #350 Wireframes for all 8 pages (high, in 5d) · #354 Mobile breakpoints (medium, no date)
  Review (scenario)           3  #355 Client review call (high, in 3d) · #352 Product page build (high, in 18d)
```

### Output modes

| | |
|---|---|
| default | a table for people; colour only when stdout is a terminal |
| `-o json`, `--json` | the contract for scripts: Odoo's field names, every field read, no colour, errors as JSON on stderr |
| `HODOO_OUTPUT=json` | the same, set once for a session or a whole script |
| `--no-headers` | table without headers, so `awk '{print $1}'` and `grep` work |
| `HODOO_COLOR=never`, `NO_COLOR`, `--color never` | no colour (also off when the output is piped, or `TERM=dumb`) |
| `--pretty` | indent the JSON |

stdout carries the result; notes, hints and errors go to stderr, so a pipe sees only
what you asked for. In JSON mode a change reports itself there too: `task create`
answers `{"id":31}`, other changes `{"id":31,"ok":true}`, a delete
`{"deleted":31,"ok":true}` — `id=$(hodoo task create … -o json | jq .id)` works.

Exit codes: `0` success, `1` the operation failed (Odoo or transport), `2` the
invocation was wrong (bad flag, unknown reference, a delete without confirmation), and
`141` for a closed pipe (`hodoo task ls | head`).

### Habits worth knowing

- **Anything asked for by name is resolved, not guessed.** A name that matches nothing
  is an error with a suggestion; one that matches several is an error that lists them,
  because a guess here writes to the wrong record.
- **Deletes ask first.** `project rm`, `task rm` and `milestone rm` prompt when stdin is
  a terminal; a script has to say `-f`/`--force` (or use `--no-input` to be told so). The
  prompt names what goes with the record - a project takes its tasks *and* its
  milestones, a task takes its subtasks - so nobody learns the cascade by typing `-f`.
- **A state write reports when Odoo overrides it.** `task update --state …`, `task done`,
  `task cancel` and `task reopen` read the task back, and when the state Odoo computes
  differs from the one asked for they say so on stderr with the reason. The JSON contract
  is untouched: the hint never reaches `-o json`.
- **`-n`/`--dry-run` shows what would be sent and sends nothing**, including for
  `project create --template`.
- **`-q`/`--quiet`** drops the confirmations and hints, keeping the data.
- **`-v`/`--verbose`** adds Odoo's Python traceback on failure.
- **`hodoo help <command>`** and `--help` on anything explain that command, with
  examples; `--help` also lists the credential order and the exit codes.
- **"Stage" means two different things, so the commands say which.** A task's stage
  (`project task-stages ls|create`, `task create --stage`) is one of a project's kanban
  columns. A *project's* stage (`project stages ls|create|update|rm`, `project update
  --stage`) is one of a handful the whole server shares. `project attach` and `project
  detach` take `--task-stage`, because that is the kind they attach. Everything about a
  stage lives under `hodoo project`, since a stage is a project's business either way.

Configuration resolves in this order, first hit wins:

1. the flag (`--url`, `--api-key`, `--db`, `-o`)
2. the process environment (`ODOO_URL`, `ODOO_API_KEY`, `ODOO_DB`, `HODOO_OUTPUT`)
3. a `.env` file at or above the working directory (up to four levels), read for the
   same names

A `.env` is read into a map and consulted explicitly; the process environment is never
mutated (`std::env::set_var` needs `unsafe` in edition 2024, and this crate forbids
unsafe code). `Config::from_env()` covers steps 1-2 for library callers; `hodoo::dotenv`
covers step 3.

In `-o json`, keys are **Odoo's own field names** (`date_deadline`, `user_ids`,
`privacy_visibility`, `type_ids`), so the output lines up with the model documentation
and with `/doc`. The Rust field names are friendlier; the rename lives on the struct
with `#[serde(rename = ...)]`.

### Dates a person writes

`--due`, `--due-before`, `--start` and `--end` accept `today`, `tomorrow`, `yesterday`,
`+3d`, `+2w`, `2026-12-01`, or `"2026-12-01 09:00"`. Deadlines come back as `in 3d`,
`2d ago`, `today`, or a date once they are more than a month away; overdue is red.

## Library

```rust
use hodoo::{Client, Config, ProjectFields, TaskFields, TaskFilter};

let client = Client::new(Config::new("https://odoo.example.com", key)?)?;

let project = client.projects().create(ProjectFields::new("Website")).await?;
let backlog = client
    .stages()
    .create_in(project, hodoo::StageFields::new("Backlog"))
    .await?;
let task = client
    .tasks()
    .create(TaskFields {
        project: Some(project),
        stage: Some(backlog),
        ..TaskFields::new("Write the copy")
    })
    .await?;

let open = client
    .tasks()
    .search(TaskFilter { project: Some(project), open_only: true, ..Default::default() })
    .await?;
```

`Client` is `Clone + Send + Sync` and holds no mutable state, so an application
shares one; a multi-user deployment builds one per request from that user's own
key. Ids are tagged (`ProjectId` cannot be passed where a `TaskId` is wanted),
Odoo's dates are `DateTime<Utc>`/`NaiveDate` rather than strings, and
[`Client::call`] reaches any model or method the typed helpers do not cover.

## What it encodes, so callers do not rediscover it

- A task's stage is a `project.task.type`; a *project's* stage is a
  `project.project.stage`. Two different things, two different id types, and both live
  under `hodoo project`: `project task-stages` and `project stages`.
- A task stage belongs to a project (`project.task.type.project_ids`) before a
  task in that project can sit in it. Plain `create` on a project does not create
  any stage; `stages().create_in(project, ..)` does. The rule is a view domain,
  not a server one: Odoo accepts a foreign stage over the API and the task then
  sits where the kanban will not show it.
- Odoo assigns the *calling* user to every task it creates over the API, with or
  without `user_ids` in the payload (a subtask is the exception). Clearing the
  assignees takes an explicit empty list: `user_ids: [[6, 0, []]]`, which is
  `TaskFields { assignees: Some(vec![]), .. }` or `hodoo task update --unassign`.
- Task states are `01_in_progress`, `02_changes_requested`, `03_approved`,
  `1_done`, `1_canceled`, `04_waiting_normal`; priorities are the strings `0`-`3`.
- `state` is *computed from dependencies*: a task whose `depend_on_ids` are not all
  closed reads `04_waiting_normal` ("Waiting"), so in Odoo 19 "waiting" means
  "blocked by", not "on hold". The compute does not fire during creation, so a task
  created with dependencies still reads `01_in_progress` until `depend_on_ids` is
  written again (or `state` is set explicitly) - verified against 19.0, and the
  reason `hodoo task create --depends-on` alone does not produce a waiting task.
  The same compute wins over a later write: `task update --state changes-requested`
  on a task with open blockers reads `waiting` again on the next read, because the
  state is recomputed from the blockers. Set a state only where they are closed.
- Deleting a project **cascades**: its tasks, its milestones and its chatter go with
  it. A child id list gathered before the delete is stale afterwards, and unlinking
  from it answers 404. Milestones are the one that catches people out.
- Projects and tasks share **one tag model** (`project.tags`), so `tag ls` and a tag
  count include both; a tag is created on demand by `--tag` on any record.
- `--limit 0` means *unlimited* on the `ls` commands, and the default is a page.
- `board` answers differently per mode: the table groups open tasks by column (one
  line each, plus a header), while `-o json` returns a single object holding every
  open task. Counting lines of a JSON board counts one.
- Assigning `user_ids` also adds the calling user and stamps `date_assign`.
- A task without a project is private and loses its stage.
- Milestones need `allow_milestones` on the project, and task dependencies need
  `allow_task_dependencies`.
- `project.project.stage_id` and `project.task.milestone_id` are group-restricted in
  Odoo: reading them as a user without the project's Stages/Milestones settings gets an
  `AccessError` naming the field, whose message the CLI prints as-is.
- Reads always name their fields: Odoo returns every field, computed columns
  included, when `fields` is omitted.
- Reads are retried once after a connection failure or a 502 (this deployment
  runs on 512 MB, where Odoo does get OOM-killed); writes never are, because
  JSON-2 has no idempotency key and a retried `create` duplicates a record.

## What it does not wrap, and the call that does

The typed surface covers projects, tasks, task stages, project stages, tags, milestones
and chatter. Everything else is one `call` — these are the ones that came up in practice,
with what was verified against Odoo 19.0 rather than assumed.

| Want | Call |
|---|---|
| Project from a template (repeatable setups) | `call project.project action_create_from_template --ids <tpl> --body '{"values":{...}}'` — see below |
| Create a client contact | `call res.partner create --body '{"vals_list":[{"name":"Acme","is_company":true}]}'` — needs Contacts > Creation (`base.group_partner_manager`), which a project administrator does not have |
| Archive a project stage | `call project.project.stage write --ids <id> --body '{"vals":{"active":false}}'` — what Odoo suggests when a project is still in the stage |
| Collaborators, project roles | `call project.collaborator ...` / `call project.role ...` |
| Rollups and group-bys | `call project.task search_count --body '{"domain":[...]}'` |
| Find your own user id | `hodoo whoami` (wraps `res.users/context_get`) |

### Project templates

Make a project a template with `is_template: true` (that flag *is* the whole server-side
mechanism; the `action_toggle_*` methods only return UI dialogs), then instantiate it. The
result is the new project, so JSON-2 answers with a list of ids.

```sh
hodoo call project.project write --body '{"ids":[<template>],"vals":{"is_template":true}}'
hodoo call project.project action_create_from_template --ids <template> \
  --body '{"values":{"name":"Acme site","partner_id":12}}'
```

Verified against 19.0, because none of this is obvious:

- **Dates shift instead of being copied.** The copy runs `today -> today + (template.date
  - template.date_start)`, so a 30-day template always spans 30 days from the start date.
- **Pass `date_start` *and* `date`, or neither.** `date_start` alone is a server-side 500:
  Odoo does `values["date"] = values["date_start"] + (self.date - self.date_start)` and
  never parses the string, so it adds a `timedelta` to a `str`. Both, or neither, skips
  that branch. Odoo's own test passes both.
- **`partner_id` is blacklisted**, so the new project has no customer unless you pass one
  in `values`.
- **Tasks, subtasks, stages, tags, assignees, planned hours, dependencies and milestone
  links all come across**, with dependencies and milestone links remapped to the new ids.
- **Milestone deadlines do not**: `deadline` is `copy=False`. Link survives, date does not.
- **Task roles are cleared** on the copies unless you pass a role-to-user mapping.
- **`is_template` is preserved on tasks.** Ordinary tasks in a template come back as
  ordinary work; tasks you flagged stay flagged and only show under the "Templates" filter,
  because a project's task list excludes `has_template_ancestor = True`. Put the real
  breakdown in the template as normal tasks and use flagged ones as an optional library.
- Odoo posts "Project created from template X." on the new project's chatter.

The UI path is `project.template.create.wizard` (name, dates, alias, role mapping), which
calls the same method — so there is no second mechanism to learn.

## Tests

```sh
cargo test                              # unit tests, stub-HTTP tests, doctests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The CLI has its own tests: `crates/hodoo-cli/tests/ux.rs` runs the real binary against a
stub Odoo and asserts what a user sees — the table default, `-o json`, `--no-headers`,
human deadlines, a dry run sending nothing, a delete refusing without `-f`, the
stdout/stderr split, exit codes, and clap's suggestions for a typo. Unit tests inside the
binary cover the renderer and the argument surface.

Two suites are `#[ignore]`d because they need a real server. They take their
credentials from the same place as the CLI, so a `.env` is enough:

```sh
HODOO_LIVE=1 cargo test -- --ignored --nocapture
# or, overriding anything the .env carries:
HODOO_LIVE=1 ODOO_URL=... ODOO_API_KEY=... cargo test -- --ignored --nocapture
```

- `tests/live.rs` runs a project lifecycle (create project, stage, task,
  assignee, deadline, dependencies, comments, milestone, subtask, searches) and
  deletes everything it created at the end, even when an assertion fails.
- `tests/drift.rs` compares every field this crate reads against Odoo's own
  `/doc-bearer/<model>.json` listing, so an Odoo rename fails a test instead of
  a production read. It needs a key whose user has the Settings group (the
  backend error says "Technical Documentation users"); otherwise it prints a
  skip instead of failing.

`HODOO_INSECURE=1` accepts a self-signed certificate, which is what
`configs/nginx-odoo.conf` generates by default in this repository.

## A worked example

`scenarios/startup-founder.sh` builds a whole founder's Odoo through the CLI: two client
websites, the product itself and personal life, with 18 task stages, tags, milestones,
subtasks, a dependency chain per project, chatter, and a task in every state. `up` asserts
46 properties while it works (so it doubles as an end-to-end test) and `down` removes
exactly what it created, found by its `(scenario)` name marker. `show` is the dashboard: it
unsets the script's JSON mode and simply runs `hodoo whoami`, `project ls`, `board`,
`task ls --overdue`, `milestone ls` and `task show`, so it is also a tour of the human
output.

```sh
cargo build                       # the scenario drives the built binary
./scenarios/startup-founder.sh up
./scenarios/startup-founder.sh show
./scenarios/startup-founder.sh down
```

`scenarios/icare-dd.sh` is the second one, and a harder test of the CLI: a manager-level
due diligence of a cross-border fund manager as one project with 15 workstreams, five
phases, four phase gates, 7 milestones and 79 tasks, each of which carries its own
standard, evidence and acceptance test in its description. It builds the same way
(`up` / `down` / `show`, marker `(icare-dd)`, assertions as it goes) and it asserts the
rules that make the dataset worth having: no task without an owner, a standard, the
evidence and the acceptance test, and no red finding without a remediation.

```sh
just icare-dd up                  # or ./scenarios/icare-dd.sh up
./scenarios/icare-dd.sh show
just icare-dd down
```

**Before writing a third one, read [`scenarios/SOP.md`](scenarios/SOP.md).** It records the
rules the first two builds proved the hard way: derive every count from the data table
rather than typing a total, guard a rebuild on a residue sum of *everything* the script
creates, re-query child ids after a cascade, and never parse a table with `awk`.

The toolchain lives in `~/.cargo/bin`, which is not always on `PATH`; use
`~/.cargo/bin/cargo` or export the path first.
