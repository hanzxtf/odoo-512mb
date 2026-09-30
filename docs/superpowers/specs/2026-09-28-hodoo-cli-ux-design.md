# hodoo CLI: human-first rework (design)

Date: 2026-09-28
Status: implemented

## Why

The first CLI was designed as a transport for scripts: JSON on stdout for every command,
ids on every flag, help text that only listed flags. That is the wrong default. A person
who types `hodoo task ls` wants a table they can read, not a JSON blob; a person who types
`hodoo task create --name "x" --project 7` wants to write `--project acme`, not look up an
id first. Guidelines followed (clig.dev, and the shapes `gh`, `kubectl`, `git` and `cargo`
settled on):

- "Human-readable output is paramount. Humans come first, machines second."
- "a user should be able to pipe output to `grep` and it should do what they expect."
- "Disable color if your program is not in a terminal or the user requested it."
- "Prefer flags to args … Use standard names for flags, if there is a standard."
- "Never *require* a prompt. Always provide a way of passing input with flags."
- "If a user hits Ctrl-C … exit as soon as possible."
- "Suggest commands the user should run", "If the user did something wrong and you can
  guess what they meant, suggest it."
- Config precedence: flags → environment → project config (`.env`).

## What changes

| | before | after |
|---|---|---|
| Default output | JSON for everything | aligned table, colour only on a TTY |
| Machine output | always | `-o json` (or `--json`), stable and complete |
| Identifiers | ids only | **ids or names** everywhere a record is referenced |
| Errors | one-line JSON object | human sentence plus a suggested fix, on stderr |
| Destructive `rm` | immediate | confirmation, or `-f`/`--force`; `--no-input` never prompts |
| Preview | none | `-n`/`--dry-run` prints what would be sent, sends nothing |
| Help | flag lists | what it does, **examples first**, env vars, exit codes, next steps |
| Completions | none | `hodoo completions bash|zsh|fish|elvish|powershell` |
| Success | silent `{"ok":true}` | one short line naming what changed, plus a next step |
| Missing values | lookup error | fuzzy suggestion ("did you mean …") |

## Output contract

- **stdout** carries the result and nothing else: a table by default, JSON with
  `-o json`. Errors and hints go to **stderr**.
- `-o json` is a contract: Odoo's own field names, complete records, errors as
  `{"error":{kind,status,message,...}}` on stderr, no colours, no hints.
- Tables: headers in bold, `--no-headers` for `awk`. Long values are truncated only when
  writing to a terminal (with `…`), so a pipe always sees full values.
- Colours: red for overdue/error, yellow for due-soon/urgent, dim for finished work and
  hints, bold for headers. Disabled by a non-TTY stdout, `NO_COLOR`, `TERM=dumb`, or
  `--color never`; forced by `--color always` (which implies colour even when piped).
- Modes and colour can be set once for a whole session: `HODOO_OUTPUT=json`,
  `HODOO_COLOR=never`. A flag always wins over the environment (clig.dev's precedence).

## Exit codes

`0` success · `1` the operation failed (Odoo or transport) · `2` the invocation was wrong
(bad flag, unknown reference, refused confirmation without a TTY) · `141` a closed pipe
(`| head`), reported the way a SIGPIPE'd process would be.

## Commands

Verbs are consistent across nouns: `ls`, `show`, `create`, `update`, `rm`, plus the
domain-specific ones. Every reference flag and argument accepts an **id or a name**;
names are matched exactly first, then case-insensitively, then fuzzily, and an ambiguous
or missing name is an error listing the candidates.

```
hodoo whoami                        # who the key is, on which server
hodoo version                       # the server's Odoo version; needs no key

hodoo project ls [--customer acme] [--mine] [--tag website]
hodoo project show <project>        # detail block: dates, stages, counts, chatter
hodoo project create --name "Acme website" [--customer acme] [--template <tpl>]
hodoo project update <project> [fields]
hodoo project rm <project> [-f]
hodoo project stages ls             # the project stages, with what sits in each
hodoo project stages create --name "On Hold" [--sequence 17] [--fold]
hodoo project stages update <stage> [--name <N>] [--sequence <N>] [--fold|--unfold]
hodoo project stages rm <stage> [-f]
hodoo project task-stages ls <project>   # the project's task stages, with open counts
hodoo project task-stages create --name "Review" [--project <ref>] [--sequence 40] [--fold]
hodoo project task-stages update <stage> [--name <N>] [--sequence <N>] [--fold|--unfold]
hodoo project task-stages rm <stage> [-f]
hodoo project attach <project> --task-stage <stage>…   # borrow a shared task stage
hodoo project detach <project> --task-stage <stage>…
hodoo project comment <project> --body "…" [--internal]

hodoo task ls [--project <ref>] [--mine] [--open] [--overdue] [--due-before <when>]
              [--stage <ref>] [--assignee <user>] [--tag <name>] [--parent <ref>]
hodoo task show <task>              # detail block: state, who, when, blocked by, chatter
hodoo task create --name "…" --project <ref> [--stage <ref>] [--assignee <user>]…
              [--tag <name>]… [--due <when>] [--priority high] [--depends-on <task>]…
              [--parent <task>] [--milestone <ref>] [--hours 8]
hodoo task update <task> [fields]
hodoo task done|cancel|reopen <task>
hodoo task move <task> --stage <ref>
hodoo task comment <task> --body "…" [--internal]
hodoo task messages <task> [--limit 20]
hodoo task deps <task>              # what it waits on, and what waits on it
hodoo task rm <task> [-f]
hodoo board [<project>]             # tasks grouped by stage: the kanban a founder wants

hodoo milestone ls --project <ref>
hodoo milestone create --project <ref> --name "Beta" [--due <when>]
hodoo milestone reached <id> [--undo]
hodoo tag ls

hodoo call <model> <method> [--body '<json>'] [--ids 1,2]   # the escape hatch
hodoo completions <shell>
```

`--when` values accept `today`, `tomorrow`, `yesterday`, `+3d`, `+2w`, `2026-12-01`, and
`2026-12-01 09:00`.

## Changes to existing behaviour (breaking, deliberately)

- `call --json <body>` becomes `call --body <body>`, because `--json` is now the standard
  "print JSON" flag and overloading it would be ambiguous.
- Scripts that read stdout as JSON must pass `-o json` (or set `HODOO_OUTPUT=json`). The
  scenario script does so with one exported variable.
- `project rm` refuses without `-f` when stdin is not a terminal; `task rm` and
  `milestone rm` behave the same.
- `hodoo stage …` became `hodoo project task-stages …`, and `hodoo project stages <project>`
  became `hodoo project task-stages ls <project>`. "Stage" alone now means a *project's*
  stage: `hodoo project stages` lists `project.project.stage`, and `project attach`/`detach`
  take `--task-stage`. Odoo keeps the two ideas in two models, so a CLI that calls both of
  them "stage" makes the reader guess which one a command touches.
- **A stage is a project's business, so every stage command lives under `hodoo project`.** A
  top-level `task-stages ls` listing every stage on the server was dropped: it answered a
  question nobody asks, and it made "stage" mean a task's stage at the top level and a
  project's stage one level down. `project task-stages` is scoped to a project like every
  other `project` subcommand, and both stage nouns now carry the same verbs (`ls`, `create`,
  `update`, `rm`), which is also what makes `--help` show the CRUD. Both `rm`s refuse while a
  record is still in the stage and name it, because Odoo's own answer is a `ValidationError`
  about the model.

## Structure

`main.rs` parses and dispatches; everything else has one job:

```
main.rs        clap parse, context, error → exit code
cli.rs         the argument surface only (help text, examples, aliases)
output.rs      output modes, table, colours, human dates, blocks, hints
refs.rs        id-or-name resolution, date/duration parsing, ambiguity errors
prompt.rs      confirmation, --force, --no-input, --dry-run rendering
cmd/           one module per noun: project, task, stage, milestone, tag, call,
               completions, account (whoami, version)
```

The library (`crates/hodoo`) is **not** touched: resolution, rendering and prompts are
presentation concerns, so the client keeps its single responsibility and its API does not
churn because the CLI grew a personality.

## Tests

- Parser: `Cli::command().debug_assert()` (catches flag collisions), and that the rendered
  help contains examples, the env vars and the exit codes.
- Output units: table padding/alignment/truncation, `--no-headers`, human dates, colour
  on/off, JSON shape.
- End-to-end with a stub Odoo (wiremock) and `assert_cmd`: table vs `-o json`, `--dry-run`
  sends **no** request, `rm` without `-f` on a non-TTY exits 2 without deleting, `-f`
  deletes, an Odoo error becomes one human stderr line and exit 1, `--no-input` never
  prompts, `completions` emits a script.

## As built

- `-o table|json` and `--json` both exist, and `HODOO_OUTPUT` is handled by clap's `env`
  (so it shows up in `--help` as a documented fallback rather than a hidden behaviour).
  `--color`/`HODOO_COLOR` joined it; `NO_COLOR` and `TERM=dumb` are still honoured.
- The `PROJECT` column appears in `task ls` only when the listing spans projects; inside
  one project it would repeat the same value on every row.
- Board stage names are clipped at 24 columns, not 16: real names ("In Progress
  (scenario)") were being cut into nonsense.
- Name lookups are best effort. A key that may read projects but not users gets dashes in
  the name columns instead of a failed command, because a name is decoration and the id is
  the data.
- `deps -o json` answers `{"task":…,"blocked_by":[…],"blocks":[…]}`: both directions of the
  relation, matching what the human view shows. `blocks` needs `dependent_ids`, which the
  client reads on its own because it is not part of a task's usual field list.
- `Error::UnexpectedResponse` now says "(an empty body: does this server serve /json/2, and
  does the model exist?)" when the body is empty, instead of trailing a colon into nothing.
- `--when` in the command table above is `--due`/`--due-before`/`--start`/`--end` in the
  built CLI, each named after the field it sets.
- `milestone rm` gained an example when the test that requires examples in every leaf
  command caught it missing.
- `project stages` was reachable only through `hodoo call` until it grew `ls`, `create`,
  `update` and `rm` (the last refuses while a project is in the stage, naming it, because
  Odoo's own answer is a `ValidationError` about the project model). `project ls` and
  `project show` now name the stage a project is in; both read `project.project.stage`
  through the typed client, which grew `ProjectStages` (`list`, `create`, `update`,
  `delete`) alongside `Stages`.
- `StageFields` gained `active`, so a caller can archive a stage, the alternative Odoo
  itself suggests to a blocked delete.
- A name that matches nothing now suggests the command that would list it, per noun: "no
  project stage matches … List what exists with `hodoo project stages ls`". The old wording
  printed `hodoo stage ls`, which the rename above retired.
