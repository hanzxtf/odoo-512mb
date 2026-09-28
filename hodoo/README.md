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

```sh
# Either export them, or put them in a .env at (or above) the working
# directory -- a .env at the repository root covers every command below.
export ODOO_URL=https://odoo.example.com
export ODOO_API_KEY=...

hodoo version                 # needs no key at all
hodoo whoami                  # proves url, certificate and key in one call
hodoo project ls --name website --limit 10
hodoo project create --name "Website" --allow-milestones --allow-task-dependencies
hodoo stage create --name "Backlog" --project 7      # attached to the project
hodoo task create --name "Write the copy" --project 7 --stage 12 \
    --assignee 2 --deadline 2026-12-01 --priority high --tag 4
hodoo task ls --project 7 --open --order "priority desc"
hodoo task comment 31 --body "blocked on the certificate" --internal
hodoo task deps 31
hodoo task done 31
hodoo call res.partner search_read --json '{"domain":[],"fields":["name"]}'
```

Configuration resolves in this order, first hit wins:

1. the flag (`--url`, `--api-key`, `--db`)
2. the process environment (`ODOO_URL`, `ODOO_API_KEY`, `ODOO_DB`)
3. a `.env` file at or above the working directory (up to four levels), read for
   the same names

A `.env` is read into a map and consulted explicitly; the process environment is
never mutated (`std::env::set_var` needs `unsafe` in edition 2024, and this crate
forbids unsafe code). `Config::from_env()` covers steps 1-2 for library callers;
`hodoo::dotenv` covers step 3.

Every command prints JSON on stdout; failures print a JSON object on stderr and
exit `1` (Odoo or transport) or `2` (usage or configuration). `--pretty` indents,
`--verbose` adds Odoo's Python traceback, `--limit 0` means no limit.

The JSON keys are **Odoo's own field names** (`date_deadline`, `user_ids`,
`privacy_visibility`, `type_ids`) both ways, so output lines up with the model
documentation and with `/doc`. The Rust field names are friendlier; the rename
lives on the struct with `#[serde(rename = ...)]`.

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
  `project.project.stage`. Two different things, two different id types.
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

The typed surface covers projects, tasks, stages, tags, milestones and chatter. Everything
else is one `call` — these are the ones that came up in practice, with what was verified
against Odoo 19.0 rather than assumed.

| Want | Call |
|---|---|
| Project from a template (repeatable setups) | `call project.project action_create_from_template --ids <tpl> --json '{"values":{...}}'` — see below |
| Create a client contact | `call res.partner create --json '{"vals_list":[{"name":"Acme","is_company":true}]}'` — needs Contacts > Creation (`base.group_partner_manager`), which a project administrator does not have |
| Project stages (`To Do`, `In Progress`, ...) | `call project.project.stage search_read` then `project update --stage <id>` |
| Collaborators, project roles | `call project.collaborator ...` / `call project.role ...` |
| Rollups and group-bys | `call project.task search_count --json '{"domain":[...]}'` |
| Find your own user id | `hodoo whoami` (wraps `res.users/context_get`) |

### Project templates

Make a project a template with `is_template: true` (that flag *is* the whole server-side
mechanism; the `action_toggle_*` methods only return UI dialogs), then instantiate it. The
result is the new project, so JSON-2 answers with a list of ids.

```sh
hodoo call project.project write --json '{"ids":[<template>],"vals":{"is_template":true}}'
hodoo call project.project action_create_from_template --ids <template> \
  --json '{"values":{"name":"Acme site","partner_id":12}}'
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
46 properties while it works (so it doubles as an end-to-end test), `down` removes exactly
what it created by its `(scenario)` name marker, and `show` prints a read-only dashboard:
projects with overdue counts, the blocked-by graph, milestones and the personal list.

```sh
cargo build                       # the scenario drives the built binary
./scenarios/startup-founder.sh up
./scenarios/startup-founder.sh show
./scenarios/startup-founder.sh down
```

The toolchain lives in `~/.cargo/bin`, which is not always on `PATH`; use
`~/.cargo/bin/cargo` or export the path first.
