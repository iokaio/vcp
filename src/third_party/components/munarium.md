# Munarium local-library selection

Revision: `8da666067000ca1ee9c131bc67e70b978862faa3` from
`https://github.com/iokaio/munarium`. State: imported, not VCP-qualified.
Owner: P0-07 selection, P0-02 local-memory experiment, P5 production integration.
Root and selected original source declare Apache-2.0; upstream NOTICE identifies
Copyright (c) 2026 Ioka LLC. Keep original LICENSE, NOTICE and source headers with
this import; this record does not license transitive dependencies by inference.

[Native qualification](../../../docs/development/munarium-baseline.md) identifies
three libraries and their required contract inputs. [Executed evidence](../../../docs/evaluations/p0-07-munarium-datastore.md)
records 200 passing tests with one intentionally ignored performance benchmark,
and a 148-package native normal/build dependency closure without the prohibited
server/provider/PostgreSQL packages. The immutable Cargo lockfile remains the
dependency version/checksum source; the normalized closure is retained in local
run evidence, not presented as a release SBOM.

Selected source includes `server/src/munarium-core`, `munarium-store-mem` and
`munarium-datastore`, plus `server/contract/matrix/VERSION` and the complete
`server/contract/datastore/` test/schema/reference inputs. The datastore's lexical
fixtures remain under its test directory. Preserve their upstream provenance;
the PostgreSQL analyzer fixture does not require a running PostgreSQL service.

The original `server/Cargo.toml`, lockfile and Rust 1.98.0 toolchain describe the
baseline. The [selection](munarium-selection.json) imports 70 files with a
[manifest-only patch](../patches/munarium/README.md) that registers the three
libraries in the Codex workspace and materializes upstream package/dependency
requirements. Rust implementation and test bodies remain unchanged. The retained
root Cargo files are provenance inputs, not a second build entry point.
The [shared import evidence](../../../docs/evaluations/p0-07-munarium-import.md)
records 200 passing tests with a 147-package normal/build graph. The
[dependency record](munarium-dependencies.json) binds package identities,
registry checksums and license declarations to the shared lockfile. It is not
a release notice bundle. [Build instructions](../../../docs/development/munarium-source.md)
consume committed source directly. Rust 1.98.0 versus Codex's 1.95.0 remains an
integration qualification choice; no second engine or supported compiler range
is implied.

The datastore owns local file/index effects, not canonical VCP authority.
The in-memory store is a reference fixture, not durable canonical storage; its
clock, UUID and budget helpers must not become an independent VCP ledger.
Munarium's provider gateway and PostgreSQL/server modules are excluded from this
selected graph. Its excluded Ollama transport is research for local inference only:
validating an HTTP(S) URL does not establish local execution or forbid remote
embedding endpoints. P0-02 must qualify a pinned local runtime/model separately.
