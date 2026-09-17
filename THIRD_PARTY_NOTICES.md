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

## Future code, dependencies, and assets

Codex is the planned engine/CLI foundation, to be copied and committed under `src/third_party/codex/` as defined in [ADR-013](docs/adr/013-upstream-reuse-and-vendoring.md). The exact revision, selected files, and dependency closure remain unqualified. Gemini CLI adaptations, Munarium runtime components, Tantivy, DiskANN, and embedding assets likewise require selection and qualification. No Codex source, other runtime dependency graph, or model bundle exists in this checkout yet. Design decisions do not count as shipped third-party material.

When importing material, record its exact origin and version, destination files, license identifier, copyright, modifications, and required notices. Preserve full third-party license and notice texts under the planned `src/third_party/licenses/` or alongside the imported source, with an entry in `src/third_party/upstreams.toml`. Add applicable attribution here and in `NOTICE` with the source import, and recheck this inventory against the actual shipped dependency and asset graph before a release. The current documentation clarification adds no Codex attribution entry because it adds no Codex source.

The root project license does not replace another component's terms. Names belonging to other projects remain their owners' names; see [TRADEMARK.md](TRADEMARK.md).
