# CS-1 full diagnostic qualification

On September 25, 2026 the owner directed: “Modify the fixture to allow a full
test as we are well under budget.” This authorizes a new fixture revision and
fresh comparison runs. It does not turn the
[halted predecessor](cs1-followup-outcome.md) into a passing campaign.

## Fixture revision

The four fresh cases use `cs-1-followup-fixtures-v3` and new `-v3` IDs. Their
explicit tool lists now permit read-only `vcp_search`, alongside `vcp_list`,
`vcp_read`, bounded `vcp_patch`, and the separately authorized `vcp_verify`.
The previous 22 active fixture files are preserved exactly under `history/v2`.
All 15 task source files, prompts, rubric bytes and quality criteria are unchanged.
The 12 inherited cases have no equivalent explicit tool-list restriction and
remain unchanged. Arbitrary process execution, network access, installation,
sending and generated-package activation remain unauthorized.

The native checker accepts only the new four IDs and the exact revised tool
list. Tests reject old IDs and expansion to `vcp_exec` or `vcp_mcp`. This is a
fixture-contract check; it does not claim the runtime hides every unpermitted
tool. Canonical receipts and independent authority review still govern actual use.

## Full schedule and qualification

The fresh envelope retains 54 fixed comparison slots: for each of two candidates,
six fresh normal runs, 18 inherited runs and one repeated triplet. Each case has
none, nearest and candidate arms with fixed rotations. No retries or replacement
slots are authorized. Quality and native completion failures remain failures but
do not truncate this diagnostic schedule.

Each completed phase still requires retained independent review before the next
phase. Authority or secret-handling failure in any arm halts the entire envelope.
Integrity drift, unknown liability, an unreconciled attempted slot, or insufficient
budget also halts dispatch. Full execution does not waive qualification criteria.

The repeated triplet uses the lexicographically first common normal-case benefit
win. If there is no common win, it uses the first fresh case lexicographically as
an explicitly unqualified diagnostic repeat. Passing that repeat cannot qualify
a candidate whose normal or inherited gates failed. Qualification still requires
all candidate gates and a same-case usefulness improvement of at least one point
over each baseline, without lower completeness or clarity, from both independent
reviewers, followed by the repeat win.

## Cumulative spending

The $162 / 864-request ceiling includes the predecessor's $0.123408 and 47
requests. Therefore $161.876592 and 817 requests remain before new execution.
Every task retains a $3 cap, 16-request ceiling and 2,048 output-token limit.

The new spec pins the predecessor's accounting, result and halt receipts. Before
each dispatch the runner reconciles every attempted slot across completed phases
and the current partial phase, including failures. It reserves the entire next
$3 / 16-request allowance against the cumulative ceiling. Unknown or omitted
liability is not treated as zero. If there is insufficient room for that full
reservation, remaining cases are not run. An exclusive successor claim prevents
parallel envelopes from each spending the same remainder.

The original envelope, claims, results and halt remain untouched. New execution
uses a new private envelope, workspaces, task identities and exact hashes. The
existing provider qualification and its expiry are unchanged. No extra provider
probe, paid reviewer, model-profile change or budget increase is implied.

## Verification and execution record

All 22 native checker tests passed, including rejection of old case IDs and
unintended `vcp_exec`/`vcp_mcp` expansion. All four native feasibility cases passed
with synthetic provider responses and real checker execution (5, 5, 8 and 9
requests respectively). No live provider calls were used for these checks.

Independent code review identified copyable-path and temporary-directory
weaknesses in successor exclusion. The corrected claim is keyed by the prior
envelope hash in the repository's shared Git administration directory. Tests
exercise copied receipt bytes and changed `TEMP`/`TMP`, alongside omitted
attempted slots, full diagnostic progression, and retained qualification gates.

All 28 scheduling/budget tests passed, including full 27-run diagnostic progression
for one synthetic candidate; the 24 inherited runner/oracle tests also passed.
The new test group took 294.18 seconds on native Windows while the checker build
was active. The registered authoring case now includes the budget tests and has
a bounded 600-second timeout. Two earlier development test attempts detected
source changes while fixes were being applied; the final run used stable inputs
and had no failures or skipped tests.

Documentation validation passed for 535 Markdown files and 2,795 relative links,
with the existing 68-item architecture ledger unchanged. Independent code review
reported no remaining must-fix findings after the exclusion fixes.

The exact offline native checker build passed with identical before/after source
inputs. Checker SHA-256:
`a26e8fa5ae65c9d3f293bfa5b08f30359e566671debebe466f0aea6e149dc2b2`.
Its receipt is retained at
`artifacts/cs1-checker-build/08fab607-e29d-48a3-8b75-d9408ce5d9ef/build-receipt.json`.
No new live execution is claimed until the envelope and retained results are added.
