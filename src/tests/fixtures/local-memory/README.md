# Synthetic local-memory corpus

Original VCP fixture under Apache-2.0. No private repository, transcript or
external corpus was used. `corpus.json` has 24 documents, two workspaces and
seven independently specified query expectations, authored before native runs.

Atlas describes a fictional coding tool. Boreal describes unrelated applications
and deliberately reuses some identifiers. Atlas's obsolete pause design remains
indexed as historical content; its `current: false` flag belongs to the fixture's
canonical view, not an instruction supplied by the index. Final query results
must exclude other workspaces and obsolete content before applying the limit.

The original `vcp-memory-spike` executable uses the retained Munarium public APIs
and verified local MiniLM helper. See the [procedure and limits](../../../../docs/development/local-memory-spike.md).
Required lexical and semantic IDs test relevance separately from ANN recall
against the independent f64 cosine oracle. No stored fake vector can qualify the
real local inference path. This tiny fixture establishes API feasibility only.
