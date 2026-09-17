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

Codex, Gemini CLI, Munarium runtime components, Tantivy, DiskANN, and embedding assets are candidates in the architecture. No runtime dependency graph or model bundle exists in this checkout yet. Their appearance in design documents is not a claim that their code is already shipped.

When importing or distributing material, record its exact origin and version, destination files, license identifier, copyright, modifications, and required notices. Preserve full third-party license and notice texts under the planned `src/third_party/licenses/` or alongside the imported source, with an entry in `src/third_party/upstreams.toml`. Update this inventory from the actual shipped dependency and asset graph before a release.

The root project license does not replace another component's terms. Names belonging to other projects remain their owners' names; see [TRADEMARK.md](TRADEMARK.md).
