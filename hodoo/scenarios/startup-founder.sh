#!/usr/bin/env bash
#
# A startup founder's Odoo, through hodoo: two client websites, the product
# itself, and personal life.
#
#   ./startup-founder.sh up     create (or rebuild) the whole dataset
#   ./startup-founder.sh down   delete exactly what this script created
#   ./startup-founder.sh show   read-only dashboard over the dataset
#
# `up` asserts what it created, so it doubles as the test case: every number it
# checks, it printed first. Nothing is deleted unless you run `down`.
#
# Everything it makes carries a "(scenario)" marker in its name: projects,
# tasks, task stages, tags, milestones and the two client contacts. `down` finds
# records by that marker, so it can only ever delete its own data, and it is safe
# to run twice. Odoo's own records (the project stages To Do / In Progress /
# Done / Cancelled, the default task stages, the users) are reused, never
# recreated and never touched.
#
# Credentials come from the environment or a .env, as everywhere else in hodoo.
#
# Dataset
#   contacts    Acme Manufacturing, Nordwind Studio
#   projects    Acme Manufacturing - Website   customer Acme,     8 tasks
#               Nordwind Studio - Site         customer Nordwind, 5 tasks
#               Product - SaaS MVP             internal,         5 tasks
#               Personal Life                  internal,         5 tasks
#   18 task stages, 6 tags, 4 milestones, subtasks, a dependency chain per
#   project, one overdue, one waiting, one changes-requested, one canceled task,
#   and both kinds of chatter.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HODOO="${HODOO_BIN:-$ROOT/hodoo/target/debug/hodoo}"
MARKER="(scenario)"

# This is a script, so it asks for JSON once and reads it with python. A person
# running `hodoo` by hand gets tables; `show` below unsets this on purpose.

if [ ! -x "$HODOO" ]; then
  echo "no hodoo binary at $HODOO: run 'cargo build' in $ROOT/hodoo first" >&2
  exit 2
fi
cd "$ROOT"
# Everything below reads the CLI with python, so the script asks for JSON once.
export HODOO_OUTPUT=json

# --- helpers ---------------------------------------------------------------

step() { printf '\n\033[1;32m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }
die() { printf '\033[1;31mERROR: %s\033[0m\n' "$*" >&2; exit 1; }

# Reads JSON on stdin and prints the value at a path, e.g. `.id` or `[0].name`.
# Containers and booleans print as JSON (`true`, `[]`), scalars print bare.
field() {
  python3 -c '
import json, re, sys
data = json.load(sys.stdin)
for token in re.findall(r"[^.\[\]]+|\[\d+\]", sys.argv[1]):
    data = data[int(token[1:-1])] if token.startswith("[") else data[token]
print(json.dumps(data) if isinstance(data, (bool, type(None), list, dict)) else data)
' "$1"
}

# Reads a JSON array of records on stdin and prints their ids, space separated.
ids() { python3 -c 'import json,sys; print(" ".join(str(row["id"]) for row in json.load(sys.stdin)))'; }

# Same, for a JSON array of bare ids.
csv() { python3 -c 'import json,sys; print(",".join(str(i) for i in json.load(sys.stdin)))'; }

expect() { # expect <expected> <actual> <what>
  if [ "$1" != "$2" ]; then
    printf '\033[1;31m  FAIL\033[0m %s: expected %s, got %s\n' "$3" "$1" "$2" >&2
    exit 1
  fi
  printf '  \033[32mok\033[0m   %s = %s\n' "$3" "$2"
}

count() { python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'; }

# Odoo's own project stages, reused rather than recreated.
project_stage() { # project_stage <name>
  "$HODOO" call project.project.stage search_read \
    --body "{\"domain\":[[\"name\",\"=\",\"$1\"]],\"fields\":[\"id\"],\"limit\":1}" | field '[0].id'
}

today() { date -d "$1" +%F; }

# Tasks are assigned to the automation user, whose partner has no email address:
# a scenario that posts comments must not be able to mail a real person.
auth() {
  # `-o json` explicitly: this helper runs in both modes (the dashboard unsets
  # HODOO_OUTPUT), and it is the one place the script needs a parseable answer.
  if ! whoami_json="$("$HODOO" whoami -o json 2>/tmp/hodoo-scenario-auth.json)"; then
    die "hodoo could not authenticate: $(head -c 300 /tmp/hodoo-scenario-auth.json)"
  fi
  AGENT="$(printf '%s' "$whoami_json" | field .uid)"
}

# --- teardown --------------------------------------------------------------

# Deletes every record this script created. Safe to run when there is none: each
# step is a domain search, so an empty result is a no-op.
down() {
  step "removing scenario data"

  local projects tags contacts stage_ids own_stages
  projects="$("$HODOO" call project.project search \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  tags="$("$HODOO" call project.tags search \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  contacts="$("$HODOO" call res.partner search \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  own_stages="$("$HODOO" call project.project.stage search \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  stage_ids=""

  # Task stages are shared, so they are not deleted with the project: collect
  # the ones attached to our projects before those projects go away. Their tasks
  # cascade with the project, which is also what releases the stages.
  local project
  for project in $(printf '%s' "$projects" | tr ',' ' '); do
    stage_ids="$stage_ids $("$HODOO" project task-stages ls "$project" | ids)"
  done
  stage_ids="$(printf '%s' "$stage_ids" | tr -s ' ' | sed 's/^ *//;s/ *$//' | tr ' ' ',')"

  if [ -n "$projects" ]; then
    note "projects:       $projects"
    "$HODOO" call project.project unlink --body "{\"ids\":[$projects]}" > /dev/null
  fi
  if [ -n "$stage_ids" ]; then
    note "task stages:    $stage_ids"
    "$HODOO" call project.task.type unlink --body "{\"ids\":[$stage_ids]}" > /dev/null
  fi
  if [ -n "$tags" ]; then
    note "tags:           $tags"
    "$HODOO" call project.tags unlink --body "{\"ids\":[$tags]}" > /dev/null
  fi
  if [ -n "$contacts" ]; then
    note "contacts:       $contacts"
    "$HODOO" call res.partner unlink --body "{\"ids\":[$contacts]}" > /dev/null
  fi
  if [ -n "$own_stages" ]; then
    note "project stages: $own_stages"
    "$HODOO" call project.project.stage unlink --body "{\"ids\":[$own_stages]}" > /dev/null
  fi

  expect 0 "$("$HODOO" call project.project search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario projects left behind"
  expect 0 "$("$HODOO" call project.task search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario tasks left behind"
  expect 0 "$("$HODOO" call project.task.type search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario task stages left behind"
  expect 0 "$("$HODOO" call res.partner search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario contacts left behind"
}

# --- the dataset -----------------------------------------------------------

up() {
  auth
  note "acting as uid $AGENT on Odoo $("$HODOO" version | field .version)"

  # Rebuilding rather than duplicating: the names are stable, so a second `up`
  # would otherwise collide with the first.
  if [ "$("$HODOO" call project.project search_count \
        --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" != "0" ]; then
    note "an earlier run is present: removing it first"
    down
  fi

  # -- contacts -------------------------------------------------------------
  # Creating contacts needs Contacts > Creation (`base.group_partner_manager`),
  # which a project administrator does not have by default. Without it the two
  # client projects are still built, just without a customer link.
  step "clients"
  local acme="" nordwind="" have_contacts=1
  if ! acme_reply="$("$HODOO" call res.partner create --body "{\"vals_list\":[{
      \"name\":\"Acme Manufacturing $MARKER\",
      \"is_company\":true,
      \"email\":\"ops@acme.example\",
      \"phone\":\"+31 20 555 0100\"
    }]}" 2>&1)"; then
    have_contacts=0
    note "this user may not create contacts: $(printf '%s' "$acme_reply" | head -c 180)"
    note "granting Contacts > Creation (base.group_partner_manager) gives the full dataset"
  else
    acme="$(printf '%s' "$acme_reply" | field '[0]')"
    nordwind="$("$HODOO" call res.partner create --body "{\"vals_list\":[{
      \"name\":\"Nordwind Studio $MARKER\",
      \"is_company\":true,
      \"email\":\"hello@nordwind.example\"
    }]}" | field '[0]')"
    expect "Acme Manufacturing $MARKER" \
      "$("$HODOO" call res.partner read --ids "$acme" --body '{"fields":["name"]}' | field '[0].name')" \
      "the client contact reads back"
    note "Acme $acme, Nordwind $nordwind"
  fi

  local customer_of_acme=() customer_of_nordwind=()
  if [ "$have_contacts" = 1 ]; then
    customer_of_acme=(--customer "$acme")
    customer_of_nordwind=(--customer "$nordwind")
  fi

  # -- tags -----------------------------------------------------------------
  # Tags are created on the spot by `--tag <name>`, so nothing to do here but
  # check at the end that they exist.
  step "tags"

  # -- projects -------------------------------------------------------------
  step "projects"
  local p_acme p_nordwind p_product p_personal
  p_acme="$("$HODOO" project create \
    --name "Acme Manufacturing - Website $MARKER" \
    "${customer_of_acme[@]}" --manager "$AGENT" \
    --stage "$(project_stage 'In Progress')" \
    --visibility employees \
    --start "$(today '-5 days')" --end "$(today '+60 days')" \
    --milestones --dependencies \
    --tag "client $MARKER" --tag "website $MARKER" \
    --description '<p>Marketing site for Acme: 8 pages, CMS, multi-language.</p>' | field .id)"
  p_nordwind="$("$HODOO" project create \
    --name "Nordwind Studio - Site $MARKER" \
    "${customer_of_nordwind[@]}" --manager "$AGENT" \
    --stage "$(project_stage 'To Do')" \
    --visibility followers \
    --start "$(today '+3 days')" --end "$(today '+75 days')" \
    --dependencies \
    --tag "client $MARKER" --tag "website $MARKER" \
    --description '<p>Portfolio site for Nordwind: 5 sections, photography-led.</p>' | field .id)"
  p_product="$("$HODOO" project create \
    --name "Product - SaaS MVP $MARKER" \
    --manager "$AGENT" \
    --stage "$(project_stage 'In Progress')" \
    --visibility employees \
    --start "$(today '-20 days')" --end "$(today '+120 days')" \
    --dependencies \
    --tag "growth $MARKER" \
    --description '<p>Our own product: auth, billing, onboarding.</p>' | field .id)"
  p_personal="$("$HODOO" project create \
    --name "Personal Life $MARKER" \
    --visibility followers \
    --start "$(today '+1 day')" \
    --tag "home $MARKER" \
    --description '<p>Anything that is not work.</p>' | field .id)"
  expect 4 "$("$HODOO" call project.project search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario projects"
  expect 2 "$("$HODOO" project ls --tag "client $MARKER" --limit 0 | count)" "projects carrying the client tag"

  # The fields a founder actually cares about, and that Odoo defaults.
  expect "employees" "$("$HODOO" project show "$p_acme" | field .privacy_visibility)" "acme visibility"
  expect "followers" "$("$HODOO" project show "$p_nordwind" | field .privacy_visibility)" "nordwind visibility"
  if [ "$have_contacts" = 1 ]; then
    expect "$acme" "$("$HODOO" project show "$p_acme" | field .partner_id)" "acme's customer link"
    expect 1 "$("$HODOO" project ls --customer "$acme" --limit 0 | count)" \
      "the customer filter finds exactly Acme's project"
  else
    note "skipping the customer assertions: no Contacts > Creation right"
  fi

  # -- task stages ----------------------------------------------------------
  # A project starts with no task stages, so a staged task is impossible until
  # they exist: this is the Odoo behaviour hodoo documents.
  step "task stages"
  expect "[]" "$("$HODOO" project show "$p_acme" | field .type_ids)" \
    "a fresh project starts with no task stages"

  local a_backlog a_design a_build a_review a_live
  a_backlog="$("$HODOO" project task-stages create --name "Backlog $MARKER" --project "$p_acme" --sequence 10 | field .id)"
  a_design="$("$HODOO" project task-stages create --name "Design $MARKER" --project "$p_acme" --sequence 20 | field .id)"
  a_build="$("$HODOO" project task-stages create --name "Build $MARKER" --project "$p_acme" --sequence 30 | field .id)"
  a_review="$("$HODOO" project task-stages create --name "Review $MARKER" --project "$p_acme" --sequence 40 | field .id)"
  a_live="$("$HODOO" project task-stages create --name "Live $MARKER" --project "$p_acme" --sequence 50 --fold | field .id)"

  local n_inbox n_doing n_blocked n_done
  n_inbox="$("$HODOO" project task-stages create --name "Inbox $MARKER" --project "$p_nordwind" --sequence 10 | field .id)"
  n_doing="$("$HODOO" project task-stages create --name "Doing $MARKER" --project "$p_nordwind" --sequence 20 | field .id)"
  n_blocked="$("$HODOO" project task-stages create --name "Blocked $MARKER" --project "$p_nordwind" --sequence 30 | field .id)"
  n_done="$("$HODOO" project task-stages create --name "Done $MARKER" --project "$p_nordwind" --sequence 40 --fold | field .id)"

  local d_ideas d_ready d_building d_qa d_released
  d_ideas="$("$HODOO" project task-stages create --name "Ideas $MARKER" --project "$p_product" --sequence 10 | field .id)"
  d_ready="$("$HODOO" project task-stages create --name "Ready $MARKER" --project "$p_product" --sequence 20 | field .id)"
  d_building="$("$HODOO" project task-stages create --name "Building $MARKER" --project "$p_product" --sequence 30 | field .id)"
  d_qa="$("$HODOO" project task-stages create --name "QA $MARKER" --project "$p_product" --sequence 40 | field .id)"
  d_released="$("$HODOO" project task-stages create --name "Released $MARKER" --project "$p_product" --sequence 50 --fold | field .id)"

  local h_someday h_week h_doing h_done
  h_someday="$("$HODOO" project task-stages create --name "Someday $MARKER" --project "$p_personal" --sequence 10 | field .id)"
  h_week="$("$HODOO" project task-stages create --name "This week $MARKER" --project "$p_personal" --sequence 20 | field .id)"
  h_doing="$("$HODOO" project task-stages create --name "Doing $MARKER" --project "$p_personal" --sequence 30 | field .id)"
  h_done="$("$HODOO" project task-stages create --name "Done $MARKER" --project "$p_personal" --sequence 40 --fold | field .id)"

  expect 18 "$("$HODOO" call project.task.type search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario task stages"
  expect 5 "$("$HODOO" project task-stages ls "$p_acme" | count)" "stages attached to Acme"
  expect "true" "$("$HODOO" project task-stages ls "$p_acme" \
    | python3 -c 'import json,sys; print(str(any(s["fold"] for s in json.load(sys.stdin))).lower())')" \
    "one Acme stage is folded"

  # attach/detach: one stage shared with a second project, then given back.
  "$HODOO" project attach "$p_nordwind" --task-stage "$a_design" > /dev/null
  expect 5 "$("$HODOO" project task-stages ls "$p_nordwind" | count)" "Nordwind borrowed the Design stage"
  "$HODOO" project detach "$p_nordwind" --task-stage "$a_design" > /dev/null
  expect 4 "$("$HODOO" project task-stages ls "$p_nordwind" | count)" "and gave it back"

  # -- milestones -----------------------------------------------------------
  step "milestones"
  local m_signoff m_launch m_freeze m_beta
  m_signoff="$("$HODOO" milestone create --project "$p_acme" --name "Design sign-off $MARKER" \
    --due "$(today '+7 days')" | field .id)"
  m_launch="$("$HODOO" milestone create --project "$p_acme" --name "Launch $MARKER" \
    --due "$(today '+55 days')" | field .id)"
  m_freeze="$("$HODOO" milestone create --project "$p_nordwind" --name "Content freeze $MARKER" \
    --due "$(today '+25 days')" | field .id)"
  m_beta="$("$HODOO" milestone create --project "$p_product" --name "Private beta $MARKER" \
    --due "$(today '+40 days')" | field .id)"
  expect 4 "$("$HODOO" call project.milestone search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario milestones"

  # -- Acme: the full build chain ------------------------------------------
  step "Acme: discovery to launch, on a dependency chain"

  local kickoff wireframes homepage product_page audit mobile review_call popup
  kickoff="$("$HODOO" task create --name "Kickoff and discovery notes $MARKER" \
    --project "$p_acme" --stage "$a_backlog" --priority low --assignee "$AGENT" \
    --hours 4 --tag "client $MARKER" \
    --description '<p>Call notes, goals, constraints.</p>' | field .id)"
  wireframes="$("$HODOO" task create --name "Wireframes for all 8 pages $MARKER" \
    --project "$p_acme" --stage "$a_design" --priority high --assignee "$AGENT" \
    --due "$(today '+5 days')" --hours 12 \
    --tag "client $MARKER" --tag "website $MARKER" --milestone "$m_signoff" | field .id)"
  homepage="$("$HODOO" task create --name "Homepage build $MARKER" \
    --project "$p_acme" --stage "$a_build" --priority medium --assignee "$AGENT" \
    --due "$(today '+12 days')" --hours 16 --tag "website $MARKER" \
    --depends-on "$wireframes" | field .id)"
  product_page="$("$HODOO" task create --name "Product page build $MARKER" \
    --project "$p_acme" --stage "$a_build" --priority medium --assignee "$AGENT" \
    --due "$(today '+18 days')" --hours 14 --tag "website $MARKER" \
    --depends-on "$wireframes" | field .id)"
  audit="$("$HODOO" task create --name "Accessibility audit $MARKER" \
    --project "$p_acme" --stage "$a_review" --priority high --assignee "$AGENT" \
    --due "$(today '+30 days')" --hours 8 \
    --depends-on "$homepage" --depends-on "$product_page" | field .id)"
  mobile="$("$HODOO" task create --name "Mobile breakpoints $MARKER" \
    --project "$p_acme" --stage "$a_design" --priority medium --assignee "$AGENT" \
    --hours 6 --parent "$wireframes" | field .id)"
  review_call="$("$HODOO" task create --name "Client review call $MARKER" \
    --project "$p_acme" --stage "$a_review" --priority high --assignee "$AGENT" \
    --due "$(today '+3 days')" --state waiting --tag "client $MARKER" | field .id)"
  popup="$("$HODOO" task create --name "Newsletter popup, rejected $MARKER" \
    --project "$p_acme" --stage "$a_backlog" --priority low --tag "website $MARKER" | field .id)"

  # The dependency chain is the point of allow_task_dependencies.
  expect "$wireframes" "$("$HODOO" task deps "$homepage" | field '.blocked_by[0]')" \
    "the homepage waits on the wireframes"
  local audit_deps
  audit_deps="$("$HODOO" task deps "$audit" \
    | python3 -c 'import json,sys; print(" ".join(str(i) for i in json.load(sys.stdin)["blocked_by"]))')"
  expect 2 "$(echo "$audit_deps" | wc -w | tr -d ' ')" "the audit waits on two tasks"
  local dep
  for dep in $audit_deps; do
    case "$dep" in
      "$homepage"|"$product_page") ;;
      *) die "the audit waits on an unexpected task: $dep" ;;
    esac
  done
  # Waiting is derived from open dependencies, but Odoo does not compute it while
  # the task is being created: writing the dependencies again is what makes a
  # blocked task read as blocked. This surprised us once; the scenario pins it.
  expect "01_in_progress" "$("$HODOO" task show "$homepage" | field .state)" \
    "a freshly created task with an open blocker is not waiting yet"
  "$HODOO" task update "$homepage" --depends-on "$wireframes" > /dev/null
  expect "04_waiting_normal" "$("$HODOO" task show "$homepage" | field .state)" \
    "writing the dependencies turns it into waiting"
  expect "04_waiting_normal" "$("$HODOO" task show "$review_call" | field .state)" \
    "an explicit waiting state sticks"
  expect "$wireframes" "$("$HODOO" task show "$mobile" | field .parent_id)" "the mobile work is a subtask of the wireframes"

  # State transitions.
  "$HODOO" task done "$kickoff" > /dev/null
  "$HODOO" task cancel "$popup" > /dev/null
  expect "1_done" "$("$HODOO" task show "$kickoff" | field .state)" "the kickoff task is done"
  expect "1_canceled" "$("$HODOO" task show "$popup" | field .state)" "the rejected popup is canceled"
  expect "true" "$("$HODOO" task show "$kickoff" | field .is_closed)" "is_closed follows the state"

  # Moving a task between stages, and escalating it.
  "$HODOO" task update "$product_page" --stage "$a_review" --priority high > /dev/null
  expect "$a_review" "$("$HODOO" task show "$product_page" | field .stage_id)" "the product page moved to Review"
  expect "2" "$("$HODOO" task show "$product_page" | field .priority)" "and was escalated to high"

  # Chatter, both kinds.
  "$HODOO" task comment "$homepage" --body "Waiting on Acme's brand assets before the header can be finished." --internal > /dev/null
  "$HODOO" task comment "$wireframes" --body "Wireframes are ready for your review." > /dev/null
  "$HODOO" project comment "$p_acme" --body "Kickoff done, design in progress." --internal > /dev/null
  expect "true" "$("$HODOO" task messages "$homepage" \
    | python3 -c 'import json,sys; print(str(any("brand assets" in (m["body"] or "") for m in json.load(sys.stdin))).lower())')" \
    "the internal note is on the thread"

  # -- Nordwind: content-led, with a blocker --------------------------------
  step "Nordwind: content, photography, and a blocker"

  local inventory copywriting shoot blocked photos
  inventory="$("$HODOO" task create --name "Content inventory $MARKER" \
    --project "$p_nordwind" --stage "$n_inbox" --priority medium --assignee "$AGENT" \
    --hours 5 --tag "client $MARKER" | field .id)"
  copywriting="$("$HODOO" task create --name "Copywriting for 5 sections $MARKER" \
    --project "$p_nordwind" --stage "$n_doing" --priority high --assignee "$AGENT" \
    --due "$(today '+14 days')" --hours 10 --milestone "$m_freeze" | field .id)"
  shoot="$("$HODOO" task create --name "Photo shoot brief $MARKER" \
    --project "$p_nordwind" --stage "$n_doing" --priority medium --assignee "$AGENT" \
    --due "$(today '+9 days')" --hours 4 --depends-on "$inventory" | field .id)"
  blocked="$("$HODOO" task create --name "Hosting decision $MARKER" \
    --project "$p_nordwind" --stage "$n_blocked" --priority urgent --assignee "$AGENT" \
    --due "$(today '-2 days')" --state changes-requested | field .id)"
  photos="$("$HODOO" task create --name "Pick 12 hero photos $MARKER" \
    --project "$p_nordwind" --stage "$n_doing" --priority low --assignee "$AGENT" \
    --hours 3 --parent "$shoot" --tag "website $MARKER" | field .id)"

  expect "02_changes_requested" "$("$HODOO" task show "$blocked" | field .state)" \
    "the hosting decision is changes-requested"
  expect 5 "$("$HODOO" task ls --project "$p_nordwind" --open --limit 0 | count)" "five Nordwind tasks are open"
  expect 1 "$("$HODOO" task ls --project "$p_nordwind" --parent "$shoot" --limit 0 | count)" \
    "the photo selection is the shoot's only subtask"

  # -- Product: the build chain --------------------------------------------
  step "the product: a chain from the data model to billing"

  local datamodel auth_task billing onboarding pipeline
  datamodel="$("$HODOO" task create --name "Define the data model $MARKER" \
    --project "$p_product" --stage "$d_released" --priority high --assignee "$AGENT" \
    --hours 6 --tag "growth $MARKER" \
    --description '<p>Tenants, users, projects, tasks.</p>' | field .id)"
  auth_task="$("$HODOO" task create --name "Auth: signup and login $MARKER" \
    --project "$p_product" --stage "$d_building" --priority urgent --assignee "$AGENT" \
    --due "$(today '+10 days')" --hours 24 --milestone "$m_beta" \
    --depends-on "$datamodel" | field .id)"
  billing="$("$HODOO" task create --name "Stripe billing $MARKER" \
    --project "$p_product" --stage "$d_qa" --priority high --assignee "$AGENT" \
    --due "$(today '+35 days')" --hours 20 --tag "billing $MARKER" \
    --depends-on "$auth_task" | field .id)"
  onboarding="$("$HODOO" task create --name "Onboarding checklist UI $MARKER" \
    --project "$p_product" --stage "$d_ready" --priority medium --assignee "$AGENT" \
    --hours 12 --depends-on "$auth_task" | field .id)"
  pipeline="$("$HODOO" task create --name "Deploy pipeline $MARKER" \
    --project "$p_product" --stage "$d_building" --priority high --assignee "$AGENT" \
    --hours 10 | field .id)"

  "$HODOO" task done "$datamodel" > /dev/null
  "$HODOO" milestone reached "$m_beta" > /dev/null
  "$HODOO" milestone reached "$m_signoff" > /dev/null
  "$HODOO" task comment "$billing" --body "Beta invite list is at 43 signups." --internal > /dev/null

  expect "true" "$("$HODOO" task show "$datamodel" | field .is_closed)" "the data model is closed"
  expect "true" "$("$HODOO" milestone ls --project "$p_product" | field '[0].is_reached')" \
    "the private beta milestone is reached"
  expect "false" "$("$HODOO" milestone ls --project "$p_acme" | python3 -c '
import json, sys
rows = json.load(sys.stdin)
print(str(next(m["is_reached"] for m in rows if m["name"].startswith("Launch"))).lower())
')" "the launch milestone is still open"

  # -- Personal life --------------------------------------------------------
  step "personal life: private, unassigned, and not work"

  local dentist passport trip hotels tap
  dentist="$("$HODOO" task create --name "Book the dentist $MARKER" \
    --project "$p_personal" --stage "$h_week" --priority high \
    --due "$(today '+2 days')" --tag "health $MARKER" | field .id)"
  passport="$("$HODOO" task create --name "Renew the passport $MARKER" \
    --project "$p_personal" --stage "$h_someday" --priority medium \
    --state waiting --tag "home $MARKER" | field .id)"
  trip="$("$HODOO" task create --name "Plan the weekend trip $MARKER" \
    --project "$p_personal" --stage "$h_doing" --priority medium \
    --due "$(today '+20 days')" | field .id)"
  hotels="$("$HODOO" task create --name "Compare three hotels $MARKER" \
    --project "$p_personal" --stage "$h_doing" --priority low --parent "$trip" | field .id)"
  tap="$("$HODOO" task create --name "Fix the dripping tap $MARKER" \
    --project "$p_personal" --stage "$h_week" --priority low --tag "home $MARKER" \
    --due "$(today '-1 day')" | field .id)"

  # Odoo assigns the calling user to whatever it creates, so "personal" tasks
  # arrive assigned to the automation user. Nobody else may be on them, and
  # clearing them takes an explicit empty list.
  expect "$AGENT" "$("$HODOO" task ls --project "$p_personal" --limit 0 \
    | python3 -c '
import json, sys
owners = sorted({u for t in json.load(sys.stdin) for u in t["user_ids"]})
print(",".join(str(u) for u in owners))
')" "personal tasks are assigned only to the automation user"
  "$HODOO" task update "$tap" --unassign > /dev/null
  expect "[]" "$("$HODOO" task show "$tap" | field .user_ids)" "and can be unassigned again"
  "$HODOO" task update "$tap" --assignee "$AGENT" > /dev/null
  expect "$p_personal" "$("$HODOO" call project.task read --ids "$hotels" --body '{"fields":["project_id"]}' \
    | field '[0].project_id[0]')" "the personal subtask stayed in its project"

  # -- cross-project views --------------------------------------------------
  step "what a founder actually asks"

  # Six tags in total: four on projects, two only ever used by tasks, and all of
  # them created on demand by `--tag`.
  expect 6 "$("$HODOO" call project.tags search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "scenario tags"

  local total open
  total="$("$HODOO" call project.task search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")"
  open="$("$HODOO" task ls "$MARKER" --open --limit 0 | count)"
  expect 23 "$total" "tasks in the dataset"
  expect 20 "$open" "of which are open"
  note "closed: $((total - open)) (done, canceled)"

  local overdue
  overdue="$("$HODOO" task ls "$MARKER" --open --due-before "$(today 'today')" --limit 0 | count)"
  expect 2 "$overdue" "overdue tasks"
  note "overdue: $("$HODOO" task ls "$MARKER" --open --due-before "$(today 'today')" \
    --limit 0 --order date_deadline | python3 -c '
import json, sys
print(", ".join(t["name"] for t in json.load(sys.stdin)))
')"

  expect 5 "$("$HODOO" task ls "$MARKER" --open --due-before "$(today '+7 days')" --limit 0 | count)" \
    "open tasks due within a week"

  # Per-stage load on the biggest project, which is what a board view shows.
  note "the Acme board:"
  "$HODOO" project task-stages ls "$p_acme" | python3 -c '
import json, subprocess, sys
for stage in json.load(sys.stdin):
    counted = subprocess.run(
        ["'"$HODOO"'", "call", "project.task", "search_count", "--body",
         json.dumps({"domain": [["stage_id", "=", stage["id"]], ["is_closed", "=", False]]})],
        capture_output=True, text=True, check=True).stdout
    print("    %-22s %s open" % (stage["name"], json.loads(counted)))
'

  # The escape hatch, for the rollups hodoo deliberately does not wrap.
  note "open work per project:"
  local project
  for project in "$p_acme" "$p_nordwind" "$p_product" "$p_personal"; do
    printf '    %-46s %s open\n' "$("$HODOO" project show "$project" | field .name)" \
      "$("$HODOO" call project.task search_count \
        --body "{\"domain\":[[\"project_id\",\"=\",$project],[\"is_closed\",\"=\",false]]}")"
  done

  # A stage from another project is *accepted*: that rule is a view domain, not
  # a server one. Record the behaviour (and undo it) rather than assert the
  # opposite, which is what this check did until Odoo proved it wrong.
  local probe
  probe="$("$HODOO" task create --name "Wrong-stage probe $MARKER" \
    --project "$p_personal" --stage "$a_build" | field .id)"
  expect "$a_build" "$("$HODOO" task show "$probe" | field .stage_id)" \
    "a foreign stage is accepted (the project/stage rule is UI-only)"
  # ...and using it enrolls that stage into the project, which is how the kanban
  # stays consistent afterwards. Undo both, or the dataset gains a stray stage.
  expect "true" "$("$HODOO" project task-stages ls "$p_personal" \
    | python3 -c "import json,sys; print(str(any(s['id'] == $a_build for s in json.load(sys.stdin))).lower())")" \
    "and the stage is enrolled into the other project"
  # A script has nobody to ask, so it says -f out loud: that is the point of the rule.
  "$HODOO" task rm "$probe" -f > /dev/null
  "$HODOO" project detach "$p_personal" --task-stage "$a_build" > /dev/null
  expect 4 "$("$HODOO" project task-stages ls "$p_personal" | count)" "the stray stage was detached again"

  # What Odoo does refuse: an id that is gone, and a field it does not have.
  set +e
  "$HODOO" task show 999999999 > /dev/null 2>/tmp/hodoo-scenario-error.json
  local missing=$?
  "$HODOO" call project.task read --body '{"fields":["no_such_field"]}' \
    > /dev/null 2>>/tmp/hodoo-scenario-error.json
  local bad_field=$?
  set -e
  expect 1 "$missing" "reading a task that is gone fails"
  expect 1 "$bad_field" "reading a field Odoo does not have fails"
  note "the refusals, verbatim: $(head -c 170 /tmp/hodoo-scenario-error.json)"

  step "up is done"
  cat <<EOF
    projects  Acme $p_acme | Nordwind $p_nordwind | Product $p_product | Personal $p_personal
    stages    $(echo "a_backlog=$a_backlog a_live=$a_live n_done=$n_done h_done=$h_done")
    milestones $m_signoff $m_launch $m_freeze $m_beta
    ./startup-founder.sh show    read-only dashboard
    ./startup-founder.sh down    remove all of the above
EOF
}

# --- read-only dashboard ---------------------------------------------------

show() {
  # The dashboard is for a person, so it unset the script's JSON mode and lets the
  # CLI's own tables do the talking: nothing here parses output.
  unset HODOO_OUTPUT
  auth

  if [ "$("$HODOO" project ls "$MARKER" --limit 0 --no-headers | wc -l)" = "0" ]; then
    echo "no scenario data: run './startup-founder.sh up' first"
    return
  fi

  step "the founder"
  "$HODOO" whoami
  "$HODOO" project ls "$MARKER" --limit 0

  step "boards"
  local project
  for project in "Acme Manufacturing" "Nordwind Studio" "Product - SaaS MVP" "Personal Life"; do
    "$HODOO" board "$project"
    echo
  done

  step "overdue, worst first"
  "$HODOO" task ls "$MARKER" --overdue --order date_deadline --limit 0

  step "waiting or blocked"
  "$HODOO" task ls "$MARKER" --state waiting --limit 0
  "$HODOO" task ls "$MARKER" --state changes-requested --limit 0

  step "milestones"
  for project in "Acme Manufacturing" "Nordwind Studio" "Product - SaaS MVP"; do
    "$HODOO" milestone ls --project "$project"
  done

  step "what waits on what"
  "$HODOO" task deps "Homepage build $MARKER"
  "$HODOO" task deps "Accessibility audit $MARKER"
  "$HODOO" task deps "Stripe billing $MARKER"

  step "one task in full"
  "$HODOO" task show "Product page build $MARKER"
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  show) show ;;
  *)
    cat <<EOF
usage: $(basename "$0") {up|down|show}

  up     create (or rebuild) the startup-founder dataset, asserting it as it goes
  down   delete exactly what 'up' created, found by its "$MARKER" marker
  show   read-only dashboard: clients, projects, overdue work, blockers, dependencies

Binary:      $HODOO  (override with HODOO_BIN)
Credentials: the environment, then a .env, exactly as the CLI itself resolves them.
EOF
    exit 2
    ;;
esac
