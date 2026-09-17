# Upstream qualification inputs

[upstreams.toml](upstreams.toml) records immutable investigation candidates.
It does not yet declare an imported selection, completed license closure or
supported runtime. No upstream source is committed in this directory.

Use the [qualification procedure](../../docs/development/upstream-qualification.md)
and [ADR-013](../../docs/adr/013-upstream-reuse-and-vendoring.md). The source
inventory tool reads Git objects without following checkout links, records
original-byte SHA-256 hashes and rejects unsafe paths and gitlinks. Its JSON
output is evidence in an ignored directory until an import selects reviewed
paths. Candidate metadata in TOML is descriptive; a complete import must add
machine-validated selection/mapping, closure, license, patch and resulting-byte
records before the candidate can become a supported component.
