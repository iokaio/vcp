# Synthetic local-memory corpus

Original VCP fixture under Apache-2.0. No private repository, transcript or
external corpus was used. `corpus.json` has 24 documents, two workspaces and
seven independently specified query expectations, authored before native runs.

Atlas describes a fictional coding tool. Boreal describes unrelated applications
and deliberately reuses some identifiers. Atlas's obsolete pause design remains
indexed as historical content. Version 2 names it explicitly in the replacement's
`supersedes` field; the texts and expected relevance are unchanged. Retained
Munarium governance resolves current claims and checks historical visibility.
The `current` flags are independent expected results, not the selection mechanism
or instructions supplied by the index. Final query results
must exclude other workspaces and obsolete content before applying the limit.

The original `vcp-memory-spike` executable uses the retained Munarium public APIs
and verified local MiniLM helper. See the [procedure and limits](../../../../docs/development/local-memory-spike.md).
Required lexical and semantic IDs test relevance separately from ANN recall
against the independent f64 cosine oracle. No stored fake vector can qualify the
real local inference path. This tiny fixture establishes API feasibility only.

Each process replays the public fixture into a volatile scoped backend. It does
not restore a durable claim store. The changed corpus digest intentionally makes
version 1 experiment receipts incompatible; build a new disposable experiment
directory without overwriting previous evidence. No product migration is implied.
