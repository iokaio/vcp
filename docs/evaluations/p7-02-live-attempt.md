# P7-02 approved live trial: authentication failure

On September 21, 2026 the owner approved the prepared eighteen attempts and
$45 aggregate cap, including the sole generation verification launcher's exact
permission proposal. This supersedes the earlier pending-approval state in the
workflow preparation record. Source revision: `875da8039b267107b01f35b31dd967ec81589646`.

| Approved plan | SHA-256 | Allocation | Observation |
|---|---|---|---|
| Read-only baseline/skill pairs | `9d36819e22aee9df0e78c26762f9dae2d94539364d25317b66ea9361ece8d732` | $40; sixteen attempts | First attempt failed; fifteen not run |
| U03 generation pair | `f936475020c5144422f60d8f8397387b2de8b15a9a84cf6ade381a37ec1a68e2` | $5; two attempts | Not started; no execution claim |

Both exact plan hashes, current provider profiles and fresh stores validated
before dispatch. The read-only runner then permanently claimed its plan and
submitted the `architecture-normal-v1` baseline request. The captured response
was HTTP 401, `{"error":{"message":"User not found.","code":401}}`.
The retained fifty response bytes have SHA-256
`6faf1b143addca514302d08060df34ba407457f06d73add7ca78af04a158c8f9`.
The response capture is aborted, with authentication/recovery material excluded.

The canonical task paused with one attempt in `reconciliation_pending`, zero
active reservations, **$0 settled and $0.201640 unresolved liability**. Actual
cost is unknown; an authentication error is not substituted for provider charge
evidence. There was no final model answer or usefulness grade. The runner's cost
gate stopped subsequent dispatch and retained the failed case and all fifteen
not-run cases. The two separately approved generation attempts were not started
with the rejected credential. There were no retries or repeated claims.

Local private receipts remain beside the original plans: `execution-claim.json`,
`result.json`, the first case's `stdout.jsonl`, canonical costs/routing/outputs/
context pages, and `retained-response-inspection.json`. A separate
`owner-authorization.json` records the owner's approval without modifying either
hashed plan. The non-secret summary is retained locally at
`artifacts/p7-02-approved-live-observation.json`; credentials and private trial
paths are not published here.

## Remaining work

The inherited `OPENROUTER_API_KEY` was rejected. No alternate value exists in the
Windows user or machine environment. A valid credential through an authorized
local mechanism is required before further live work. Preserve the unresolved
reserve unless reliable provider accounting evidence establishes the charge.
The shipped CLI has no charge-import command; an HTTP status alone cannot supply
the provider request identity, final usage and retained evidence required by the
canonical usage-observation API.

The original read-only plan is permanently claimed and has no continuation
operation. It must not be replayed or have its failed attempt removed. Any
continuation must preserve the original denominator, liability and approved
aggregate cap. The generation plan remains unclaimed and subject to its existing
hash/profile-validity checks. P7-02 remains in progress; no live usefulness,
generation or unavailable-toolchain acceptance is marked complete.
