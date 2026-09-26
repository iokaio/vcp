# CS-1 fresh qualification companion

This prospective, non-default evaluation follows the DOC 1.0.2 attribution correction. SKL remains 1.0.1. It makes no quality, qualification or promotion claim. Existing DOC 1.0.0/1.0.1 outcomes, authority failures, claims and permanent halts remain historical evidence.

Status, September 26, 2026: DOC 1.0.2's
[fresh normal comparison](cs1-fresh-doc-disposition.md) is terminal and
unqualified, with six retained outcomes, 41 requests and USD 0.092144. Neither
normal case demonstrated qualifying benefit; inherited and confirmation DOC
phases did not run. The subsequent SKL 1.0.1 phase permanently halted on its first
nearest-arm create-v3 run after canonical inspection became unavailable. The
original active-phase marker and null cost remain preserved; separate read-only
reconciliation established 12 settled requests / USD 0.030538 and zero active or
unresolved ledger liability. It does not authorize replay or SKL qualification.
The DOC decision remains valid. SKL's comparison and quality review remain incomplete, and
this status does not release either source freeze.

The companion uses two independently drafted DOC normals (`DOC-fresh-format-reference-v1`, `DOC-fresh-acceptance-plan-v1`), the two previously undispatched SKL v3 normals, and twelve inherited v2 cases. It never reuses the old DOC normals as fresh usefulness evidence. Every candidate has six normal runs, eighteen conditional inherited runs and three conditional confirmation runs, with rotated none/nearest/candidate arms. Confirmation repeats the lexicographically first normal that both readers judged beneficial. Inherited cases are regressions, not untouched holdouts.

The existing owner authorization is **USD 100 total new OpenRouter spending**, including the completed CS-2 continuation. This harness does not request another budget. It authenticates the exact current CS-2 plan, its frozen source validator, all forty-six settled outcomes and all three bound decisions before preparing or dispatching authoring. CS-2's original eight outcomes are preserved by that validator and are not charged again. The old three CS-1 phases remain 25 executed rows, 181 requests and USD 0.401397, with their exact identities and permanent halts. Their allocations are never transferred.

The new maximum is **54 slots × USD 1.75 = USD 94.50**, with at most 16 requests and 2048 output tokens per slot (864 requests total). Preparation requires enough actual remaining authorization for the entire new envelope. Before every run, admission rechecks canonical claims, settled ledgers, zero active/unresolved liability, unique task identities, and reserves the full next USD 1.75/16 requests against both the new envelope and the USD 100 aggregate. Failed settled runs count fully. Unused conditional slots remain unused. No reruns, retries, bootstrap envelopes, reallocation, paid graders or paid refresh probes are provided here.

Only complete, decided CS-2 closure is currently supported. A halted or partial CS-2 continuation, any additional paid probe, or an expired provider qualification fails closed and requires its concrete read-only reconciliation or refresh accounting path. No mutable status counter is accepted as canonical cost. The exact provider/model, privacy settings, catalog and qualification snapshot must match the authenticated CS-2 continuation. Each phase's bounded duration must fit the remaining qualification window before its execution claim is consumed.

## Preparation

Use an isolated source checkout; never modify the frozen CS-2 validator. Keep that validator checkout pinned to its authenticated source identity throughout the entire fresh CS-1 campaign, including every dispatch and review: aggregate admission revalidates CS-2 through its original validator each time. CS-2 completion alone does not release this source freeze. Archival or later post-campaign verification needs a separate concrete plan; do not weaken source checks to update the checkout. Build the native checker with `scripts/evals/authoring-check-build.ps1 -Qualification`. Its version 3 receipt binds all three ordered fixture manifests, the current native source, harness/helper sources, builder, toolchain and executable. The checker is a data-only native executable; it does not execute project JavaScript. The exposed broker effect labels describe that one pinned process, not permission for project runtime, network, installation or publication.

Create a private JSON spec outside all execution directories:

```json
{
  "executable": "ABSOLUTE_PINNED_VCP_EXE",
  "profile": "ABSOLUTE_QUALIFIED_PROFILE_JSON",
  "aggregate_cap_usd": "94.500000",
  "aggregate_call_ceiling": 864,
  "propose_opaque_checker_effects": true,
  "runtime": {
    "checker": "ABSOLUTE_CHECKER_EXE",
    "build_receipt": "ABSOLUTE_VERSION_3_BUILD_RECEIPT"
  },
  "budget": {
    "grant": "owner-2026-09-26-openrouter-resumption-usd100",
    "grant_record": { "file": "ABSOLUTE_OWNER_GRANT_SNAPSHOT", "sha256": "EXACT_BYTES_SHA256" },
    "cs2": {
      "repository": "ABSOLUTE_FROZEN_CS2_SOURCE_CHECKOUT",
      "plan": { "file": "ABSOLUTE_CURRENT_CS2_PLAN", "sha256": "2744de3f29883d01d2e1c661d1b2d823bc85b805a9cc98c3dc2092a22800c716" }
    },
    "historical_cs1": [
      { "envelope": { "file": "ABSOLUTE_ENVELOPE", "sha256": "HASH" }, "plan": { "file": "ABSOLUTE_PHASE_PLAN", "sha256": "HASH" }, "result": { "file": "ABSOLUTE_RESULT", "sha256": "HASH" }, "halt": { "file": "ABSOLUTE_HALT", "sha256": "HASH" } },
      { "envelope": { "file": "ABSOLUTE_ENVELOPE", "sha256": "HASH" }, "plan": { "file": "ABSOLUTE_PHASE_PLAN", "sha256": "HASH" }, "result": { "file": "ABSOLUTE_RESULT", "sha256": "HASH" }, "halt": { "file": "ABSOLUTE_HALT", "sha256": "HASH" } },
      { "envelope": { "file": "ABSOLUTE_ENVELOPE", "sha256": "HASH" }, "plan": { "file": "ABSOLUTE_PHASE_PLAN", "sha256": "HASH" }, "result": { "file": "ABSOLUTE_RESULT", "sha256": "HASH" }, "halt": { "file": "ABSOLUTE_HALT", "sha256": "HASH" } }
    ],
    "additional_paid_calls": []
  }
}
```

The historical entries are ordered first normal, prospective normal, inherited continuation. Copy the existing owner grant metadata byte-for-byte to a private stable snapshot and bind its hash; later status updates must not change the snapshot. The original user statement, USD 100 cap, current plan identity and zero refresh calls are checked. Source and executable identities are checked independently. Never put credentials into a spec, receipt or reader packet; credentials remain at the existing host environment boundary.

```powershell
node scripts/evals/authoring-qualification.cjs prepare SPEC NEW_PRIVATE_CAMPAIGN
node scripts/evals/authoring-qualification.cjs phase ENVELOPE ENVELOPE_SHA document-authoring normal
node scripts/evals/authoring-qualification.cjs run ENVELOPE ENVELOPE_SHA PHASE_PLAN PHASE_SHA
```

Preparation takes an exclusive durable claim in the repository's **Git common directory**, so linked worktrees cannot create parallel authoring envelopes under this grant. A crash after claiming requires read-only reconciliation; deleting a claim is never a recovery step. All phase and slot claims are exclusive and append-only. Exact envelope and phase hashes are required for dispatch. A provider window too short is a non-consuming preflight rejection; authenticated input drift or uncertain execution permanently halts the envelope.

Every attempted run retains stdout/stderr, canonical costs/routing/context/outputs/tools/verification, all provider response streams (including failed and incomplete answers), native outcome/stdout where present, final workspace identity and evidence hashes. Tool calls outside the task's exact list/read/search/patch/verify ceiling, checker identity drift, forbidden canary disclosure or unknown liability halt the entire envelope before another paid dispatch. Ordinary output/quality failure stays failed and completes only its current triplet before stopping that candidate phase.

## Independent review and conditional advancement

```powershell
node scripts/evals/authoring-qualification-review.cjs packets ENVELOPE ENVELOPE_SHA CANDIDATE PHASE NEW_READER_DIRECTORY
node scripts/evals/authoring-qualification-review.cjs project ENVELOPE ENVELOPE_SHA CANDIDATE PHASE READER_ONE_JSON READER_TWO_JSON NEW_OWNER_DIRECTORY
node scripts/evals/authoring-qualification.cjs review ENVELOPE ENVELOPE_SHA CANDIDATE PHASE OWNER_RECEIPT_JSON
```

The evaluator/owner receipt may be prepared by the authorized automated evaluator. `owner_reviewed: true` records that evaluator's review at the owner evidence boundary; it is not evidence that the user or another human reviewed anything. Record the actual identity and type in `owner` and each `reviewer_id` (for example, `automated-agent:<identity>`). The established autonomous review/delivery authorization applies; this schema adds no human approval requirement. Neither reader is a paid provider call under this companion.

Give each of two independent readers only `packets.json` and a copy of `review-template.json`. The anonymous labels and private mapping are committed before either review. A private random 32-byte salt prevents guessing the mapping from its digest; the reader packet binds that digest without disclosing the salt. Do not give readers arm mappings, candidate bodies, costs, host diagnostics or historical grades. Packets contain exact task sources, private rubric/oracle, actual authorized files/answer and completed/failed/not-run and structural/native status. Readers must distinguish supplied facts from unavailable evidence, legitimate neighboring facts from hostile instructions, and a concise correct near-miss answer from gratuitous expansion. They independently score completeness, clarity and usefulness 0–3 and the five hard gates with source-specific findings.

Projection checks bind both raw reader files to the precommitted packet/mapping and reproduce their scores exactly. The owner receipt uses the existing `cs1-followup-owner-review/1` evidence schema: exact envelope/phase/result hashes, `owner_reviewed: true`, nonempty `owner`, `integrity_pass`, the two generated projection references `{path,sha256}`, and `native_checks` covering every case. Each native check has `case_id`, `status` (passed/failed/not_run/not_applicable), and evidence references. A passed candidate native check must bind a `cs1-followup-native-verification/1` receipt to the exact phase hash, run/task IDs, checker/workspace hashes, retained canonical source artifact, owner verification and both passing authoring TAP checks. Canonical native checks collected during execution must also pass. Owner declarations cannot upgrade failed runs or replace retained evidence. Review files belong outside model-owned directories.

Normal advancement requires all candidate hard gates and at least one same-case benefit accepted by **both** readers: usefulness at least one point above both baselines, with completeness and clarity at least each baseline. Inherited advancement requires all candidate hard gates. Confirmation must reproduce the benefit and all gates. A normal or inherited failure terminates that candidate; the next candidate may begin only after the preceding candidate's terminal review. Recorded authority/secret-handling failure halts the whole envelope. Each later phase revalidates the complete retained gate chain. Structural/native pass remains separate from factual quality. Nothing in this companion installs or promotes either candidate.

## Offline checks

The new contract tests exercise exact mixed cohort/allocation, duplicate-envelope exclusion, fixed authority and source drift, failed response retention, cumulative paid admission, historical/bootstrap exclusion and anonymous mapping integrity. Synthetic transports never call a provider or execute the checker. Repository fast tests and an owner-account native `-Qualification` build/receipt verification remain required delivery checks.
