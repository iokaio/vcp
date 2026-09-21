# P6-01/P6-04 — Conservative provider admission

Live endpoint inspection found prices that the earlier metadata reader did not
include: long-context tariff overrides and one-hour cache-write rates. The reader
now takes the maximum applicable listed rate for each supported charge category.
It rejects malformed or unsupported override structures. Request charges remain
bounded by an explicit `provider.max_price.request`; omitted request pricing does
not become an implicit zero.

When this correction changes a normalized tariff, the snapshot carries
`tariff_normalization: 2` and a distinct price/snapshot identity. Existing snapshots
whose interpretation is unchanged retain their original bytes and identities.
Configuration still reconstructs the snapshot from captured raw endpoint metadata;
a supplied price cannot replace that reconstruction.

A fixed provider can retain `byte_ceiling_qualified: false`. Admission then
reserves the endpoint's full input capacity for each possible input/cache
partition, plus the selected output and request bounds. Actual context bytes
remain a fit estimate. Two successful small probes do not prove a tokenizer bound.
Automatic routing retains its separate qualified-byte gate; this fixed-provider
fallback does not qualify a routed cost estimator or model membership.

The [qualification executable](../../scripts/evals/p6-live-conformance.md) uses a
separate candidate-metadata type and an isolated canonical task. It does not
construct a production snapshot with invented compatibility flags. Its two fixed
synthetic requests reserve before send, capture actual responses and settle only
observed cost. A missing final cost or interrupted request retains liability and
prevents continuation. The second request must refer to the first accounted tool
call. Its HTTP client disables redirects, proxies and retries.

The local conformance test passed with a scripted peer: each 1,000-microdollar
reservation settled an observed seven-microdollar charge, while missing cost
stopped after one request and retained the full unresolved reservation. This is
accounting evidence, not live compatibility. Seven native provider/retry tests
also passed on Files and SQLite, including the unqualified-byte full-input
reservation (`artifacts/p6-provider-bounds-native-final.log`). Raw live trials and their separately
authorized aggregate cap are recorded independently of the reusable fixtures.

No shipping model group, statistical quality floor, tokenizer guarantee or
provider-policy qualification follows from this implementation alone.
