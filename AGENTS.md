# AGENTS.md

Guidance for agents working in this repository.

## What this repo is

A deterministic, idempotent deployment of **Odoo 19 + PostgreSQL 17 + nginx** onto a
**512 MB RAM / Debian 13** Vultr VM (~4 users, mostly Projects). There is no application
source here: this repo *is* the deployment. `README.md` (377 lines) is the document of
record and mirrors `deploy.sh` step by step; it explains the reasoning behind every
limit (why only `/web/assets/` is cached, why OOM kills Odoo and not PostgreSQL, etc).
Read it before changing anything with a number in it.

**The repo, not the live machine, is the source of truth.** Configs are symlinked from
`configs/` into `/etc`; `configs/odoo.conf` is the one exception, rendered by `deploy.sh`
into `/etc/odoo/odoo.conf` so the admin master password never lands in git. Editing
`/etc` or `/opt/odoo` directly is pointless: the next `sudo ./deploy.sh` reverts it.

No test framework exists and none is wanted. Verification is three-layered:
`scripts/check.sh` (static, offline) → assertions at the end of `deploy.sh` (live) →
`scripts/doctor.sh` (read-only health check of a deployed box).

## Two projects in one repo

1. **The deployment** (`deploy.sh`, `scripts/`, `configs/`, `README.md`) described in this
   file's earlier sections. Shell and systemd; verified by `just check` / `just deploy` /
   `just doctor`.
2. **`hodoo/`**, a Rust workspace that talks to Odoo 19 over its JSON-2 API: a library
   (`crates/hodoo`) plus a CLI (`crates/hodoo-cli`, binary `hodoo`). It is unrelated to
   the deploy scripts, has its own `hodoo/README.md`, and is verified by Cargo. A
   Topcoat web app (`hodoo-web`) is expected to join the workspace as a third member.

### hodoo commands and toolchain

Rust 1.98 is installed under `~/.cargo/bin`, which is **not on `PATH`** in a plain
shell: run `~/.cargo/bin/cargo` or `export PATH="$HOME/.cargo/bin:$PATH"` first. All
Cargo commands run from `hodoo/`.

| Command | Does |
|---|---|
| `cargo test` | Unit tests, stub-HTTP tests (`tests/http.rs`, wiremock), doctests |
| `cargo clippy --all-targets -- -D warnings` | The lint gate; `unwrap_used`/`expect_used` are warnings, so no unwrapping outside tests |
| `just hodoo-check` | The whole gate: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` |
| `just hodoo -- <args>` | Runs the client (`just hodoo -- task ls --project acme`); builds it first, quietly |
| `just hodoo-test` / `hodoo-live-test` / `hodoo-prod-build` / `hodoo-install` / `hodoo-doctor` / `hodoo-clean` | Offline tests · the two server suites · release build against the lock file · install onto PATH · is this checkout ready? · build output and /tmp scratch |
| `cargo fmt --check` | Formatting (run `cargo fmt` to fix) |
| `HODOO_LIVE=1 cargo test -- --ignored --nocapture` | The two ignored suites: `tests/live.rs` (creates and deletes real records) and `tests/drift.rs` (field names vs Odoo's `/doc-bearer/<model>.json`; needs a Settings-level key, otherwise it prints a skip) |

### CLI conventions (the human-first rework)

The CLI is for people: a table by default, colour only on a terminal, deadlines as
`in 3d`, and an id **or a name** wherever a record is referenced (`--project acme`,
`--stage Review`, `--tag urgent`; `--tag` creates a missing tag). Scripts opt into JSON
with `-o json` / `--json` / `HODOO_OUTPUT=json` — worth setting once at the top of a
script. `docs/superpowers/specs/2026-09-28-hodoo-cli-ux-design.md` records why, with the
clig.dev rules behind each choice; `hodoo/README.md` is the user-facing version.

Things that will bite an agent writing commands into a script:

- **`rm` refuses without `-f` when stdin is not a terminal** (and without a `y` on one).
  `--no-input` makes it fail with the reason instead of hanging.
- **`-n`/`--dry-run` sends nothing** — mutations print what they *would* send. Use it to
  check a command's shape without touching data.
- **A change reports itself on stdout in JSON mode**: `{"id":31}` for a create,
  `{"id":31,"ok":true}` for an update, `{"deleted":31,"ok":true}` for a delete. In table
  mode the confirmation goes to stderr instead, so stdout stays parseable either way.
- **`call` takes its body as `--body`**, not `--json` (`--json` is the output flag now).
- Names that match several records are an error listing them, never a guess.

Credentials resolve flag, then process environment (`ODOO_URL`, `ODOO_API_KEY`,
`ODOO_DB`), then a `.env` at or above the working directory. This repo's `.env` holds
`ODOO_API_KEY` (and `ODOO_URL`), is gitignored, and is read into a map rather than
exported: `std::env::set_var` is unsafe in edition 2024 and the crate forbids unsafe
code. The same layering is available to library callers as `Config::from_env()` plus
`hodoo::dotenv`. JSON output uses **Odoo's** field names (`date_deadline`, `user_ids`,
`privacy_visibility`, `type_ids`) in both directions, which is what `#[serde(rename)]`
on the read structs is for.

`hodoo/scenarios/startup-founder.sh {up|down|show}` (or `just scenario up`) builds a
4-project dataset over the CLI, asserting 46 properties as it goes: two client websites, an
internal product and personal life, with task stages, tags, milestones, subtasks, dependency
chains, chatter and every state. Everything it creates carries a `(scenario)` marker, and
`down` deletes by that marker only, so it can never touch Odoo's own records. `up` rebuilds
from scratch (it runs `down` first) and `down` is safe to run twice. It needs the CLI built:
`cargo build` in `hodoo/` first.

`hodoo version --url <server>` needs no API key and is the cheapest way to check a
server is reachable; `hodoo whoami` proves url, certificate and key together (it answers
`res.users/context_get`, whose `uid` is the key's user). Failures exit `1`
(Odoo/transport) or `2` (usage/config) with a JSON object on stderr; a field Odoo does
not have comes back as `{"kind":"odoo","status":500,"message":"Invalid field ..."}`, so
the escape hatch tells you when a model changed.

### JSON-2 facts to know before touching `hodoo/`

- The whole API is `POST /json/2/<model>/<method>`, `Authorization: Bearer <api key>`
  (`auth='bearer'`), named arguments at the top level of the body plus optional `ids` and
  `context`. `X-Odoo-Database` is only needed with several databases behind one domain,
  so it is opt-in.
- `create` takes `vals_list` and **answers a list of ids** (`[7]`), because JSON-2
  reduces a returned recordset to its ids. Errors are `{name, message, arguments,
  context, debug}` with a Python exception name and a real HTTP status.
- Keys are per user and last at most three months. There is no password login over
  JSON-2, so nothing here can fall back to one.
- Odoo's own method discovery lives at `/doc` (browser) and `/doc-bearer/*.json`
  (bearer); the latter needs `base.group_system`.
- Odoo returns `false` (not `null`) for an unset `Char`/`Text`/`Html` field and answers
  many2one as `[id, "Name"]`; `crates/hodoo/src/de.rs` is the one place that tolerates
  both. Reads always send an explicit `fields` list, or Odoo returns every computed
  column.
- Only reads are retried (once, on a connection failure or 502/503/504), because this
  deployment's Odoo is OOM-killable and JSON-2 has no idempotency key for writes.

## Commands

`just` recipes are thin one-line wrappers around `deploy.sh` and `scripts/`;
nothing lives only in the justfile, so the scripts work on their own.

| Command | Purpose |
|---|---|
| `just check` / `./scripts/check.sh` | Static, no VM access: `bash -n`, shellcheck, `systemd-analyze verify`, `just --fmt --check`. **Run this before pushing.** |
| `just deploy` / `sudo ./deploy.sh` | Full deploy or re-apply. Idempotent, root required, refuses non-Debian. |
| `just doctor` / `sudo ./scripts/doctor.sh` | Read-only health check. Exit 1 if any check FAILs; warnings alone exit 0. |
| `just upgrade` | `git pull --ff-only` then deploy. Upgrades the *deployment*, not Odoo. |
| `just backup` / `dumps` | Run a backup now, or list the backup directory. |
| `just logs [unit]` | `journalctl -u <unit> -f` (default `odoo`). |
| `just memory` / `timers` / `psql [db]` / `cert` / `reload` | RAM+zram+swap, timers, psql as admin, served cert, `nginx -t` + reload. |

Checklist for any change: `just check` → edit repo → `just deploy` → `just doctor`.
nginx-only changes still need the file edited in `configs/` (the live config is a
symlink); `just reload` applies it without a full deploy.

### `just check` needs a Debian-shaped host

`systemd-analyze verify` resolves unit paths, so on a dev box it reports bogus failures
(`Command /opt/odoo/venv/bin/python is not executable`, `Unit postgresql.service not
found`) and `check` exits 1. That is expected, not a real finding. shellcheck is a hard
dependency for full value: if it is missing the step prints `skipped` and the overall
exit status still reports success, so a "passed" run on a machine without shellcheck
did not actually lint the shell. `check` deliberately does not run `nginx -t` (the
deploy does, against the real config).

## Architecture and control flow

```
Internet → nginx :443 (TLS, gzip, disk cache for /web/assets/ only)
         → Odoo :8069 (loopback, workers = 0, 1 cron thread)
         → PostgreSQL 17 (unix socket, peer auth only)
```

`deploy.sh` is one top-to-bottom script with 12 numbered sections, matching the step
table in the README. Control-flow facts that matter when editing it:

- **Tunables live at the top** of `deploy.sh`: `ODOO_COMMIT`, `PG_MAJOR`, `MODULES`,
  `SWAPFILE`, `SWAP_SIZE`. Change them there, not inline.
- **Expensive steps are gated on marker files**, so re-runs are seconds: pip install on
  `/opt/odoo/.requirements.sha256` (a hash of the checked-out `requirements.txt`, which
  only changes when `ODOO_COMMIT` does), module init on `/var/lib/odoo/.initialized`.
  Everything else re-runs every time, which is what makes an edit take effect.
- **`set -euo pipefail`** in `deploy.sh`; `scripts/check.sh` and `scripts/doctor.sh` use
  `set -uo pipefail` on purpose, so one failing check is collected into `status`/`fails`
  instead of aborting the run. Keep that distinction.
- **`die`/`log` helpers** provide colored output; `die` writes to stderr and exits
  non-zero. `doctor.sh` uses `ok`/`warn`/`bad`/`info` and exits `fails > 0 ? 1 : 0`.
- **Post-conditions are asserted, not assumed.** The deploy proves zram is active, that
  PostgreSQL accepts a connection after `pg_hba.conf` is applied (a bad `pg_hba.conf`
  does not stop PostgreSQL from starting, only from connecting), that nginx answers on
  both `127.0.0.1` and `[::1]`, that ufw is active with exactly 22/80/443, that the
  fail2ban jail loaded, and it runs one real backup and checks a dump exists. If you add
  a step, add its assertion; a step nobody verifies is a step that silently fails.

## Non-obvious couplings (change both sides or the deploy rots)

- **Adding or removing a symlinked config**: `deploy.sh` installs it, and `doctor.sh`'s
  `SYMLINKS` array must list it or drift detection is blind. Verify with
  `readlink -f` on both sides as the array does.
- **Database name has exactly one owner**: `db_name` in `configs/odoo.conf`, parsed with
  `awk -F' = ' '/^db_name/{print $2}'` by `deploy.sh` *and* `scripts/backup.sh` (which
  refuses to guess rather than dump the wrong DB) *and* `doctor.sh`. Never hardcode the
  literal `odoo` as a database name.
- **Backup artifact naming** (`odoo_<db>_<stamp>.dump`, `odoo_filestore_<stamp>.tgz`) is
  produced by `backup.sh` and globbed by `doctor.sh`. Change one, change the other. The
  write protocol in `backup.sh` matters: dump to `.partial`, prove it with
  `pg_restore -l`, then `mv` into place, so a killed run never leaves something that
  looks like a backup. `TimeoutStartSec=infinity` in `configs/backup.service` exists so
  a large dump is not SIGTERMed at systemd's 90 s default.
- **README step table / "What the deploy does"** is meant to track `deploy.sh` section by
  section. A behavior change that skips the README leaves the doc of record wrong.
- **Config file headers**: every file in `configs/` opens with a
  `# Managed by odoo-512mb repo (symlinked to <live path>)` comment. Keep that pattern,
  including the target path, on any new file.
- **`PGDIR`/`/etc/postgresql/17/main`** is hardcoded in `deploy.sh` and `doctor.sh`;
  `PG_MAJOR` is a tunable in `deploy.sh` only. Bumping the major version means touching
  both scripts and `configs/postgresql.conf`.

## Gotchas

- **The nginx proxy cache key has no session or cookie component.** Only content-hashed
  `/web/assets/` URLs may be proxy-cached; caching `/web/static/`, `/web/image/`,
  `/web/content/` or anything session-scoped serves one user's response to the next
  requester. This is why the vhost caches exactly one location.
- **The nginx `access_log` format is load-bearing**: it is set explicitly to `combined`
  because the fail2ban filter (`configs/fail2ban-filter-odoo.conf`) parses that format.
  Changing it silently stops brute-force bans. The filter's premise is also Odoo-specific:
  a failed form login re-renders with HTTP 200 (422 for some form errors) and a success
  redirects 302, so `POST /web/login` answering 200/422 means failure.
- **ufw rules live in `deploy.sh`**, not a config file, because ufw rewrites
  `/etc/ufw/user.rules` itself; there is nothing to symlink. Edit the script and re-run.
  Rules are applied before `ufw --force enable` so the deploy's own SSH session is not
  cut; keep that ordering, and keep 22 in the allowed list.
- **Logical replication entries are intentionally omitted** from `configs/pg_hba.conf`,
  and TCP is `reject` rather than password-checked. `doctor.sh` fails if a TCP login
  succeeds, which is the signal that the pinned file was replaced by a package upgrade.
- **Changing `MODULES` requires `rm /var/lib/odoo/.initialized`** then a re-deploy;
  otherwise module init never runs again (it re-runs as an update on the existing DB).
- **Odoo version is pinned to `ODOO_COMMIT`** deliberately. `just upgrade`/`git pull`
  moves the deployment only. Never edit the installed copy under `/opt/odoo`; bump the
  commit instead, after `just backup`.
- **`just check` also enforces justfile formatting** (`just --fmt --check`). Run
  `just --fmt --justfile justfile` after editing the justfile or check fails.
- **`doctor.sh` warns when the repo working tree is dirty** — because the documented
  upgrade path is `git pull --ff-only`, which a dirty tree would refuse.
- **Debian 13 + PostgreSQL 17 only.** `deploy.sh` dies on any other distro and warns on
  another version. Node.js is deliberately absent (only needed for RTL locales).
- **No CI config exists in this repo** even though the README calls `check.sh` "the CI
  job" — treat `just check` as a manual gate to run before pushing.

## Style conventions

- Shell: `#!/usr/bin/env bash`, UPPER_CASE variables, `==>` colored step headers,
  section separators as comment banners. Comments explain *why* a limit exists (the
  specific failure it prevents), not what the line does — match that tone; it is the
  most consistent thing about this codebase.
- **ASCII `--` instead of em dashes** throughout shell and config files, and inside
  Markdown prose as well. Keep it.
- Never write a secret into `configs/`; use a placeholder (see `__GENERATE__` in
  `configs/odoo.conf`) and render it at deploy time, keeping the value outside the repo
  (`/etc/odoo/.admin_passwd`) so the tree stays clean.
- Deliberate simplifications with a known ceiling carry a `ponytail:` comment naming the
  ceiling and the upgrade path (see `OOMScoreAdjust`/`MemoryMax` in
  `configs/odoo.service`).
- Commit messages: imperative, lowercase subject describing the outcome (e.g. "listen on
  IPv6 so an IPv6-only VM is reachable"), then a body explaining the failure that
  motivated the change and what now prevents it. The commits here were authored through
  Crush and carry its attribution footer; follow the configured commit format of whichever
  CLI you are running in.