# SOP: building a project dataset over the hodoo CLI

For anything that creates a project, its columns, its milestones and its tasks in bulk -
`startup-founder.sh`, `icare-dd.sh`, the next one. Written after the iCare build, where
seven failures came from the same root: **an assumption about Odoo, or about my own
data, typed as a constant.** Every rule below exists because one of those failed.

Read this before writing the script, not after it fails.

## 1. Data first, code second

1. Write the whole dataset as a table before writing a line of bash: one row per task,
   `|`-separated, in a `read -r -d '' NAME <<'EOF'` block. A row that cannot be expressed
   as table columns is usually two tasks or a missing column.
2. Put the rows in a stable order (workstream, then phase) so parents come before
   children. Creation is one pass; do not re-sort later.
3. **Derive every count from the table.** Never type a total into an assertion:
   ```bash
   ITEM_COUNT="$(printf '%s\n' "$ITEMS" | grep -c '|')"
   WS_COUNT="$(printf '%s\n' "$ITEMS" | cut -d'|' -f1 | sort -u | wc -l | tr -d ' ')"
   TASKS_TOTAL=$((ITEM_COUNT + GATES))
   ```
   A hand-typed total is an assertion that stops testing anything the moment the data
   changes, and it fails *after* a long build. This rule alone would have caught two of
   the seven failures.
4. Count tags and other shared records from the model's real rules, not from your
   intention: Odoo 19 keeps **one** tag model for projects and tasks, so project tags
   count too.

## 2. Build order (a project cannot be built out of order)

```
whoami → cheap version check
residue → down (if anything is left)
project (visibility, milestones, dependencies, tags)
stages  (attached to the project, numbered by sequence)
milestones (one per gate, with a date)
parents (the first phase of each workstream)
children (--parent for structure, --depends-on for the chain)
movement (a few done / changes-requested / comments)
assertions
```

- A task cannot be staged until its stages exist: **a fresh project has none.**
- A stage belongs to a project; `stage create --project` attaches it.
- Milestones need `allow_milestones` and dependencies need `allow_task_dependencies` on
  the project, which `project create --milestones --dependencies` sets.
- Reference ids by variable from the moment they are created. Never re-find a record you
  already have the id of, and never rely on a name search to be unique.

## 3. Odoo behaviour that bites (all verified against 19.0)

| Fact | Consequence for scripts |
|---|---|
| Deleting a project cascades to its tasks, milestones and chatter | Never collect a child id list before deleting its parent: **re-query after every cascade**, or the unlink 404s |
| `state` is computed from open blockers | `task update --state X` on a blocked task reads `waiting` again. Set a state only where the blockers are closed, or pin the behaviour as an assertion |
| The compute does not fire during creation | A newly created task with open blockers reads `01_in_progress`; writing the dependencies again is what makes it `04_waiting_normal` |
| Creating a task assigns the calling user | Clear with `--unassign`, or assert that only the acting user appears |
| Odoo accepts a stage from another project | That rule is a view domain, not a server one: assert it rather than assume it is refused |
| One tag model for projects and tasks | `project.tags` counts both; a tag is created by `--tag` on any record |
| `-o json` changes `board`'s shape | The table groups by column; the JSON is one object holding every open task |
| Contacts need `base.group_partner_manager` | A project admin may not create `res.partner`: degrade with a note and keep building |

## 4. Idempotency and teardown

- **A dataset is rebuilt, not patched**, so `up` must survive being run twice.
- Guard on a **residue sum of everything you create** - projects, task stages, tags,
  milestones - not just projects. An interrupted teardown leaves orphan stages that
  belong to no project, and a projects-only guard walks straight past them into
  duplicates.
- Tear down children before parents, re-querying after each cascade.
- `rm` refuses without `-f` when stdin is not a terminal; pass `-f` or `--no-input` so a
  script can never hang.
- Teardown ends by asserting zero of each kind it created, so a partial run is visible
  rather than silent.

## 5. Mode and parsing discipline

- The script sets `HODOO_OUTPUT=json` once; a `show`-style dashboard unsets it so the
  CLI's tables do for a person what python would otherwise do badly.
- **Never parse a table with `awk`.** In JSON mode there is no table, and `--limit 0`
  (unlimited) plus a fixed column count is a parse you will not notice breaking. Read
  JSON with python one-liners.
- `--limit 0` means unlimited on `ls` commands; the default is a page.
- stdout carries the result, stderr carries notes and errors: a pipe sees only the data.
- Capture a created id with `| field .id` from `-o json`, never by scraping a table.

## 6. Assertions are the deliverable

A scenario is the repo's test (there is no test framework). So:

1. Assert the **invariants that matter**, not just the counts: e.g. "no task is missing
   its Owner/Standard/Evidence/Acceptance lines", "no red item lacks a remediation".
   That is what makes the dataset trustworthy rather than merely present.
2. Assert counts per column and per tag, derived from the table.
3. **Pin surprising behaviour as an assertion** with a comment saying which way Odoo
   actually behaves. A scenario that only asserts what you expect is a scenario that
   lies when Odoo changes.
4. Exercise the failure paths (a missing id, a bad field name) and assert the exit code.
5. Print what you assert: `ok <what> = <value>` - the number is evidence, not decoration.

## 7. Before saying it works

**`just check` does not lint this directory** - it sweeps `deploy.sh` and `scripts/*.sh`
only. So the scenarios get no static gate unless you run one yourself:

```sh
bash -n hodoo/scenarios/<script>.sh
shellcheck -S warning -e SC1010 hodoo/scenarios/<script>.sh
```

`SC1010` is excluded because `hodoo task done <id>` reads as a stray shell keyword to
shellcheck; both existing scripts trip it. Everything else should be clean.

Then:

- [ ] `bash -n` clean, then `up` end to end with every assertion passing
- [ ] `up` a **second time** with no residue left from the first
- [ ] `down` then assert nothing of yours remains
- [ ] `show` renders as a person would read it
- [ ] `up` run from an **interrupted** state (kill it mid-build, run again)
- [ ] every count in the script derived from the data, no magic totals
- [ ] the script never edits or deletes another dataset's records (marker discipline)

## 8. Changes that would stop this class of failure at the source

Recommended, in order of value:

1. **`task update --state` should warn when the write is overridden.** `crates/hodoo-cli/src/cmd/task.rs:528` writes `fields.state`; a follow-up read could compare and print a
   stderr hint ("Odoo recomputed this to waiting: the task has open blockers"). Same
   shape as the existing hints for actor assignment. This turns a silent no-op into a
   sentence at the moment it happens.
2. **`project rm` / `task rm` should report what cascades.** The confirm message should
   name the child records that will go with it (tasks, milestones, chatter), so a script
   author learns the cascade from the prompt instead of from a 404.
3. **`board` should say which shape it is in.** `--json` returning one object with a
   `tasks` array, while the table groups by column, is a trap for scripts; document it in
   `--help` (done in the README as of this SOP).
4. **A `hodoo tag create`.** Tags can only be created through `--tag` today, so
   `tag ls` is the only way to confirm a namespace; a create would make tag setup
   explicit rather than a side effect of the first task.
5. **The README's "What it encodes" is the right home for cascades and the shared tag
   model** - both are now in it, because they cost an afternoon each otherwise.
