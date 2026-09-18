# Upstream qualification inputs

[upstreams.toml](upstreams.toml) records immutable investigation candidates.
Codex and three Munarium libraries are now committed qualification selections,
with mapping, patches, license records and original/resulting byte inventories.
Gemini remains an external comparison candidate. These records do not establish
a supported VCP release or completed production integration.

The [local embedding record](components/embedding-runtime.md) identifies installed
Candle/Tokenizers dependencies and immutable external model assets used by the
original VCP CPU helper. Model weights remain outside the checkout; their license
declaration is recorded separately from software terms.

Use the [qualification procedure](../../docs/development/upstream-qualification.md)
and [ADR-013](../../docs/adr/013-upstream-reuse-and-vendoring.md). The source
inventory tool reads Git objects without following checkout links, records
original-byte SHA-256 hashes and rejects unsafe paths and gitlinks. Its JSON
output is evidence in an ignored directory until an import selects reviewed
paths. Candidate metadata in TOML is descriptive; a complete import must add
machine-validated selection/mapping, closure, license, patch and resulting-byte
records before the candidate can become a supported component.
