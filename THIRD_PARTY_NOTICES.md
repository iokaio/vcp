# Third-party notices and provenance

VCP's original work is licensed under [Apache-2.0](LICENSE). This inventory records material actually used in the repository; research candidates are not automatically distributed dependencies.

## Munarium community guidance

- Source: [iokaio/munarium](https://github.com/iokaio/munarium/tree/8da666067000ca1ee9c131bc67e70b978862faa3).
- Inspected revision: `8da666067000ca1ee9c131bc67e70b978862faa3`.
- Copyright: (c) 2026 Ioka LLC.
- License: [Apache License 2.0](https://github.com/iokaio/munarium/blob/8da666067000ca1ee9c131bc67e70b978862faa3/LICENSE); a copy of the same license is included as [LICENSE](LICENSE).
- Original notice: [Munarium NOTICE](https://github.com/iokaio/munarium/blob/8da666067000ca1ee9c131bc67e70b978862faa3/NOTICE). Relevant attribution is retained in VCP's [NOTICE](NOTICE).

The root README organization, contribution guidance, support/security/name-policy structure, and pull request disclosure conventions were adapted for VCP from Munarium's `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `SUPPORT.md`, `TRADEMARK.md`, and `.github/pull_request_template.md`. VCP's corresponding files replace the upstream product descriptions, component commands, release claims, and commercial terms with VCP-specific text. The issue templates are written for VCP's planning and implementation workflow.

VCP's code of conduct is project-specific wording informed by the community standards in Munarium's `CODE_OF_CONDUCT.md`; it does not reproduce the Contributor Covenant text distributed by Munarium. Munarium's file identifies its own Contributor Covenant attribution separately.

This community-document adaptation is separate from the runtime selection below. Statements in Munarium's original notice about its components, migrations, and enterprise distribution describe that project, not VCP.

## Munarium local-library source

The 70 files at `src/third_party/munarium/` come from the same immutable
Munarium revision above. The original [LICENSE](src/third_party/munarium/LICENSE)
and [NOTICE](src/third_party/munarium/NOTICE) are retained, including Ioka LLC
copyright and trademark attribution. [Selection records](src/third_party/components/munarium-selection.json)
and [component notes](src/third_party/components/munarium.md) identify the three
libraries, tests, contract fixtures and baseline build inputs.

The [ordered patch](src/third_party/patches/munarium/README.md) changes only the
three library Cargo manifests: explicit upstream package/dependency requirements
and membership in the shared Codex workspace. Modification comments identify
these changes; Rust implementation and test bodies retain their original bytes.
Server, provider and PostgreSQL packages are excluded.

The embedded `PG_ENGLISH_STOP_WORDS` array and `pg16-english.stop` fixture derive
from PostgreSQL's Snowball English stopword list. The 127-line fixture matches
`src/backend/snowball/stopwords/english.stop` byte for byte at PostgreSQL 16.15
commit `7d3e000c5961a544302072058a1184e9a588837b`, SHA-256
`b3f772a000465cb76e23adb03b47073c591c156fad8f7af09c8b8e80d6bd8eac`.
PostgreSQL's [Snowball provenance](https://github.com/postgres/postgres/blob/7d3e000c5961a544302072058a1184e9a588837b/src/backend/snowball/README)
identifies Snowball and the stopword source. Retain the original
[PostgreSQL notice](src/third_party/licenses/postgresql-16.15-COPYRIGHT) and
[Snowball BSD notice](src/third_party/licenses/snowball-2.2.0-COPYING), the latter
from Snowball `48a67a2831005f49c48ec29a5837640e23e54e6b` (2.2.0).
Munarium's Apache source header does not replace these embedded-data terms.

The [147-package dependency record](src/third_party/components/munarium-dependencies.json)
records selected normal/build package identities, registry checksums and upstream
license declarations for the native Windows feature set, bound to the shared
lockfile. External dependencies are provisioned by Cargo, not vendored by this
import. This record is not a release notice bundle or a license determination
for every optional feature.

## OpenAI Codex source baseline

The source at `src/third_party/codex/` is selected from
[openai/codex at 3d3ae4965ab370217e871b3a7f0d15589557ee4b](https://github.com/openai/codex/tree/3d3ae4965ab370217e871b3a7f0d15589557ee4b).
It retains the upstream [Apache-2.0 license](src/third_party/codex/LICENSE) and
[NOTICE](src/third_party/codex/NOTICE): Copyright 2025 OpenAI, with Ratatui-derived
code under MIT and the original Florian Dehau/Ratatui Developers attribution.
[Component notes](src/third_party/components/codex.md) and
[source-selection records](src/third_party/components/codex-selection.json)
identify all retained paths, closure inputs, notices and hashes.

Selected bundled components retain their separate terms: bubblewrap
[LGPL-2.0-or-later source and license](src/third_party/codex/codex-rs/vendor/bubblewrap/COPYING),
WezTerm [MIT](src/third_party/codex/third_party/wezterm/LICENSE), bundled skill
licenses, and native voice [notices and license texts](src/third_party/codex/third_party/voice/NOTICE.md).
The license symlink `codex-rs/vendor/bubblewrap/LICENSE` is explicitly materialized
as a regular copy of `COPYING` for Windows. VCP's ordered
[compatibility patch](src/third_party/patches/codex/README.md) raises the
`codex-chatgpt` crate recursion limit and registers the Munarium libraries in
the workspace/lockfile, then adds the local CPU embedding adapter and its reviewed
dependencies. The sixth patch changes four controller/extension files and adds
five continuation-admission cases to the retained turn-input integration suite.
The eighth patch adds private startup/model/tool admission, completion receipt
callbacks and native Job Object membership observation. Its new extension module
is original VCP code; the surrounding retained modules preserve upstream ownership.
Existing serialization, hashing and process dependencies are connected to the
original lifecycle package without changing external package identities.
The seventh patch adds thread identity to admission, an owned interruption method,
three additional retained-controller cases, and the original `vcp-lifecycle`
workspace/lock entry. Its Rust host and seven real-controller tests are original
VCP code using the retained Codex APIs and synthetic test helpers; no new external
dependency identity or third-party source is imported.
Modified files carry notices; original
copyright and license terms remain unchanged.
Patch 0011 registers original VCP domain, protocol, store and engine packages in
the same workspace and lockfile. They reuse existing serialization, UUID, SHA-256,
SQLx, Tokio and error dependencies; no external dependency identity/checksum or
third-party implementation is added by this increment.
Patch 0012 also registers original VCP budget and audit packages with those
existing dependencies; external identities and checksums remain unchanged.
Patch 0014 registers original P2 repository/context packages with existing pinned
dependencies and retained native process containment; no external pins change.
See [the context guide](docs/development/p2-context.md).
Patch 0015 registers the original provider codec and carries an optional host
deadline through retained HTTP response headers/body. Existing external dependency
pins remain unchanged; see [the provider guide](docs/development/p2-provider.md).
Patch 0016 registers the original policy package and canonical store/engine
dependencies without changing external pins. See [the authority guide](docs/development/p2-policy.md).

Patch 0013 adapts retained HTTP request/response and private host admission seams
to the canonical foundation. Original VCP host code reuses those APIs and existing
Wiremock fixtures; no new external source or dependency identity is imported.
Individual source copyright headers remain intact. No voice DLLs, Microsoft
redistributables, model assets or VCP release package are distributed by this import.

## Portable storage qualification dependencies

The original `vcp-storage-spike` package connects already locked `age` 0.11.2
(MIT OR Apache-2.0), `ed25519-dalek` 2.2.0 (BSD-3-Clause), SQLx 0.9.0
(MIT OR Apache-2.0) and the selected Munarium datastore. Patch 0009 records its
workspace/lock entry without changing external package identities. The
deliberately public X25519 test identity in its handoff fixture originates in
`age` 0.11.2 `src/x25519.rs` unit tests; it is not user recovery material.

Independent interoperability uses Go age v1.3.2 at
`b74dce4cdbe35b5e5f66c06d9612b72f89028758` (BSD-3-Clause). It is explicitly
downloaded as a qualification tool and is not committed or shipped. Its release,
archive and executable hashes are in
[the tool pin](src/third_party/components/age-qualification.json).

## Development TOML parser

`src/tests/package-lock.json` pins `@iarna/toml` 2.2.5 from the public npm registry
with its integrity digest. It is an installed development tool for provenance
manifests, not copied engine source. Copyright (c) 2016 Rebecca Turner; ISC license,
retained in the installed package's `LICENSE`. Its public package URL and digest
are recorded in the [development lockfile](src/tests/package-lock.json).
`npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund` reproduces installation.

## Local CPU embedding adapter and reference

The original VCP adapter in `src/crates/vcp-embedding/` uses Cargo dependencies
Candle 0.11.0 (`candle-core`, `candle-nn`, `candle-transformers`) and Tokenizers
0.22.2. Candle's published package source is
[`31f35b147389700ed2a178ee66a91c3cc25cc80d`](https://github.com/huggingface/candle/tree/31f35b147389700ed2a178ee66a91c3cc25cc80d);
Tokenizers' is
[`6573f2c56172bac56f211e77934be3215adef2c2`](https://github.com/huggingface/tokenizers/tree/6573f2c56172bac56f211e77934be3215adef2c2).
Candle declares MIT OR Apache-2.0; VCP uses the Apache-2.0 grant. Distributed
[Candle](src/third_party/licenses/candle-0.11.0-LICENSE) and
[Tokenizers](src/third_party/licenses/tokenizers-0.22.2-LICENSE) terms are retained.
Both license files have SHA-256
`c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4`.

The adapter's API selection and pooling approach were informed by the
[Candle BERT example at ddf1b879dc3a1760cbcb3f3c4a7c6467850cec4a](https://github.com/huggingface/candle/blob/ddf1b879dc3a1760cbcb3f3c4a7c6467850cec4a/candle-examples/examples/bert/main.rs).
VCP adds bounded verified-file loading, fixed CPU selection, typed setup errors,
input limits, truncation reporting and independent reference checks. Upstream
runtime implementation remains an installed Cargo dependency, not copied source.
The [141-package native dependency record](src/third_party/components/embedding-dependencies.json)
retains registry checksums and license declarations. It is not a release notice bundle.

The external model is
[sentence-transformers/all-MiniLM-L6-v2 at 1110a243fdf4706b3f48f1d95db1a4f5529b4d41](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/tree/1110a243fdf4706b3f48f1d95db1a4f5529b4d41).
Its selected model card declares Apache-2.0. The [asset inventory](src/third_party/components/minilm-assets.json)
records exact paths, sizes and hashes, separately from software licensing.
Acquired model files and the model card remain outside the checkout; no weights
are bundled. Four original synthetic texts and generated vectors are retained
in [reference fixtures](src/tests/fixtures/local-embeddings/README.md), with the
independent PyTorch/Transformers generator and hashed reference-tool requirements.

## Remaining dependencies and assets

The original [local corpus qualification executable](src/crates/vcp-memory-spike/Cargo.toml)
calls the retained Munarium governance, in-memory store, datastore and VCP embedding APIs. Its public synthetic
corpus and checks are original VCP material; it does not copy additional upstream
implementation or test bodies. The [214-package dependency record](src/third_party/components/local-memory-dependencies.json)
reuses the selected embedding/Munarium package identities and license declarations.
The fourth Codex patch registers this local package in the shared workspace and
lockfile; the fifth connects its retained governance dependencies. Existing
dependency pins and upstream Rust implementation remain unchanged.

The Codex Cargo lockfile retains dependency identities/checksums for its source
baseline. A release must inventory the actual enabled transitive graph and retain
all applicable notices and corresponding-source obligations; this source record
is not release qualification. The selected Munarium graph includes Tantivy and
DiskANN; release asset packaging remains qualification work. Record exact origin,
license, selected paths and modifications as those components land.

## Gemini behavioral adaptation

The bounded Rust policy/scheduler port in `src/crates/vcp-lifecycle/src/ports.rs`,
its tests, shared Gemini fixture and comparison script adapt behavior from Google
Gemini CLI revision `6a466a7e2fe2b1255752c1e74f69b31f0216084d`, Copyright
2025–2026 Google LLC, Apache-2.0. Source paths, modifications, dependency boundary
and comparison evidence are recorded in [the component record](src/third_party/components/gemini-cli.md).
The [original license](src/third_party/licenses/gemini-cli-6a466a7e-LICENSE) is
retained. Provider SDK and Node runtime source are not included in the Rust port.

The root project license does not replace another component's terms. Names belonging to other projects remain their owners' names; see [TRADEMARK.md](TRADEMARK.md).
