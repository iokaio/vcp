# Original U03 skill-arm continuation

This one-shot helper continues only the untouched skill arm of the reviewed
two-arm generation plan. It is not a baseline retry or a new budget request.
The original owner receipt must bind the original generation plan and the
aggregate $45 authorization. The continuation hash identifies executable
continuation code and evidence; it is never described as new owner approval.

The reviewed baseline retains six requests: five settled charges totaling
6,957 microdollars and one canonical unresolved liability of 201,640 microdollars.
The digest-checked retained SSE ends in `response.incomplete` with reason
`max_output_tokens`. Observed response usage does not retroactively settle that
canonical liability. The entire baseline $2.50 allocation remains reserved from
the experiment's allocation pool; none is transferred to the skill arm.

Preparation verifies original executable, assets, fixtures, runner sources,
provider source/catalog, owner spec, permission proposal, recorded launcher build,
derived profiles and exact prompts. It checks the original stopped claim and
two-row result, baseline cost/response evidence and full baseline inventory.
The skill workspace must remain the exact frozen source, with an empty store
and no attempted/result files. Its original $2.50 cap, eight requests, 300-second
deadline, 512-token output ceiling, zero retries and sole launcher are unchanged.

```powershell
node scripts/evals/builtin-generation-continuation.cjs prepare <original-plan> <original-approved-hash> <new-private-directory> <original-owner-receipt>
node scripts/evals/builtin-generation-continuation.cjs run <continuation-plan> <continuation-integrity-hash>
```

Execution is subject to root review and the existing exact owner authorization.
An exclusive continuation claim and exclusive original skill `attempted.json`
prevent replay, including competing continuation directories. All new task
outputs remain in the original skill directory; the continuation result retains
both original rows. The original baseline, root result and root claim are never
rewritten. Baseline preservation is checked after the attempt.

The helper reuses canonical accounting, active-skill evidence, required passed
parent verification and the independent generation oracle. An interrupted skill
attempt preserves its whole allocation as potentially unknown and stops. Known
settled charges and unresolved liabilities are reported separately; the original
unknown liability keeps the combined actual total unknown.
`known_settled_complete` is false when an
interrupted skill attempt has no complete canonical cost diagnostic, so the
retained baseline charge cannot be mistaken for the entire observed charge.
Failed verification cannot be overridden by the oracle. No model calls occur during preparation or
the injected-dispatch contract tests.
