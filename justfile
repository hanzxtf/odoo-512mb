# Odoo 19 on a 512 MB Debian 13 VM.
#
# Thin wrappers around deploy.sh and scripts/ -- run `just` to list them.
# Deploying, upgrading or remediating always means editing a file in this repo
# and re-running a recipe: nothing here edits the machine directly.

set shell := ["bash", "-uc"]
# Recipe arguments reach the shell as "$@", so `just hodoo -- --name "two words"`
# arrives as one argument instead of three.
set positional-arguments

repo := justfile_directory()

# Cargo is not always on PATH: it usually lives in ~/.cargo/bin here.
hodoo_env := 'PATH="$PATH:$HOME/.cargo/bin"'
hodoo_manifest := repo / "hodoo/Cargo.toml"
hodoo_debug := repo / "hodoo/target/debug/hodoo"
hodoo_release := repo / "hodoo/target/release/hodoo"

# List the recipes
default:
    @just --list

# Static checks, before touching the VM: bash -n, shellcheck, systemd units
check:
    {{ repo }}/scripts/check.sh

# Deploy or re-apply the whole stack (idempotent; needs root)
deploy:
    sudo {{ repo }}/deploy.sh

# Health check of the running box (read-only, needs root)
doctor:
    sudo {{ repo }}/scripts/doctor.sh

# Upgrade path: pull the repo, then re-apply (ODOO_COMMIT pins Odoo itself)
upgrade:
    git -C {{ repo }} pull --ff-only
    sudo {{ repo }}/deploy.sh

# Run one backup now, then show the directory and the run log
backup:
    sudo systemctl start odoo-backup.service
    ls -lt /var/backups/odoo | head -5
    journalctl -u odoo-backup -n 10 --no-pager

# Backup directory, newest first
dumps:
    ls -lt /var/backups/odoo | head -20

# Follow a service log: just logs nginx
logs unit="odoo":
    journalctl -u {{ unit }} -f -n 100

# Timers (backups, apt) and when they fire next
timers:
    systemctl list-timers --all --no-pager

# RAM, zram, swap, and per-service memory
memory:
    free -h
    zramctl
    swapon --show
    systemd-cgtop -n 1

# Open a psql session as the admin (just psql, or: just psql mydb)
psql db="odoo":
    sudo -u postgres psql -w {{ db }}

# Certificate currently served: subject and expiry
cert:
    openssl x509 -in /etc/nginx/ssl/odoo.crt -noout -subject -issuer -enddate

# Validate the nginx config, then reload
reload:
    sudo nginx -t
    sudo systemctl reload nginx
# The startup-founder dataset in Odoo: just scenario up | down | show
scenario action="show":
    {{ repo }}/hodoo/scenarios/startup-founder.sh {{ action }}

# ---------------------------------------------------------------------------
# hodoo: the Rust client for Odoo's JSON-2 API (see hodoo/README.md).
#
# Everything above manages the machine; these manage the client. `just hodoo --
# task ls --project acme` runs it, building first if it has to. The recipes that
# touch a server read ODOO_URL/ODOO_API_KEY from the environment or a .env.
# ---------------------------------------------------------------------------

# Build the client (debug); quiet, because it is a step in other recipes
hodoo-build:
    @{{ hodoo_env }} cargo build --quiet --manifest-path {{ hodoo_manifest }}

# Run the client: just hodoo -- task ls --project acme ("--" is optional)
hodoo *ARGS: hodoo-build
    @if [ "${1:-}" = "--" ]; then shift; fi; {{ hodoo_debug }} "$@"

# Formatting, lints and the offline tests: run this before pushing
hodoo-check:
    {{ hodoo_env }} cargo fmt --manifest-path {{ hodoo_manifest }} --all --check
    {{ hodoo_env }} cargo clippy --manifest-path {{ hodoo_manifest }} --all-targets -- -D warnings
    {{ hodoo_env }} cargo test --manifest-path {{ hodoo_manifest }}

# The offline test suite alone: units, stub-server, help text, end-to-end UX
hodoo-test:
    {{ hodoo_env }} cargo test --manifest-path {{ hodoo_manifest }}

# The two suites that need a real server: they read .env or the environment
hodoo-live-test:
    HODOO_LIVE=1 {{ hodoo_env }} cargo test --manifest-path {{ hodoo_manifest }} -- --ignored --nocapture

# Release build, against the committed lock file
hodoo-prod-build:
    {{ hodoo_env }} cargo build --release --locked --manifest-path {{ hodoo_manifest }}

# Install the release binary onto PATH: needs root for /usr/local/bin
hodoo-install prefix="/usr/local/bin": hodoo-prod-build
    sudo install -D -m 0755 {{ hodoo_release }} {{ prefix }}/hodoo
    @echo "installed {{ prefix }}/hodoo"
    @{{ prefix }}/hodoo --version

# Is the client ready? Toolchain, binary, credentials, server, and who the key is
hodoo-doctor:
    #!/usr/bin/env bash
    set -uo pipefail
    printf '%-12s' 'cargo'
    cargo --version 2>/dev/null || "$HOME/.cargo/bin/cargo" --version 2>/dev/null || echo 'not found (install Rust 1.85+)'
    printf '%-12s' 'binary'
    if [ -x '{{ hodoo_debug }}' ]; then
      echo '{{ hodoo_debug }}'
    else
      echo 'not built yet: just hodoo-build'
    fi
    printf '%-12s' 'credentials'
    if [ -f .env ]; then
      # Names only: a key must never be printed.
      grep -oE '^[A-Z_]+=' .env | tr -d '=' | paste -sd' ' -
    else
      echo 'no .env: ODOO_URL and ODOO_API_KEY must come from the environment'
    fi
    printf '%-12s' 'output'
    echo "HODOO_OUTPUT=${HODOO_OUTPUT:-table (default)}"
    if [ -x '{{ hodoo_debug }}' ]; then
      printf '%-12s' 'server'
      if '{{ hodoo_debug }}' -q version 2>/dev/null; then
        printf '%-12s' 'identity'
        '{{ hodoo_debug }}' -q whoami -o json 2>/dev/null |
          python3 -c 'import json,sys; d=json.load(sys.stdin); print(f"{d["name"]} (uid {d["uid"]})")' ||
          echo 'could not identify the key: check --url/ODOO_URL and the key'
      else
        echo 'could not reach the server: check --url/ODOO_URL and the certificate (--insecure)'
      fi
    fi
    printf '%-12s' 'scenario'
    if [ "$({{ hodoo_debug }} project ls '(scenario)' --limit 0 --no-headers 2>/dev/null | wc -l)" = '0' ]; then
      echo 'not loaded: just scenario up'
    else
      echo 'loaded: just scenario show'
    fi

# Remove build output and the scenario's scratch files under /tmp
hodoo-clean:
    {{ hodoo_env }} cargo clean --manifest-path {{ hodoo_manifest }}
    rm -f /tmp/hodoo-scenario-auth.json /tmp/hodoo-scenario-error.json
    @echo 'cleaned hodoo/target and the scenario scratch files'
