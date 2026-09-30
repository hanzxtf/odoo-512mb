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