# Contributing to VCP

Contributions to the design, documentation, test scenarios, and implementation are welcome. VCP is currently in planning; read the [project status](README.md) and [implementation plan](docs/plan/README.md) before proposing runtime changes.

AI agents working in this repository must also follow [AGENTS.md](AGENTS.md) or its identical [CLAUDE.md](CLAUDE.md) counterpart. Keep both guidance files synchronized when updating project instructions.

## Choose and scope a change

Use [Issues](https://github.com/iokaio/vcp/issues) for non-sensitive defects, questions, and proposals. Search existing issues first. For implementation work, identify the task ID, completed dependencies, and acceptance criteria in the [traceability ledger](docs/plan/20-traceability.md). Discuss changes to confirmed product requirements or major source boundaries before investing in a large implementation. Small corrections can go straight to a pull request.

Follow the [code layout](docs/plan/code-layout.md): documentation in `docs/`, source and test assets in `src/`, and automation in `scripts/`. Preserve cohesive selected upstream modules. A directory diagram is not a requirement to create empty replacement crates.

## License and sign-off

Submit contributions under the [Apache License, Version 2.0](LICENSE). No contributor license agreement is required. You retain your copyright; you must have the right to submit all included material under its applicable terms.

Every commit submitted for inclusion must carry a [Developer Certificate of Origin 1.1](https://developercertificate.org/) sign-off. Read the certificate before signing. Git can add the trailer:

```powershell
git commit -s -m "Describe the change"
```

This adds `Signed-off-by: Your Name <you@example.com>` using your configured Git identity. It is a contribution certification, separate from cryptographic commit signing. Sign off only for yourself. Maintainers review sign-offs before merging; this repository does not currently provide an automated DCO check.

New original source and script files should carry `SPDX-License-Identifier: Apache-2.0` in a comment near the top, after a shebang or required declaration. Do not overwrite a borrowed file's license or copyright. Root documentation is covered by the project license unless a file states otherwise.

## Attribution and disclosure

The pull request template asks about four kinds of provenance. Answer each, using "none" when applicable:

- Borrowed code, tests, documentation, or assets: identify the source, exact revision, license, and affected paths.
- Generated material: name its generator, inputs, and reproduction command.
- AI assistance: identify the tools used and confirm that you reviewed and validated their output. Do not include private prompts or credentials.
- Employer or contractual restrictions: confirm that any required permission to contribute has been obtained, without disclosing confidential agreements.

For a selected source import or attributed port, update the planned `src/third_party/upstreams.toml` record when that manifest is introduced. Retain original notices and license texts, mark modifications in changed upstream files, and add relevant attribution to [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [NOTICE](NOTICE). Include fixture and model-asset provenance as well as production dependencies. A link to a repository alone is not an immutable source record.

Codex imports follow [ADR-013](docs/adr/013-upstream-reuse-and-vendoring.md): copied source under `src/third_party/codex/` is committed with reviewed patches already applied. Review changes to that tree together with the ordered patch series, immutable source-selection/hash records, and notices. Prove that an isolated reconstruction matches the committed result. Do not introduce a submodule/subtree workflow or make normal builds fetch Codex and apply patches. This is planned import work; no Codex source exists in the current checkout.

## Development workflow

1. Fork the repository and create a topic branch from `main`, or use a topic branch in the repository if you have access.
2. Read the relevant architecture contracts, plan segment, and [delivery conventions](docs/plan/00-delivery-contract.md).
3. Make one reviewable behavioral or documentation change. Preserve unrelated work, user state, and upstream attribution.
4. Update documentation alongside behavior, including layout, task ownership, and test mappings when affected.
5. Run the checks that exist for the change and record the exact commands, outcomes, and missing prerequisites.
6. Submit a pull request against `main` using the template. Explain the problem, resulting behavior, and evidence. A maintainer reviews before merge.

Changes to licensing, community policy, `.github/`, signing, and release configuration need explicit maintainer review in the pull request. This is a review responsibility, not a prohibition on proposing improvements.

## Validation at the current stage

For documentation and community-file changes:

- Check all added or changed relative links and navigation entries.
- Keep intended behavior, deferred work, and verified implementation clearly distinguished.
- Update affected plan references together. If dependencies change, check unique task ownership, valid dependencies, absence of cycles, and first-release coverage.
- Run `git diff --check` and inspect the complete diff, including new files.

There is no Cargo workspace, build runner, runtime suite, or CI workflow yet. Do not report those checks as passing. The proposed `scripts/test.ps1` commands in the plan become contributor requirements only when implemented and documented.

As source lands, run focused tests for changed behavior, shared contracts for affected backends, and native Windows checks where real process/filesystem behavior matters. Preserve relevant upstream tests and explain intentional differences. A mock is not evidence of OS enforcement. See [test fixtures and acceptance](docs/plan/16-test-fixtures-and-acceptance.md) for the test contracts.

Routine deterministic checks should require no private credentials or paid calls. Live OpenRouter evaluations require an explicit configured budget. Keep full run artifacts in ignored local locations; commit only synthetic fixtures and reviewed, redacted summaries. Never include real workspace history, provider credentials, or recovery keys in a public report.

## Review and conduct

Be specific about limitations and unfinished work. Do not mark a task complete because its directories or interfaces exist; completion requires its acceptance evidence and dependencies. Respond to review by explaining the result and updating the change when needed.

[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) applies to issues, reviews, and other project spaces. Questions about usage or support belong in [SUPPORT.md](SUPPORT.md)'s public venues. Suspected vulnerabilities use the private channels in [SECURITY.md](SECURITY.md).

## Reference

Adapted for VCP from [Munarium's contribution guidance](https://github.com/iokaio/munarium/blob/8da666067000ca1ee9c131bc67e70b978862faa3/CONTRIBUTING.md). The adaptation replaces Munarium's component gates and release assumptions with VCP's current plan and directory conventions. Attribution is recorded in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
