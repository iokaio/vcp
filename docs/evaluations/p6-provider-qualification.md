# P6 provider qualification

On September 21, 2026, offline qualification v2 accepted both actual Responses
tool/continuation probes under [ADR-041](../adr/041-provider-evidence-and-conservative-routing.md).
Each probe has two completed, cost-accounted Responses and two corresponding
generation receipts. Each receipt records one successful non-BYOK provider
attempt; response identity and rounded charge agree. Both receipts within each
probe identify the same served endpoint and generation model.

| Arm | Requested model | Exact catalog tag | Provider | Observed generation model |
| --- | --- | --- | --- | --- |
| Economical | `anthropic/claude-3-haiku` | `amazon-bedrock` | Amazon Bedrock | `anthropic/claude-3-haiku` |
| Stronger | `anthropic/claude-haiku-4.5` | `anthropic` | Anthropic | `anthropic/claude-4.5-haiku-20251001` |

The complete dated catalog must map the provider name to exactly one endpoint.
Its `model_id` must equal the requested model, and its `name` must exactly join
the provider name and observed generation model with ` | `. Agreeing generation
fields cannot qualify an unrelated model. Missing or ambiguous mappings reject.
The association method is
`generation-single-attempt-exact-catalog-model-provider/2`.

## Exact evidence identities

All values below are SHA256. “Source bundle” identifies the authorized offline
input document, which pins the original probe spec/report, both generation
receipts and the dated catalog. Snapshot file digests and logical snapshot IDs
are recorded separately.

| Evidence | Economical | Stronger |
| --- | --- | --- |
| Source bundle | `8c26ffabf6e10ac04d021f09ea9ca567880fe8a00c73ce0cd60edd8891e44578` | `24924e69b81f4132ad3a8bd0dcf22112e5ddf44a0d8677a592025b9ed4e7e6e7` |
| Qualification claim | `a8405ac41c78f06758e7a8b51cd2fea1af84eb991b50ffc21db2e08f0da19f9d` | `eeee205df007db258353f85266e1809c38e51a7f9608c9062a3b8ccbf0d7bdcc` |
| Catalog | `abc476ae65e98c175ea379bd7d166a1235893b8f842c48de722f70372e1c0088` | `122317d06fc84be58d3bfa3e140fe280445060c156477f8308ea7cc6bca8013e` |
| Probe report | `7c9ee6650eeb8b44d72963f3f2d1524b46afcc7b9ee404570904f0a53c930c06` | `e41630ab7646de662a008d82ce0bd50074e54889550ef4e6323445e373343618` |
| Snapshot file | `ac25b87a91c5ad254eb571622af294583cad3fe9c8bab77088877c028ef7e5c7` | `31a9d62a6346f3f0fd00999f6ea345c0394ab3a15b1701265d7f4134ac885a29` |
| Snapshot ID | `8ebb68478b60216385518626b5a1da05c101d7322af69fa1720424f28932e869` | `49ea66b17fd498463ab1a27cab23f726eb5f1b7d217956a9101bd038385aa1c7` |

The reviewed qualification binary SHA256 was
`cf92b0061cf01963143afc9945cc34a10037d2fd14375ad5d0ab1d3888ad6288`.
Requalification after the exact model-revision binding fix produced snapshot
files and IDs identical to the earlier accepted snapshots. The relevant model
library tests passed (nine tests); both fresh offline qualification commands
passed. Requalification made no inference requests and incurred no model charge.

## Bounds and limits

Both snapshots retain `byte_ceiling_qualified: false`. Admission reserves the
full 200,000-token input capacity independently for ordinary input, cache reads
and cache writes, plus configured output and request maxima. A byte count is not
used as the monetary input bound. Catalog output maxima are 4,096 and 64,000
tokens respectively; the qualification probes and profile campaign used their
separately configured smaller output caps.

The observations are dated 2026-09-21 16:01:50 UTC. The economical snapshot expires
2026-09-22 14:27:18 UTC; the stronger expires 14:35:26 UTC that day. These are
historical qualification windows, not perpetual authorization. Relevant catalog,
tariff, model, schema or provider changes require renewed qualification.

The join trusts owner-reviewed files obtained from OpenRouter's authenticated
generation API and public catalog. Hashes detect changed files; they do not
authenticate an arbitrary local file or independently attest a downstream
provider's infrastructure. Absent provider identity in the Responses payload
remains absent; the catalog association is explicitly derived from the additional
receipts. This evidence supports the bounded observed protocol and provider
controls, not global tokenizer accuracy, model quality, reasoning effort or
automatic profile activation. Quality dispositions are recorded separately in
the [profile gate](p6-profile-gate.md).
