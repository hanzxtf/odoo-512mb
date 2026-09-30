# iCare Capital Holdings - manager due diligence: design

**What this specifies.** A manager-level due diligence of **iCare Capital Holdings**, a
cross-border healthcare fund manager, run as a live Odoo project over the `hodoo` CLI.
The exercise answers one question: is this manager fit to hold capital, and under what
written conditions?

**Shape.** One Odoo project is the system of record. 15 workstreams, 5 phases, 4 phase
gates, 7 milestones, ~79 tasks. Every task carries its own standard, evidence and
acceptance test in its description, so a task cannot be closed by assertion.

**Regimes.** The manager is treated as regulated in the United States, the United
Kingdom, the European Union and the Middle East/Africa at once, which is what makes the
standards matrix below the document that governs the rest of the exercise.

---

## 1. Standards matrix

Findings cite a standard by name. A finding that cannot name one is an opinion and is
recorded as a question for the IC, not a finding.

| Layer | Instruments that bind or guide the workstream |
|---|---|
| Allocator standard | ILPA Due Diligence Questionnaire v2.0, ILPA Principles 3.0, ILPA template LPA and side letter, ILPA ESG DDQ |
| European Union | AIFMD II (Directive (EU) 2024/927), ESMA implementing measures, UCITS where applicable, SFDR (Art 6/8/9) and EU Taxonomy, DORA (ICT risk), AML Regulation package, GDPR |
| United Kingdom | FCA SYSC and SM&CR (CF1/CF10), COLL, UK onshored AIFMD, MLR 2017, UK GDPR, FCA ESG sourcebook |
| United States | Advisers Act s.206, Rule 206(4)-2 (custody), Rule 206(4)-1 (marketing), Rule 17j-1 (ethics), Form ADV and Form PF, Reg S-P and S-ID, BSA/FinCEN |
| Market entry and local | DFSA and ADGM FSRA, Mauritius FSC, Kenya CMA, Nigeria SEC, South Africa FSCA; the passporting question decides which vehicle can be marketed where |
| Healthcare overlay | Reimbursement vs self-pay mix, FDA/EMA authorisation status, HIPAA and HITECH plus Stark/AKS where US care is delivered, clinical governance and patient-safety exposure |
| Horizontal controls | FATF Recommendations, sanctions programmes (OFAC, EU CFSP, UK OFSI, UN), IPEV Guidelines and IFRS 13/ASC 820, ISA 402 and SOC 2/ISO 27001/NIST CSF, FATCA-CRS, Pillar Two, Cayman/Luxembourg/DIFC vehicle law |

### Workstream to primary standard

| # | Workstream | Primary standard of record |
|---|---|---|
| 1 | Governance and strategy | ILPA DDQ s.1, ILPA Principles 3.0, AIFMD II Art 20 |
| 2 | Investment process and IC discipline | ILPA DDQ s.2, AIFMD II Art 20-21, FCA SYSC |
| 3 | Track record and performance | ILPA DDQ s.3, GIPS where claimed, IPEV for unrealised marks |
| 4 | Fund terms, conflicts and allocation | ILPA Principles 3.0 s.III, AIFMD II Art 9/12, Advisers Act s.206 |
| 5 | Licensing and legal standing | AIFMD II Art 6, FCA SYSC 3-4, Advisers Act s.203, DFSA/ADGM, FSC/CMA/FSCA |
| 6 | AML/CFT, sanctions and KYC | FATF Recommendations, MLR 2017, AMLD package, BSA/FinCEN, OFAC/OFSI/EU/UN |
| 7 | Valuation, audit and fund administration | IPEV Guidelines, IFRS 13/ASC 820, ISA 402, AIFMD II Art 36 |
| 8 | Operations and outsourcing | AIFMD II Art 30-31, ISA 402, ILPA DDQ s.6 |
| 9 | Technology, cyber and resilience | DORA, ISO 27001/SOC 2, NIST CSF, Reg S-P/S-ID, FCA SYSC 8 |
| 10 | Data protection and privacy | GDPR and UK GDPR, HIPAA/HITECH, Swiss FADP, PIPL, Art 28/44 transfers |
| 11 | ESG and SFDR claims | SFDR Art 6/8/9 and Taxonomy, ILPA ESG DDQ, TCFD/ISSB, FCA ESG sourcebook |
| 12 | Tax, structuring and cross-border | FATCA-CRS, Pillar Two, BEPS, vehicle law, transfer pricing |
| 13 | Healthcare sector capability | Reimbursement and regulatory playbook, FDA/EMA, HIPAA and Stark/AKS, clinical governance |
| 14 | Insurance and litigation | PI/D&O/E&O cover, claims history, regulatory actions, s.206 disclosure |
| 15 | Continuity and wind-down | BCP/DR standards, AIFMD II Art 47 wind-down and liquidation, key-person risk |

---

## 2. Process

Five phases, gated on evidence rather than time. Stage names in Odoo mirror them.

1. **Intake** - mandate, scope, materiality, risk-based depth, NDA and confidentiality.
2. **RFI issued** - the ILPA DDQ plus regime-specific schedules go out with an owner per
   item; the request itself is a task, so silence has a date.
3. **Evidence received** - documents are indexed, dated and versioned. An assertion that
   arrives without a document stays in this phase.
4. **Testing** - independent verification: re-perform the sample, check the primary
   register, reference calls, site visit, penetration and control reports.
5. **Findings** - each gap is graded, given a named standard, a consequence, a
   remediation, an owner and a date.
6. **IC review** - the pack goes to the IC with conditions precedent and side-letter asks.
7. **Closed** - monitoring items are created before the project closes, not after.

### Rating model

Severity is derived from the task's priority, so there is one source of truth:

| Priority | Severity tag | Meaning |
|---|---|---|
| urgent | `red (icare-dd)` | material; deal-breaker or condition precedent |
| high | `amber (icare-dd)` | must be fixed before close, or tracked with a date |
| medium / low | `green (icare-dd)` | noted, no condition |

**Rule enforced by the script:** a `red` task whose description carries no
`Remediation:` line fails the self-check. A red finding without a remedy is an opinion
with a colour.

### Evidence rule

Every task description carries four lines, and the script asserts all four are present:

```
Owner:      the accountable seat
Standard:   the instrument the item is judged against
Evidence:   the document or test that closes it
Acceptance: the test that tells a reviewer it passed
Remediation: required only for red items
```

---

## 3. Operations

- **Cadence**: weekly chase report (open tasks in `RFI issued` past due), fortnightly
  workstream review, monthly IC update, and a written escalation for any red that moves
  past its date.
- **Registers that live as tasks, not spreadsheets**: evidence register (one task per
  item, phase = evidence), chase log (due dates and chatter), escalation log (reds),
  conflict register (workstream 4), outsider register (workstream 8), ICT register
  (workstream 9).
- **Audit trail**: `task comment --internal` for anything sensitive; the chatter is the
  record of who was asked, when, and what they said.
- **After close**: the 15 monitoring tasks stay open with a `monitoring (icare-dd)` tag
  and an owner, which is what turns a one-off DD into supervision.

---

## 4. Stakeholder register

Counterparties are `res.partner` records (Odoo's only way to name a party without
creating users); owners are named in each task's description as a seat, because the
server has two user accounts and inventing seats as users would be a lie.

| Side | Seat | Accountable for |
|---|---|---|
| Counterparty | GP / IC members | governance, IC decisions, conflicts |
| Counterparty | CEO | strategy, track record narrative |
| Counterparty | CCO / MLRO | licensing, AML-CFT, sanctions, marketing rule |
| Counterparty | CFO | valuation, NAV, audit, tax |
| Counterparty | COO | operations, outsourcing, reconciliations |
| Counterparty | Head of IR | fund terms, side letters, LP reporting |
| Counterparty | DPO | data protection, transfers, HIPAA where applicable |
| Counterparty | Head of Technology | ICT risk, cyber, resilience, DORA register |
| Counterparty | Portfolio CFO | healthcare holdings, reimbursement exposure |
| Counterparty | External auditor | ISA 402, valuation challenge, audit opinion |
| Counterparty | Fund administrator | NAV calculus, investor register |
| Counterparty | Depositary / custodian | safekeeping, oversight, cash monitoring |
| Counterparty | External counsel | vehicle law, side letters, regulatory filings |
| Ours | DD lead | scope, plan, pack, sign-off |
| Ours | IC | decision, conditions precedent, waivers |
| Ours | Legal | licensing, terms, conflicts |
| Ours | AML/KYC | screening, source of wealth, adverse media |
| Ours | Fund ops and valuation | NAV, marks, administrator oversight |
| Ours | Technology and cyber | control testing, resilience |
| Ours | ESG | SFDR classification, claims discipline |
| Ours | Tax and structuring | vehicle, FATCA-CRS, Pillar Two |
| Ours | Healthcare operating partner | sector playbook, clinical and reimbursement |
| Ours | External advisors | specialist reports, register checks |

---

## 5. Odoo object model

- **Project** `iCare Capital Holdings - Manager DD (icare-dd)`, visibility `employees`,
  milestones and task dependencies enabled, manager = the acting user.
- **Task stages** (kanban columns): `Intake`, `RFI Issued`, `Evidence Received`, `Testing`,
  `Findings`, `IC Review`, `Closed` (folded).
- **Tags**: 15 workstream tags + `red`/`amber`/`green` severity + `condition-precedent`,
  `monitoring`. All suffixed `(icare-dd)` so teardown finds only ours.
- **Milestones**: `DD-01 Kickoff`, `DD-02 RFI issued`, `DD-03 Evidence complete`,
  `DD-04 Testing complete`, `DD-05 Findings drafted`, `DD-06 IC decision`,
  `DD-07 Monitoring live`.
- **Tasks**: 4 phase gates + 15 workstreams x 5 items. Within a workstream the `rfi` item
  is the parent of the other four, and the phases chain by dependency
  (`evidence` -> `rfi`, `testing` -> `evidence`, `finding` -> `testing`,
  `monitoring` -> `finding`). The IC pack gate depends on all 15 finding items, so the
  dependency graph is the process.
- **Task descriptions** carry the four-line evidence block, plus `Remediation:` on reds.

## 6. CLI plan

Built by `hodoo/scenarios/icare-dd.sh {up|down|show}`, a sibling of `startup-founder.sh`
that follows the same conventions: marker-based teardown, assertions as it builds, and
`-o json` read back with python. Nothing about the existing scenario changes.

| Step | Command |
|---|---|
| prove credentials | `hodoo whoami -o json`, `hodoo version` |
| counterparties | `hodoo call res.partner create --body '{"vals_list":[...]}'` |
| project | `hodoo project create --name ... --visibility employees --milestones --dependencies` |
| columns | `hodoo project task-stages create --name ... --project ... --sequence N [--fold]` |
| gates | `hodoo milestone create --project ... --name ... --due +Nd` |
| items | `hodoo task create --name ... --stage ... --priority ... --tag ... --description ...` |
| structure | `--parent` for sub-items, `--depends-on` for the chain |
| findings | `hodoo task show`, `hodoo task deps`, `hodoo task comment --internal` |
| dashboards | `hodoo board`, `hodoo task ls --tag/--overdue/--state`, `hodoo milestone ls` |

Deliberate ceilings, marked rather than hidden:

- **No document upload.** hodoo has no attachment command, so evidence is *named* in the
  description, not stored. Upgrade path: `hodoo call ir.attachment create` with a base64
  datas payload, or a future `hodoo attach`.
- **One server, one user.** Tasks are assigned to the acting user; seats live in the
  description. Upgrade path: real user accounts, then `--assignee` per seat.
- **No IC workflow in Odoo.** Approvals are tasks in `IC Review`, not a workflow engine.

## 7. Acceptance

`up` asserts, among others: 1 project, 7 stages, 7 milestones, 20 tags, 79 tasks, 5 reds
with zero missing remediations, every task carrying the four-line evidence block, 15
workstreams x 5 items, the IC gate reading `Waiting` after its dependencies are rewritten
(Odoo does not compute state during creation), one finding at changes-requested, 3 overdue
items, and the monitoring set still open. `down` removes only `(icare-dd)` records and
asserts none are left.
