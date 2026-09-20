# P6 escalation advisory contract

This increment adds the pure P6-03 boundary between decoded semantic answers and
the existing deterministic escalation evaluator. It does not enable remote advice
or qualify an endpoint.

The request builder accepts bounded observations whose IDs and source revisions
must match the decision binding. It exposes only closed escalation actions and
commits the exact question revision. The consumer accepts only `Purpose::Escalation`
outcomes produced in advisory mode for the same binding and question revision.
Native Boolean, choice-probability and confidence thresholds are integer policy
values. A separately qualified conventional comparator can supply discrete answers
only when local policy explicitly permits them.

The resulting signal copies canonical `hard_failure`, `checks_complete` and
required-review facts rather than inferring them. A negative review answer cannot
suppress required review. Suggested retry, replan or escalation actions do not
change the deterministic result; a suggested stop can only replace a ready result
with `AdvisoryStop`. Existing caps, pins, candidate eligibility, budget and
authority checks therefore remain the only path to a ready escalation.

The focused `vcp-models` suite covers revision changes, capped-loop advice,
unverified hard failures, mandatory review, advisory stop and rejection of a
shadow-mode outcome. All model-boundary suites remain synthetic and offline.
Canonical admission, transport, persistence and live qualification are outside
this increment.

The final offline routing qualification passed 35 decision, escalation and routing
contract tests plus all 54 frozen comparison assertions with zero harness model
calls and zero spend. Its source identity remained unchanged. The manifest is
`artifacts/p6-escalation-advisory-qualification/3d82f5ad-2fbe-4203-8985-fc138ff43a2b/manifest.json`;
the report SHA-256 is
`f665e325923c7f678c069ee233407614618d1736b819fa877a70bcd64ba1e811`.
