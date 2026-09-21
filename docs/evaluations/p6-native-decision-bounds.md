# P6-02 / P6-04 native decision charge bounds

The September 21, 2026 public [Jev endpoint catalog](https://openrouter.ai/api/v1/models/typesafe/jev-1.13/endpoints)
advertised `typesafe/jev-1.13`, exact endpoint tag `typesafe`,
`text->decisions`, a 32,000-token context, prompt price USD 0.042 per million
tokens, and explicitly zero completion price. This is metadata evidence, not
live operation, provider-control or answer qualification. The
[primary model limits](https://docs.typesafe.ai/models), inspected on the same
date, distinguish 32k for state plus the longest question from **64k for the
complete state plus all questions**. VCP reserves the latter for each input/cache
category. The 32k catalog value alone is insufficient for total batch accounting.
This service contract is restricted to exact `typesafe/jev-1.13` and the `typesafe`
endpoint; context, model or pricing drift requires requalification.

The [official Decisions request schema](https://github.com/OpenRouterTeam/typescript-sdk/blob/main/src/models/decisionsrequest.ts)
has state and questions but no output-token limit. VCP therefore accepts a native
charge bound only when completion pricing is explicitly zero. The whole 64k
request is reserved independently for ordinary input, cache reads and cache
writes, using at least the ordinary input rate. A configured request-price ceiling
still requires separate evidence of operation-specific enforcement. No tools are
available. Future unknown charge categories, nonzero output pricing or tariff
tiers fail closed until their finite bounds are implemented and qualified.

`vcp_models::decision::native_bound` parses bounded, duplicate-aware raw catalog
JSON and checks exact model/modality, a unique active endpoint and the dated limits.
The canonical worker rereads the owner-captured catalog; production installation
requires its digest and rates to match the native capability. A caller-supplied
number alone cannot enable native transport. Existing fixtures keep their separate
loopback-only origin and cannot supply production qualification.

This resolves the prior unconditional native-charge rejection without asserting
that catalog availability proves conformance. Exact operation/identity/control
evidence, canonical admission, unknown-cost retention, no replay, deadlines and
the per-purpose held-out gates remain required. No remote evaluator is enabled
by this increment. The bounds tests use local peers; the separately authorized
live probe is recorded below.

## Bootstrap probe

The qualification-only binary accepts `--native <spec.json> <new-private-directory>
<authorized-spec-sha256>`. It uses the existing provider-conformance spec shape,
with `max_output_tokens: 1` solely as an internal zero-price reservation sentinel;
no output-limit field is sent to native Decisions. The closed request targets
`https://openrouter.ai/api/alpha/decisions` with one fixed public synthetic state
and a mixed Noul/Choice/Score batch. It sends exactly one request and cannot install
an evaluator or retry a failed or uncertain attempt.

The raw catalog is independently parsed again inside the canonical worker before
admission. The request is captured before send, the raw response is retained, and
only an observed decimal cost with a response ID settles the reservation. Invalid
or missing cost remains unresolved; even HTTP errors retain any trustworthy
observed charge. The permanent output claim binds the spec, binary and compiled
source. Files must be fresh and outside the repository and sync/reparse roots.

An `observed` report means that a response and charge were captured. Its explicit
`qualified: false` prevents this transport smoke result from claiming native
semantics, control enforcement, calibration, utility or installed production
qualification. Review retained fields and exact served attribution separately
before running a purpose comparison.

## Observed native protocol probe

The authorized September 21 run sent the fixed mixed batch to the actual alpha
Decisions endpoint. It returned model `typesafe/jev-1.13-20260917`, provider
`TypeSafe`, input 410 and output 63 tokens, and observed cost USD 0.00001722.
Canonical settlement rounds upward to 18 microdollars, with zero unresolved
liability for this attempt. The result document SHA256 is
`6c5b0d70acba20934bf115caef3a343b343d19765d63acf4e4c1e10489fbc7cf`;
its immutable spec SHA256 is
`9a6955e6edee7fad756c3fd9cfc96da7ae5f712bb5c7f9ef6b4c9c01a2cccdb8`.

The actual Noul result was 0.96. Choice returned `review`, probabilities
`repeat: 0, review: 1` and separately reported confidence 1. Score returned the
fractional value 1.8, probabilities `0: 0.01, 1: 0.18, 2: 0.81`, the original
three-level legend and separately reported confidence 0.7. The fractional score
matches the weighted level mean; confidence is not substituted for its winning
level probability. These are actual service outputs on one synthetic example,
not calibrated VCP probabilities or evidence of task-level utility.

The probe preserved explicit provider-only routing, no fallback, parameter
enforcement and data-collection denial in the admitted body. Served identity and
known cost were observed. Negative-control enforcement, cancellation billing and
purpose-specific quality still require their own evidence; the bootstrap itself
does not install a production qualification record.
