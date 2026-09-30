#!/usr/bin/env bash
#
# Due diligence on a fund manager, run as an Odoo project through hodoo:
# 15 workstreams, 5 phases, 4 phase gates, 7 milestones, 79 tasks.
#
#   ./icare-dd.sh up     create (or rebuild) the whole dataset
#   ./icare-dd.sh down   delete exactly what this script created
#   ./icare-dd.sh show   read-only dashboard over the dataset
#
# The point of the shape is the rule at the bottom of every task: a task carries
# its own Workstream, Owner, Standard, Evidence and Acceptance lines, and a red
# finding must carry a Remediation. `up` asserts all of it, so a task that cannot
# name the standard it is judged against fails the build rather than closing.
#
# Everything it creates carries an "(icare-dd)" marker in its name: the project,
# tasks, task stages, tags, milestones and the counterparty contacts. `down` finds
# records by that marker only, so it can never touch Odoo's own records or the
# (scenario) dataset next to it. Safe to run twice.
#
# Credentials come from the environment or a .env, as everywhere else in hodoo.
#
# Dataset
#   project    iCare Capital Holdings - Manager DD (icare-dd), 7 stages, 79 tasks
#   workstreams 15 x 5 phases: rfi, evidence, testing, finding, monitoring
#   gates       kickoff, RFI pack, IC pack (blocked by all 15 findings), monitoring
#   severity    urgency maps to a tag: urgent=red, high=amber, else green
#   5 reds, each with a remediation; 20 tags; 7 milestones; 3 overdue; 1
#   changes-requested finding; the IC gate reads Waiting once its blockers are
#   written, which is the Odoo quirk the startup scenario also pins.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HODOO="${HODOO_BIN:-$ROOT/hodoo/target/debug/hodoo}"
MARKER="(icare-dd)"

if [ ! -x "$HODOO" ]; then
  echo "no hodoo binary at $HODOO: run 'cargo build' in $ROOT/hodoo first" >&2
  exit 2
fi
cd "$ROOT"
# A script reads the CLI, so it asks for JSON once; `show` unsets it on purpose.
export HODOO_OUTPUT=json

# --- the dataset, as data --------------------------------------------------
#
# One row per task: workstream|phase|priority|label|standard|evidence|acceptance|remediation
#
# Remediation is required on a red row (priority urgent) and is the only field a
# reviewer can hold the manager to, so it is data here, not prose in a report.
# Rows are grouped by workstream and ordered by phase, which is what builds the
# parent/child and dependency chain below.

read -r -d '' ITEMS <<'EOF' || true
strategy-governance|rfi|medium|Governance pack requested|ILPA DDQ v2.0 s.1; ILPA Principles 3.0 s.II.A|IC charter, delegation matrix, organisation chart, 12 months of board and IC minutes|Charter states authority limits, quorum and conflict recusal|
strategy-governance|evidence|medium|Charter and delegation read against the register|ILPA DDQ v2.0 s.1; AIFMD II Art 20|Signed minutes index, job descriptions, remuneration policy|Named senior managers match the public register and their job descriptions|
strategy-governance|testing|high|IC decision trail re-performed|FCA SYSC 2 and 4; AIFMD II Art 20(1)|Interview the CEO and CCO; recompute IC timestamps against the minutes|Every decision in the 20-item sample traces to a minuted IC approval|
strategy-governance|finding|high|Governance finding graded|ILPA DDQ v2.0 s.1; SFDR Art 4|Draft finding with the control gap and its consequence|Grade recorded with a remediation owner and a date|
strategy-governance|monitoring|low|Quarterly governance attestation|ILPA Principles 3.0 s.II.C; AIFMD II Art 22|Signed quarterly attestation from the CCO|Recurring item live with an owner and a cadence|
investment-process|rfi|medium|Process documentation requested|ILPA DDQ v2.0 s.2; AIFMD II Art 21|Investment policy, origination funnel, IC memo template, exit playbook|The documents describe one process, not four|
investment-process|evidence|medium|Deal sample assembled|ILPA DDQ v2.0 s.2; ILPA Principles 3.0 s.II.B|Ten deal files with memos, models, approvals and post-close monitoring|Sample spans at least two vintages and one exited position|
investment-process|testing|high|Process followed in the sample|FCA SYSC; AIFMD II Art 21|Re-perform three approvals and compare the memo to the actual terms|Deviations from the stated process are recorded and explained|
investment-process|finding|high|Process finding graded|ILPA DDQ v2.0 s.2|Draft finding on delegation and stage-gate discipline|Grade recorded with an owner and a date|
investment-process|monitoring|low|Deal-file completeness spot check|ILPA DDQ v2.0 s.2|Quarterly spot check of two new deals|Recurring item live|
performance-track-record|rfi|high|Track record requested|ILPA DDQ v2.0 s.3; GIPS where claimed|Fund and deal returns, gross and net, since inception|Figures agree to audited accounts or administrator statements|
performance-track-record|evidence|high|Return and mark history assembled|IPEV Guidelines; IFRS 13|NAV history, unrealised mark schedule, realised proceeds|Every unrealised mark has a valuation memo behind it|
performance-track-record|testing|high|Marks tested against realisation|IPEV Guidelines; ASC 820; ILPA DDQ v2.0 s.3|Compare exited marks to realised proceeds and recompute IRR and MOIC|Median realised-to-mark variance quantified and explained|
performance-track-record|finding|urgent|Track record finding graded|ILPA DDQ v2.0 s.3; GIPS; IPEV Guidelines|Draft finding on persistence, concentration and mark-to-realisation variance|Grade recorded with an owner, a date and a condition|Remediation: re-cut the track record gross and net to audited NAV and disclose the mark-to-realisation variance in the IC pack
performance-track-record|monitoring|medium|Quarterly mark-to-realisation report|IPEV Guidelines|Quarterly variance report by vintage|Recurring item live with an owner|
fund-terms-conflicts|rfi|high|Fund documents requested|ILPA Principles 3.0 s.III; ILPA template LPA|Executed LPA, side letters, PPM, fee and expense policy, co-invest policy|Documents are the executed versions and every side letter is listed|
fund-terms-conflicts|evidence|high|Terms extracted into the comparison grid|ILPA Principles 3.0; AIFMD II Art 9 and 12|Term grid against the ILPA template: fees, offsets, carry, hurdle, clawback|Every deviation from the ILPA template is flagged|
fund-terms-conflicts|testing|high|Conflicts and allocation tested|Advisers Act s.206; AIFMD II Art 12; ILPA s.III.C|Conflict register, allocation policy, cross-fund trade log|The allocation policy is tested against three real allocation decisions|
fund-terms-conflicts|finding|urgent|Terms and conflicts finding graded|ILPA Principles 3.0 s.III; Advisers Act s.206|Draft finding on fee offsets, MFN coverage and allocation conflicts|Grade recorded with the side-letter asks that follow|Remediation: table the fee-offset, MFN and allocation asks as conditions precedent and put them in the side letter
fund-terms-conflicts|monitoring|medium|Annual terms compliance attestation|ILPA Principles 3.0 s.III.D|Annual attestation against the term grid|Recurring item live|
legal-licensing|rfi|high|Licences and filings requested|AIFMD II Art 6; FCA SYSC 3 and 4; Advisers Act s.203|Authorisation certificates, permission scope, Form ADV, passport notifications|Every marketing jurisdiction has a stated legal basis|
legal-licensing|evidence|high|Register evidence obtained|FCA FS Register; DFSA and ADGM registers; FSC, CMA and FSCA lists|Register printouts dated within 30 days, per entity|Entity names on the register match the contracting entities|
legal-licensing|testing|high|Register check performed independently|AIFMD II Art 6; Advisers Act s.203|Independent look-up of each entity on the primary register|Every claimed authorisation is found, with the permitted activity verified|
legal-licensing|finding|high|Licensing finding graded|FCA SYSC 3 and 4; DFSA and ADGM rules|Draft finding on scope, passporting and cross-border marketing|Grade recorded with an owner and a date|
legal-licensing|monitoring|medium|Quarterly register re-check|FCA SYSC 4; ESMA implementing measures|Quarterly register printouts and a change log|Recurring item live|
aml-sanctions|rfi|high|AML programme requested|FATF Recommendations; MLR 2017; BSA and FinCEN|AML policy, business risk assessment, KYC files, screening vendor, SAR log|Programme covers investor, counterparty and portfolio-company onboarding|
aml-sanctions|evidence|high|Programme documents indexed|AMLD package; MLR 2017; OFAC, OFSI, EU and UN lists|Policy versions, training records, MLRO reports, screening alerts|Screening lists are current and the vendor is named|
aml-sanctions|testing|high|Investor files tested|FATF Rec. 10 and 12; MLR 2017 reg.28|Sample ten investor files for source of funds, PEP and sanctions screening|Every gap in the sample is written up with its date and owner|
aml-sanctions|finding|urgent|AML finding graded|FATF Recommendations; MLR 2017; Advisers Act s.206(4)|Draft finding on screening coverage and file completeness|Grade recorded with a remediation and a date|Remediation: close the missing source-of-funds files and re-screen the full investor register before the next close
aml-sanctions|monitoring|medium|Quarterly screening and alert review|FATF Rec. 20; MLR 2017|Quarterly alert and disposal log|Recurring item live|
valuation-audit|rfi|high|Valuation and audit pack requested|IPEV Guidelines; ISA 402; AIFMD II Art 36|Valuation policy, audit opinions, administrator NAV packs, committee minutes|The policy names the method hierarchy and the challenge process|
valuation-audit|evidence|high|NAV evidence assembled|IPEV Guidelines; IFRS 13; ASC 820|NAV statements, mark schedules, administrator reconciliation|NAV ties to the administrator's statement without adjustment|
valuation-audit|testing|high|Policy applied to three positions|IPEV Guidelines; ISA 402|Recompute three marks from source data and read the committee minutes|Recomputation matches within the policy's stated tolerance|
valuation-audit|finding|urgent|Valuation finding graded|IPEV Guidelines; ISA 402; IFRS 13|Draft finding on method consistency and committee independence|Grade recorded with a remediation and a date|Remediation: re-run the three challenged marks with an independent valuer and record the committee's challenge in the minutes
valuation-audit|monitoring|medium|Half-yearly mark review|IPEV Guidelines|Half-yearly mark review with the administrator|Recurring item live|
operations-outsourcing|rfi|medium|Operating model requested|AIFMD II Art 30 and 31; ILPA DDQ v2.0 s.6|Operating model, delegation map, administrator and depositary contracts|Every material function has a named owner and a named delegate|
operations-outsourcing|evidence|medium|Service and oversight evidence indexed|AIFMD II Art 30; ISA 402|SOC 1 or SOC 2 or ISAE 3402 reports, SLAs, delegation notifications|Reports cover the period and the relevant control objectives|
operations-outsourcing|testing|high|Reconciliations re-performed|AIFMD II Art 30; ILPA DDQ v2.0 s.6|Cash and position reconciliation between manager, custodian and administrator|Breaks are identified, aged and explained|
operations-outsourcing|finding|high|Operations finding graded|AIFMD II Art 31; ISA 402|Draft finding on delegation oversight and break ageing|Grade recorded with an owner and a date|
operations-outsourcing|monitoring|low|Monthly break-ageing review|AIFMD II Art 30|Monthly reconciliation break report|Recurring item live|
tech-cyber-dora|rfi|high|ICT and security pack requested|DORA; FCA SYSC 8; Reg S-P|ICT risk framework, asset and vendor register, incident log, response plan|The vendor register covers critical ICT third parties|
tech-cyber-dora|evidence|high|Control reports indexed|ISO 27001; SOC 2 Type II; NIST CSF|Certification, SOC 2 report, penetration test summary, BCP and DR test results|Certifications are current and scoped to the manager's own environment|
tech-cyber-dora|testing|high|Control testing performed|DORA; NIST CSF; ISO 27001 A.5|Test access reviews, a backup restore and an incident walkthrough|Every critical control has evidence from the last 12 months|
tech-cyber-dora|finding|urgent|Cyber and resilience finding graded|DORA; Reg S-P; FCA SYSC 8|Draft finding on coverage gaps, vendor concentration and incident reporting|Grade recorded with a remediation and a date|Remediation: close the critical-control evidence gaps, add vendor concentration reporting and fix the incident reporting clock
tech-cyber-dora|monitoring|high|Quarterly resilience and incident review|DORA; FCA SYSC 8|Quarterly incident and resilience report|Recurring item live|
data-privacy|rfi|medium|Privacy pack requested|GDPR Art 30; UK GDPR; HIPAA and HITECH|Record of processing, DPIA list, retention schedule, DPO appointment|A lawful basis is recorded for every processing activity|
data-privacy|evidence|medium|Processing evidence indexed|GDPR Art 28 and 30; HIPAA and HITECH|Processor contracts, transfer mechanism, sub-processor list|Every processor has an Art 28 contract with the terms that article requires|
data-privacy|testing|high|Transfers and data subject rights tested|GDPR Art 44 and 15; UK GDPR|Transfer impact assessment, data subject request log|Transfers have a documented mechanism and requests close inside the legal clock|
data-privacy|finding|high|Privacy finding graded|GDPR Art 28 to 44; HIPAA|Draft finding on lawful basis, retention and healthcare data exposure|Grade recorded with an owner and a date|
data-privacy|monitoring|low|Annual privacy review|GDPR Art 24; UK GDPR|Annual review of the processing record and the DPIAs|Recurring item live|
esg-sfdr|rfi|medium|ESG pack requested|SFDR Art 6, 8 and 9; ILPA ESG DDQ|ESG policy, fund classification, PAI statement, exclusion list|The classification is consistent with the marketed documents|
esg-sfdr|evidence|medium|ESG evidence indexed|SFDR Art 10; EU Taxonomy; TCFD and ISSB|Pre-contractual disclosures, website disclosures, data vendor|The disclosures match the classification claimed|
esg-sfdr|testing|high|Claim reconciled to holdings|SFDR Art 8 and 9; FCA ESG sourcebook; Advisers Act s.206(4)-1|Reconcile the fund's stated ESG character with its actual holdings|The holdings support the claim, or the claim is narrowed|
esg-sfdr|finding|high|ESG finding graded|SFDR; FCA ESG sourcebook|Draft finding on classification, data quality and greenwashing exposure|Grade recorded with an owner and a date|
esg-sfdr|monitoring|low|Annual classification review|SFDR Art 6 to 12|Annual review against the marketing material|Recurring item live|
tax-structuring|rfi|medium|Structuring pack requested|FATCA and CRS; BEPS; vehicle law|Structure chart, tax residence opinions, FATCA and CRS registrations|Each vehicle has a stated tax residence and a legal opinion|
tax-structuring|evidence|medium|Tax registrations indexed|FATCA and CRS; Pillar Two|GIIN and CRS registrations, filings, transfer pricing files|Filings are current for every marketing jurisdiction|
tax-structuring|testing|high|Substance and residence tested|BEPS Action 5; Pillar Two; vehicle law|Review board presence, employees and decision-making per vehicle|Substance matches what the structure claims|
tax-structuring|finding|high|Tax finding graded|Pillar Two; BEPS; FATCA and CRS|Draft finding on substance, treaty risk and investor reporting|Grade recorded with an owner and a date|
tax-structuring|monitoring|low|Annual tax filing review|FATCA and CRS; Pillar Two|Annual filing calendar and completion check|Recurring item live|
healthcare-sector|rfi|medium|Sector playbook requested|Reimbursement frameworks; FDA and EMA rules; HIPAA; Stark and AKS|Sector thesis, reimbursement map, regulatory playbook, clinical governance policy|The thesis names the payer mix for each holding|
healthcare-sector|evidence|medium|Sector evidence indexed|FDA and EMA authorisations; HIPAA and HITECH|Authorisation status, payer contracts, quality and safety indicators|Every asset has a regulatory status on file|
healthcare-sector|testing|high|Sector capability tested|Reimbursement and regulatory standards; Stark and AKS|Interview the operating partner and test one asset's reimbursement scenario|The manager can evidence sector judgement beyond the pitch|
healthcare-sector|finding|high|Sector finding graded|HIPAA; Stark and AKS; FDA and EMA rules|Draft finding on capability, concentration and regulatory exposure|Grade recorded with an owner and a date|
healthcare-sector|monitoring|low|Quarterly policy-watch note|Healthcare regulatory standards|Quarterly note on reimbursement and regulatory changes|Recurring item live|
insurance-litigation|rfi|medium|Insurance and claims pack requested|ILPA DDQ v2.0 s.7; Advisers Act s.206|PI, D&O and E&O policies, claims history, litigation schedule|Cover limits and exclusions are stated per entity|
insurance-litigation|evidence|medium|Policies and proceedings indexed|Insurance standards; Advisers Act s.206(4)|Policy schedules, notification records, counsel's litigation schedule|The claims schedule is complete to the date of the request|
insurance-litigation|testing|high|Cover tested against the exposure|Insurance standards; ILPA DDQ v2.0 s.7|Map cover to the exposure of each vehicle and role|Gaps between cover and exposure are written up|
insurance-litigation|finding|high|Cover finding graded|ILPA DDQ v2.0 s.7; Advisers Act s.206|Draft finding on limits, exclusions and regulatory proceedings|Grade recorded with an owner and a date|
insurance-litigation|monitoring|low|Annual cover review|Insurance standards|Annual review at renewal|Recurring item live|
continuity-winddown|rfi|medium|Continuity pack requested|AIFMD II Art 47; BCP standards; ILPA DDQ v2.0 s.6|BCP and DR plans, wind-down plan, key-person map, succession plan|Plans name the trigger, the decision-maker and the timetable|
continuity-winddown|evidence|medium|Test and succession evidence indexed|BCP standards; AIFMD II Art 47|Test results, succession names, key-person insurance|The last continuity test is inside 12 months|
continuity-winddown|testing|high|Wind-down and continuity tested|AIFMD II Art 47; BCP standards|Walk the wind-down plan through with the COO against the real books|The plan is executable without new approvals|
continuity-winddown|finding|high|Continuity finding graded|AIFMD II Art 47; ILPA DDQ v2.0 s.6|Draft finding on key-person concentration and wind-down readiness|Grade recorded with an owner and a date|
continuity-winddown|monitoring|low|Annual continuity test review|BCP standards|Annual test review and rectification list|Recurring item live|
EOF

# The three items whose evidence is late, so the dataset carries a real overdue
# set instead of an invented one: the responder is named in the RFI and silent.
OVERDUE="legal-licensing insurance-litigation continuity-winddown"

# Counts are derived from the table above, never typed: a hand-typed total is how
# an assertion quietly stops testing anything after the data changes.
WS_COUNT="$(printf '%s\n' "$ITEMS" | cut -d'|' -f1 | sort -u | wc -l | tr -d ' ')"
ITEM_COUNT="$(printf '%s\n' "$ITEMS" | grep -c '|')"
PHASE_COUNT="$(printf '%s\n' "$ITEMS" | cut -d'|' -f2 | sort -u | wc -l | tr -d ' ')"
GATES=4
STAGES=7
TASKS_TOTAL=$((ITEM_COUNT + GATES))
# 15 workstreams, 3 severities, condition-precedent, monitoring, and the two tags
# the project itself carries: Odoo 19 keeps one tag model for both.
TAGS_TOTAL=$((WS_COUNT + 5 + 2))

# Counterparties and their seats: the register the spec describes, as data.
read -r -d '' PARTIES <<'EOF' || true
iCare Capital Holdings GP Limited|Our counterparty: the manager entity that signs the LPA
iCare Capital Holdings - Chief Compliance Officer|Licensing, AML-CFT, sanctions and the marketing rule
iCare Capital Holdings - Chief Financial Officer|Valuation, NAV, audit and tax
iCare Capital Holdings - Chief Operating Officer|Operations, outsourcing and reconciliations
iCare Capital Holdings - Head of Investor Relations|Fund terms, side letters and LP reporting
iCare Capital Holdings - Data Protection Officer|Data protection, transfers and health data
iCare Capital Holdings - Head of Technology|ICT risk, cyber and operational resilience
iCare Capital Holdings - Portfolio CFO|Healthcare holdings and reimbursement exposure
iCare Capital Holdings - External Auditor|ISA 402, valuation challenge and the audit opinion
iCare Capital Holdings - Fund Administrator|NAV calculus and the investor register
iCare Capital Holdings - Depositary and Custodian|Safekeeping, oversight and cash monitoring
iCare Capital Holdings - External Counsel|Vehicle law, side letters and regulatory filings
EOF

# --- helpers ---------------------------------------------------------------

step() { printf '\n\033[1;32m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }
die() { printf '\033[1;31mERROR: %s\033[0m\n' "$*" >&2; exit 1; }

# Reads JSON on stdin and prints the value at a path, e.g. `.id` or `[0].name`.
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
# How many of several numbers are greater than zero.
sum() { python3 -c 'import sys; print(sum(int(a) for a in sys.argv[1:]))' "$@"; }
count() { python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'; }
ascsv() { tr -s ' ' ',' | sed 's/^,//;s/,$//' ; }
wordcount() { python3 -c 'import sys; print(len(sys.stdin.read().split()))'; }

expect() { # expect <expected> <actual> <what>
  if [ "$1" != "$2" ]; then
    printf '\033[1;31m  FAIL\033[0m %s: expected %s, got %s\n' "$3" "$1" "$2" >&2
    exit 1
  fi
  printf '  \033[32mok\033[0m   %s = %s\n' "$3" "$2"
}

today() { date -d "$1" +%F; }

# How many tasks match a domain, without reading them.
task_count() { "$HODOO" call project.task search_count --body "{\"domain\":$1}"; }

# How many tasks match a domain plus a python predicate over the rows read.
# Predicates are one-liners over `t`, so the assertions below read as English.
task_where() { # task_where <domain> <python expression over t>
  "$HODOO" call project.task search_read \
    --body "{\"domain\":$1,\"fields\":[\"name\",\"description\",\"stage_id\",\"priority\",\"user_ids\"]}" \
    | python3 -c "import json,sys; print(sum(1 for t in json.load(sys.stdin) if $2))"
}

tag_id() { # tag_id <name>
  "$HODOO" call project.tags search_read \
    --body "{\"domain\":[[\"name\",\"=\",\"$1\"]],\"fields\":[\"id\"],\"limit\":1}" | field '[0].id'
}

stage_id() { # stage_id <name>
  "$HODOO" call project.task.type search_read \
    --body "{\"domain\":[[\"name\",\"=\",\"$1\"]],\"fields\":[\"id\"],\"limit\":1}" | field '[0].id'
}

# Odoo's own project stages, reused rather than recreated.
project_stage() {
  "$HODOO" call project.project.stage search_read \
    --body "{\"domain\":[[\"name\",\"=\",\"$1\"]],\"fields\":[\"id\"],\"limit\":1}" | field '[0].id'
}

# Tasks are assigned to the automation user, whose partner has no email address: a
# dataset that posts comments must not be able to mail a real person.
auth() {
  if ! whoami_json="$("$HODOO" whoami -o json 2>/tmp/hodoo-icare-auth.json)"; then
    die "hodoo could not authenticate: $(head -c 300 /tmp/hodoo-icare-auth.json)"
  fi
  AGENT="$(printf '%s' "$whoami_json" | field .uid)"
}

# The description every task must carry. `workstream` names the lane, the other
# four lines are the ones a reviewer checks before a task may close.
describe() { # describe <workstream> <owner> <standard> <evidence> <acceptance> [remediation]
  printf '<p>Workstream: %s</p><p>Owner: %s</p><p>Standard: %s</p><p>Evidence: %s</p><p>Acceptance: %s</p>' "$1" "$2" "$3" "$4" "$5"
  [ -n "${6:-}" ] && printf '<p>Remediation: %s</p>' "$6"
  return 0
}

# --- teardown --------------------------------------------------------------

down() {
  step "removing icare-dd data"

  local projects stages tags contacts milestones own_stages
  projects="$("$HODOO" call project.project search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  stages="$("$HODOO" call project.task.type search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  tags="$("$HODOO" call project.tags search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  contacts="$("$HODOO" call res.partner search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  milestones="$("$HODOO" call project.milestone search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  own_stages="$("$HODOO" call project.project.stage search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"

  # Tasks and milestones cascade with the project, so deleting it is enough for
  # those; the rest are re-searched afterwards, because a cascade is the only
  # thing that can make a list gathered up front stale.
  if [ -n "$projects" ]; then
    note "projects:       $projects"
    "$HODOO" call project.project unlink --body "{\"ids\":[$projects]}" > /dev/null
  fi
  milestones="$("$HODOO" call project.milestone search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  if [ -n "$milestones" ]; then
    note "milestones:     $milestones"
    "$HODOO" call project.milestone unlink --body "{\"ids\":[$milestones]}" > /dev/null
  fi
  stages="$("$HODOO" call project.task.type search --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}" | csv)"
  if [ -n "$stages" ]; then
    note "task stages:    $stages"
    "$HODOO" call project.task.type unlink --body "{\"ids\":[$stages]}" > /dev/null
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

  expect 0 "$("$HODOO" call project.project search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd projects left behind"
  expect 0 "$("$HODOO" call project.task search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd tasks left behind"
  expect 0 "$("$HODOO" call project.task.type search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd task stages left behind"
  expect 0 "$("$HODOO" call project.milestone search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd milestones left behind"
  expect 0 "$("$HODOO" call project.tags search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd tags left behind"
  expect 0 "$("$HODOO" call res.partner search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd contacts left behind"
}

# --- the dataset -----------------------------------------------------------

up() {
  auth
  note "acting as uid $AGENT on Odoo $("$HODOO" version | field .version)"

  # Rebuilding rather than duplicating: the names are stable, so a second `up`
  # would collide with the first. The residue check counts stages and tags as
  # well, because a teardown interrupted between two unlinks leaves those behind
  # with no project to point at.
  local residue
  residue=$(( $("$HODOO" call project.project search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")
           + $("$HODOO" call project.task.type search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")
           + $("$HODOO" call project.tags search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")
           + $("$HODOO" call project.milestone search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}") ))
  if [ "$residue" != "0" ]; then
    note "an earlier run is present: removing it first"
    down
  fi

  # -- the counterparty ------------------------------------------------------
  # Creating contacts needs Contacts > Creation (base.group_partner_manager). A
  # project administrator does not have it by default, so the register degrades
  # to names in task descriptions rather than failing the build.
  step "the counterparty and its seats"
  local parties="" have_contacts=1
  local -A PARTY_ID=()
  local name why reply id
  while IFS='|' read -r name why; do
    [ -n "$name" ] || continue
    if ! reply="$("$HODOO" call res.partner create --body "$(python3 -c '
import json, sys
print(json.dumps({"vals_list": [{"name": sys.argv[1], "is_company": False, "comment": sys.argv[2]}]}))
' "$name $MARKER" "$why")" 2>&1)"; then
      if [ "$have_contacts" = 1 ]; then
        have_contacts=0
        note "this user may not create contacts: $(printf '%s' "$reply" | head -c 160)"
        note "granting Contacts > Creation (base.group_partner_manager) completes the register"
      fi
      break
    fi
    id="$(printf '%s' "$reply" | field '[0]')"
    PARTY_ID["$name"]="$id"
    parties="$parties $id"
  done < <(printf '%s\n' "$PARTIES")
  if [ "$have_contacts" = 1 ]; then
    note "$(printf '%s' "$parties" | wordcount) counterparty records created"
  else
    note "the register lives in the task descriptions instead"
  fi
  local customer_args=()
  if [ "$have_contacts" = 1 ]; then
    customer_args=(--customer "${PARTY_ID['iCare Capital Holdings GP Limited']}")
  fi

  # -- tags -----------------------------------------------------------------
  # Tags are created on the spot by `--tag`, so there is nothing to create here:
  # only the tally at the end proves they arrived.
  step "tags"
  note "15 workstream tags, 3 severity tags, condition-precedent and monitoring"
  local -A SEVERITY=([urgent]="red $MARKER" [high]="amber $MARKER" [medium]="green $MARKER" [low]="green $MARKER")

  # -- the project ----------------------------------------------------------
  step "the project"
  local p_dd
  p_dd="$("$HODOO" project create \
    --name "iCare Capital Holdings - Manager DD $MARKER" \
    "${customer_args[@]}" --manager "$AGENT" \
    --stage "$(project_stage 'In Progress')" \
    --visibility employees \
    --start "$(today 'today')" --end "$(today '+95 days')" \
    --milestones --dependencies \
    --tag "due-diligence $MARKER" --tag "fund-manager $MARKER" \
    --description '<p>Manager-level due diligence of iCare Capital Holdings: 15 workstreams, five phases, four gates. Findings cite a standard; reds carry a remediation.</p>' \
    | field .id)"
  expect 1 "$("$HODOO" call project.project search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd projects"
  expect "employees" "$("$HODOO" project show "$p_dd" | field .privacy_visibility)" "project visibility"
  local flags
  flags="$("$HODOO" call project.project read --ids "$p_dd" \
    --body '{"fields":["allow_milestones","allow_task_dependencies"]}')"
  expect "true" "$(printf '%s' "$flags" | field '[0].allow_milestones')" "milestones are allowed"
  expect "true" "$(printf '%s' "$flags" | field '[0].allow_task_dependencies')" "dependencies are allowed"
  if [ "$have_contacts" = 1 ]; then
    expect "${PARTY_ID['iCare Capital Holdings GP Limited']}" \
      "$("$HODOO" project show "$p_dd" | field .partner_id)" "the manager is the project's customer"
  fi

  # -- kanban columns -------------------------------------------------------
  step "the five phases, as kanban columns"
  expect "[]" "$("$HODOO" project show "$p_dd" | field .type_ids)" \
    "a fresh project starts with no task stages"
  local -A S=()
  S[intake]="$("$HODOO" project task-stages create --name "Intake $MARKER" --project "$p_dd" --sequence 10 | field .id)"
  S[rfi]="$("$HODOO" project task-stages create --name "RFI Issued $MARKER" --project "$p_dd" --sequence 20 | field .id)"
  S[evidence]="$("$HODOO" project task-stages create --name "Evidence Received $MARKER" --project "$p_dd" --sequence 30 | field .id)"
  S[testing]="$("$HODOO" project task-stages create --name "Testing $MARKER" --project "$p_dd" --sequence 40 | field .id)"
  S[findings]="$("$HODOO" project task-stages create --name "Findings $MARKER" --project "$p_dd" --sequence 50 | field .id)"
  S[ic]="$("$HODOO" project task-stages create --name "IC Review $MARKER" --project "$p_dd" --sequence 60 | field .id)"
  S[closed]="$("$HODOO" project task-stages create --name "Closed $MARKER" --project "$p_dd" --sequence 70 --fold | field .id)"
  expect "$STAGES" "$("$HODOO" call project.task.type search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd task stages"
  expect "$STAGES" "$("$HODOO" project task-stages ls "$p_dd" | count)" "stages attached to the project"

  # -- the gates ------------------------------------------------------------
  step "the four phase gates"
  local -A M=()
  M[01]="$("$HODOO" milestone create --project "$p_dd" --name "DD-01 Kickoff $MARKER" --due "$(today '+2 days')" | field .id)"
  M[02]="$("$HODOO" milestone create --project "$p_dd" --name "DD-02 RFI issued $MARKER" --due "$(today '+7 days')" | field .id)"
  M[03]="$("$HODOO" milestone create --project "$p_dd" --name "DD-03 Evidence complete $MARKER" --due "$(today '+28 days')" | field .id)"
  M[04]="$("$HODOO" milestone create --project "$p_dd" --name "DD-04 Testing complete $MARKER" --due "$(today '+45 days')" | field .id)"
  M[05]="$("$HODOO" milestone create --project "$p_dd" --name "DD-05 Findings drafted $MARKER" --due "$(today '+60 days')" | field .id)"
  M[06]="$("$HODOO" milestone create --project "$p_dd" --name "DD-06 IC decision $MARKER" --due "$(today '+75 days')" | field .id)"
  M[07]="$("$HODOO" milestone create --project "$p_dd" --name "DD-07 Monitoring live $MARKER" --due "$(today '+90 days')" | field .id)"
  expect 7 "$("$HODOO" call project.milestone search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd milestones"

  # One milestone per phase, so a milestone is a gate the board can be read against.
  local -A PHASE_STAGE=([rfi]=rfi [evidence]=evidence [testing]=testing [finding]=findings [monitoring]=ic)
  local -A PHASE_MILESTONE=([rfi]=02 [evidence]=03 [testing]=04 [finding]=05 [monitoring]=07)
  local -A PHASE_DUE=([rfi]="+7 days" [evidence]="+28 days" [testing]="+45 days" [finding]="+60 days" [monitoring]="+90 days")

  local g_kickoff g_rfi g_ic g_monitor
  g_kickoff="$("$HODOO" task create \
    --name "Kickoff: mandate, scope and materiality $MARKER" \
    --project "$p_dd" --stage "${S[intake]}" --priority low --assignee "$AGENT" \
    --due "$(today '+2 days')" --milestone "${M[01]}" \
    --tag "green $MARKER" --tag "condition-precedent $MARKER" \
    --description "$(describe phase-gate "Our DD lead and our IC" \
      "ILPA DDQ v2.0 scope; ILPA Principles 3.0" \
      "signed mandate, scope memo, materiality thresholds" \
      "the scope memo names the depth per workstream and the IC that receives it")" | field .id)"
  g_rfi="$("$HODOO" task create \
    --name "Issue the RFI pack: ILPA DDQ plus regime schedules $MARKER" \
    --project "$p_dd" --stage "${S[rfi]}" --priority high --assignee "$AGENT" \
    --due "$(today '+7 days')" --milestone "${M[02]}" --depends-on "$g_kickoff" \
    --tag "amber $MARKER" --tag "condition-precedent $MARKER" \
    --description "$(describe phase-gate "Our DD lead" \
      "ILPA DDQ v2.0; ILPA Principles 3.0; AIFMD II; Advisers Act s.206; FATF Recommendations" \
      "the RFI pack as sent, its index, and the chase calendar" \
      "every workstream item has a named responder and a date to answer by")" | field .id)"
  g_ic="$("$HODOO" task create \
    --name "IC pack: findings, conditions precedent and side-letter asks $MARKER" \
    --project "$p_dd" --stage "${S[ic]}" --priority high --assignee "$AGENT" \
    --due "$(today '+75 days')" --milestone "${M[06]}" \
    --tag "amber $MARKER" --tag "condition-precedent $MARKER" \
    --description "$(describe phase-gate "Our DD lead and our IC" \
      "ILPA Principles 3.0 s.III; Advisers Act s.206; AIFMD II Art 20" \
      "the IC pack, the red-flag schedule and the list of conditions precedent" \
      "every red has a condition precedent or a written IC waiver")" | field .id)"
  g_monitor="$("$HODOO" task create \
    --name "Monitoring plan: quarterly items, owners and cadence $MARKER" \
    --project "$p_dd" --stage "${S[closed]}" --priority low --assignee "$AGENT" \
    --due "$(today '+90 days')" --milestone "${M[07]}" --depends-on "$g_ic" \
    --tag "monitoring $MARKER" --tag "green $MARKER" \
    --description "$(describe phase-gate "Our DD lead" \
      "ILPA Principles 3.0 s.II.C; AIFMD II Art 22" \
      "the monitoring schedule handed to the operating team" \
      "every open condition has a recurring item, an owner and a cadence")" | field .id)"

  # -- the workstreams ------------------------------------------------------
  step "15 workstreams x 5 phases"
  local -A PARENT=() LAST=() ITEMS_OF_WS=() ID=()
  local findings_ids="" red_count=0 ws phase priority label standard evidence acceptance remediation
  while IFS='|' read -r ws phase priority label standard evidence acceptance remediation; do
    [ -n "$ws" ] || continue
    local owner="Our workstream lead: $ws"
    local args=(--name "$label $MARKER" --project "$p_dd"
      --stage "${S[${PHASE_STAGE[$phase]}]}" --priority "$priority" --assignee "$AGENT"
      --milestone "${M[${PHASE_MILESTONE[$phase]}]}"
      --tag "$ws $MARKER" --tag "${SEVERITY[$priority]}")
    # A monitoring item outlives the diligence, so it is tagged to be findable
    # after the project closes.
    if [ "$phase" = monitoring ]; then
      args+=(--tag "monitoring $MARKER")
    fi
    local due
    due="$(today "${PHASE_DUE[$phase]}")"
    case "$phase" in
      rfi) ;; # the RFI item is the workstream's parent: it goes out first
      *) args+=(--parent "${PARENT[$ws]}") ;;
    esac
    # The three late responders: their evidence is past due, which is what an
    # overdue item in a live diligence actually looks like.
    local late=""
    case " $OVERDUE " in *" $ws "*) [ "$phase" = evidence ] && late=1 ;; esac
    if [ -n "$late" ]; then
      due="$(today '-3 days')"
    fi
    args+=(--due "$due")
    if [ "$phase" != rfi ]; then
      args+=(--depends-on "${LAST[$ws]}")
    fi
    if [ -n "$remediation" ]; then
      args+=(--description "$(describe "$ws" "$owner" "$standard" "$evidence" "$acceptance" "$remediation")")
    else
      args+=(--description "$(describe "$ws" "$owner" "$standard" "$evidence" "$acceptance")")
    fi
    # The first phase of a workstream waits on the RFI going out; the rest chain
    # inside the workstream.
    if [ "$phase" = rfi ]; then
      args+=(--depends-on "$g_rfi")
    fi
    local task
    task="$("$HODOO" task create "${args[@]}" | field .id)"
    LAST[$ws]="$task"
    ID["$ws:$phase"]="$task"
    if [ "$phase" = rfi ]; then
      PARENT[$ws]="$task"
    fi
    if [ "$phase" = finding ]; then
      findings_ids="$findings_ids $task"
    fi
    if [ "$priority" = urgent ]; then
      red_count=$((red_count + 1))
    fi
    ITEMS_OF_WS[$ws]="$((${ITEMS_OF_WS[$ws]:-0} + 1))"
  done < <(printf '%s\n' "$ITEMS")

  expect "$WS_COUNT" "${#ITEMS_OF_WS[@]}" "workstreams in the dataset"
  expect "$PHASE_COUNT" "$(printf '%s\n' "${ITEMS_OF_WS[@]}" | sort -u | tr '\n' ' ' | sed 's/ *$//')" "phases per workstream"
  expect 5 "$red_count" "red items, one per high-risk workstream"

  # -- the dependency that makes the process real ---------------------------
  step "the IC gate waits on every workstream's finding"
  local dep_list
  dep_list="$(printf '%s' "$findings_ids" | ascsv)"
  local dep_args=()
  local one
  for one in $findings_ids; do dep_args+=(--depends-on "$one"); done
  "$HODOO" task update "$g_ic" "${dep_args[@]}" > /dev/null
  expect "$WS_COUNT" "$("$HODOO" task deps "$g_ic" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["blocked_by"]))')" "the IC gate is blocked by one task per workstream"
  expect "$WS_COUNT" "$("$HODOO" call project.task search_count \
    --body "{\"domain\":[[\"id\",\"in\",[$dep_list]],[\"stage_id\",\"=\",${S[findings]}]]}")" \
    "of which sit in the Findings column"

  # Odoo does not compute "waiting" while a task is being created: writing the
  # dependencies again is what turns a blocked task into a waiting one. The
  # startup scenario pins the same behaviour; this one relies on it.
  expect "04_waiting_normal" "$("$HODOO" task show "$g_ic" | field .state)" \
    "the IC gate reads waiting once its blockers are written"

  # -- movement, so the board is not a wall of new tasks --------------------
  step "progress, a scope change and a red flag"
  "$HODOO" task done "$g_kickoff" > /dev/null
  "$HODOO" task done "${ID[strategy-governance:testing]}" > /dev/null
  # Closing the kickoff is what releases the RFI pack, which is the only chain
  # in the dataset that starts closed.
  expect "01_in_progress" "$("$HODOO" task show "$g_rfi" | field .state)" \
    "the RFI pack is released by the kickoff closing"
  # The manager asked for the request to be re-cut before answering, so the item
  # goes back with a change requested rather than sitting in silence.
  "$HODOO" task update "$g_rfi" --state changes-requested > /dev/null
  expect "02_changes_requested" "$("$HODOO" task show "$g_rfi" | field .state)" \
    "the RFI pack is changes-requested"

  # A task with open blockers reads "waiting" whatever else is true of it: the
  # privacy finding is refused by the manager (a change requested, in any human
  # reading) and Odoo still shows it as waiting on its own open testing item.
  local privacy_finding
  privacy_finding="${ID[data-privacy:finding]}"
  expect "04_waiting_normal" "$("$HODOO" task show "$privacy_finding" | field .state)" \
    "a blocked task reads waiting, not changes-requested"

  local aml_finding
  aml_finding="$("$HODOO" call project.task search_read \
    --body "{\"domain\":[[\"name\",\"like\",\"AML finding graded $MARKER\"]],\"fields\":[\"id\"],\"limit\":1}" | field '[0].id')"
  "$HODOO" task comment "$aml_finding" \
    --body "Screening covers investors but not portfolio-company counterparties; the register will be re-run after the next close. Escalated to the IC as a condition precedent." --internal > /dev/null
  "$HODOO" project comment "$p_dd" --body "RFI issued to the CCO and CFO; three workstreams already past their evidence date." --internal > /dev/null
  expect "true" "$("$HODOO" task messages "$aml_finding" \
    | python3 -c 'import json,sys; print(str(any("condition precedent" in (m["body"] or "") for m in json.load(sys.stdin))).lower())')" \
    "the escalation is on the finding's thread"

  # -- what the dataset must be true about ----------------------------------
  step "the rules this dataset has to satisfy"

  local all='[["name","like","'"$MARKER"'"]]'
  expect "$TASKS_TOTAL" "$(task_count "$all")" "tasks in the dataset"
  expect "$((ITEM_COUNT - WS_COUNT))" "$("$HODOO" call project.task search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"],[\"parent_id\",\"!=\",false]]}")" \
    "of which hang off a workstream parent"
  expect $((WS_COUNT + GATES)) "$("$HODOO" call project.task search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"],[\"parent_id\",\"=\",false]]}")" \
    "roots: one lead per workstream and the gates"

  # The rule the whole exercise rests on: no task without a standard, an owner,
  # the evidence that closes it, and the test that says it passed.
  local missing_lines
  missing_lines="$(task_where "$all" 'any(k not in (t["description"] or "") for k in ["Workstream:", "Owner:", "Standard:", "Evidence:", "Acceptance:"])')"
  expect 0 "$missing_lines" "tasks missing an owner, standard, evidence or acceptance line"

  # ...and no red without a remedy.
  local reds red_without_remedy
  reds="$(task_where "$all" 't["priority"] == "3"')"
  expect 5 "$reds" "red items"
  red_without_remedy="$(task_where "$all" 't["priority"] == "3" and "Remediation:" not in (t["description"] or "")')"
  expect 0 "$red_without_remedy" "red items with no remediation"
  local amber
  amber="$(task_where "$all" 't["priority"] == "2"')"
  note "amber: $amber, the rest green"

  # Every workstream carries five items, and every item names its workstream.
  local workstreams_in_data
  workstreams_in_data="$(printf '%s\n' "$ITEMS" | cut -d'|' -f1 | sort -u | wc -l | tr -d ' ')"
  expect "$WS_COUNT" "$workstreams_in_data" "workstreams in the data table"
  local ws_tasks
  ws_tasks="$("$HODOO" call project.task search_count \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"],[\"tag_ids\",\"!=\",false]]}")"
  note "tasks carrying a tag: $ws_tasks"

  # Tags: 15 workstreams, 3 severities, condition-precedent and monitoring.
  expect "$TAGS_TOTAL" "$("$HODOO" call project.tags search_count --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]]}")" "icare-dd tags"
  expect 5 "$("$HODOO" task ls "$MARKER" --tag "red $MARKER" --limit 0 | count)" "tasks carrying the red tag"
  expect "$((WS_COUNT + 1))" "$("$HODOO" task ls "$MARKER" --tag "monitoring $MARKER" --limit 0 | count)" \
    "monitoring items, counting the monitoring gate"

  # The board, which is what anyone actually opens on a Monday morning.
  local per_stage total_check=0
  local stage
  for stage in intake rfi evidence testing findings ic closed; do
    per_stage="$("$HODOO" call project.task search_count \
      --body "{\"domain\":[[\"project_id\",\"=\",$p_dd],[\"stage_id\",\"=\",${S[$stage]}]]}")"
    note "  $(printf '%-18s' "$stage") $per_stage"
    total_check=$((total_check + per_stage))
  done
  expect "$TASKS_TOTAL" "$total_check" "tasks across the seven columns"
  expect "$((WS_COUNT + 1))" "$("$HODOO" call project.task search_count \
    --body "{\"domain\":[[\"project_id\",\"=\",$p_dd],[\"stage_id\",\"=\",${S[rfi]}],[\"is_closed\",\"=\",false]]}")" \
    "items still waiting on the RFI"
  # `board` answers differently per mode: a table groups by column (a line each,
  # plus a header), while its JSON is one object holding every open task. The
  # script runs in JSON mode, so it counts the tasks.
  expect "$((TASKS_TOTAL - 2))" "$("$HODOO" board "iCare Capital Holdings - Manager DD $MARKER" \
    | python3 -c 'import json,sys; data=json.load(sys.stdin); print(sum(len(g["tasks"]) for g in data))')" \
    "open tasks the board lists"

  # Odoo assigns the calling user to whatever it creates; nothing may be
  # assigned to anyone else, because nobody else exists.
  expect "$AGENT" "$("$HODOO" call project.task search_read \
    --body "{\"domain\":[[\"name\",\"like\",\"$MARKER\"]],\"fields\":[\"user_ids\"],\"limit\":0}" \
    | python3 -c '
import json, sys
owners = sorted({u for t in json.load(sys.stdin) for u in t["user_ids"]})
print(" ".join(str(u) for u in owners))
')" "every task is assigned to the acting user"

  local open overdue
  open="$("$HODOO" task ls "$MARKER" --open --limit 0 | count)"
  expect "$((TASKS_TOTAL - 2))" "$open" "open tasks (two are closed: the kickoff and one governance item)"
  overdue="$("$HODOO" task ls "$MARKER" --open --overdue --limit 0 | count)"
  expect 3 "$overdue" "overdue items, one per silent responder"
  note "overdue: $("$HODOO" task ls "$MARKER" --open --overdue --order date_deadline \
    --limit 0 | python3 -c 'import json,sys; print(", ".join(t["name"].replace(" (icare-dd)", "") for t in json.load(sys.stdin)))')"
  expect 3 "$("$HODOO" task ls "$MARKER" --open --due-before "$(today '+7 days')" --limit 0 | count)" \
    "open items due within a week"

  # A field Odoo does not have is the signal that a model changed, so the escape
  # hatch is exercised rather than assumed.
  set +e
  "$HODOO" task show 999999999 > /dev/null 2>/tmp/hodoo-icare-error.json
  local missing_id=$?
  set -e
  expect 1 "$missing_id" "reading a task that is gone fails"
  note "the refusal, verbatim: $(head -c 150 /tmp/hodoo-icare-error.json)"

  step "up is done"
  cat <<EOF
    project     $p_dd  iCare Capital Holdings - Manager DD $MARKER
    columns     ${S[intake]} ${S[rfi]} ${S[evidence]} ${S[testing]} ${S[findings]} ${S[ic]} ${S[closed]}
    gates       $g_kickoff $g_rfi $g_ic $g_monitor
    milestones  7 (DD-01 kickoff through DD-07 monitoring)
    find-me     ./icare-dd.sh show    read-only dashboard
    remove      ./icare-dd.sh down    deletes only $MARKER records
EOF
}

# --- read-only dashboard ---------------------------------------------------

show() {
  # The dashboard is for a person, so it drops the script's JSON mode and lets
  # the CLI's own tables do the talking: nothing here parses output.
  unset HODOO_OUTPUT
  auth

  if [ "$("$HODOO" project ls "$MARKER" --limit 0 --no-headers | wc -l)" = "0" ]; then
    echo "no icare-dd data: run './icare-dd.sh up' first"
    return
  fi

  step "who is asking, and on what server"
  "$HODOO" whoami

  step "the project"
  "$HODOO" project ls "$MARKER" --limit 0
  "$HODOO" milestone ls --project "iCare Capital Holdings - Manager DD $MARKER"

  step "the board"
  "$HODOO" board "$MARKER"

  step "the reds, which is what the IC actually reads"
  "$HODOO" task ls "$MARKER" --tag "red $MARKER" --limit 0

  step "past due, worst first"
  "$HODOO" task ls "$MARKER" --open --overdue --order date_deadline --limit 0

  step "waiting on someone else"
  "$HODOO" task ls "$MARKER" --state waiting --limit 0
  "$HODOO" task ls "$MARKER" --state changes-requested --limit 0

  step "one workstream in full: the terms and conflicts lane"
  "$HODOO" task ls "$MARKER" --tag "fund-terms-conflicts $MARKER" --limit 0

  step "what blocks the IC pack"
  "$HODOO" task deps "IC pack: findings, conditions precedent and side-letter asks $MARKER"

  step "one red finding, in full"
  "$HODOO" task show "AML finding graded $MARKER"

  step "the monitoring items that outlive the diligence"
  "$HODOO" task ls "$MARKER" --tag "monitoring $MARKER" --limit 0
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  show) show ;;
  *)
    cat <<EOF
usage: $(basename "$0") {up|down|show}

  up     build the iCare manager-DD dataset, asserting it as it goes
  down   delete exactly what 'up' created, found by its "$MARKER" marker
  show   read-only dashboard: board, reds, overdue, blockers, monitoring

15 workstreams x 5 phases, 4 phase gates, 7 milestones, 79 tasks. Every task
carries Workstream, Owner, Standard, Evidence and Acceptance lines; a red item
must also carry a Remediation, and 'up' fails if one does not.

Binary:      $HODOO  (override with HODOO_BIN)
Credentials: the environment, then a .env, exactly as the CLI resolves them.
EOF
    exit 2
    ;;
esac
