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

This adaptation does not import Munarium runtime code. Statements in Munarium's original notice about its components, migrations, and enterprise distribution describe that project, not VCP.

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
as a regular copy of `COPYING` for Windows; no code content is modified.
Individual source copyright headers remain intact. No voice DLLs, Microsoft
redistributables, model assets or VCP release package are distributed by this import.

## Development TOML parser

`src/tests/package-lock.json` pins `@iarna/toml` 2.2.5 from the public npm registry
with its integrity digest. It is an installed development tool for provenance
manifests, not copied engine source. Copyright (c) 2016 Rebecca Turner; ISC license,
retained in the installed package's `LICENSE`. Its public package URL and digest
are recorded in the [development lockfile](src/tests/package-lock.json).
`npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund` reproduces installation.

## Remaining dependencies and assets

The Codex Cargo lockfile retains dependency identities/checksums for its source
baseline. A release must inventory the actual enabled transitive graph and retain
all applicable notices and corresponding-source obligations; this source record
is not release qualification. Gemini adaptations, Munarium runtime imports,
Tantivy, DiskANN and embedding assets remain selection work. Record exact origin,
license, selected paths and modifications as those components land.

The root project license does not replace another component's terms. Names belonging to other projects remain their owners' names; see [TRADEMARK.md](TRADEMARK.md).
