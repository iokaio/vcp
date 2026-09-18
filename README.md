# VCP — Vibe Code Pro

VCP is an open-source experiment in a local coding agent that combines repository-aware coding, model selection through OpenRouter, persistent memory, and visible delegated work. The first planned release is a native Windows command-line application, licensed under Apache 2.0.

**Status: early implementation.** The repository includes a committed Codex foundation, native feasibility adapters and the [P1 canonical host](docs/development/p1-retained-host.md): typed state, durable commands, full capture, two canonical stores, shared accounting and rebuildable history. The [P0 handoff](docs/development/p0-handoff.md) and [P1 qualification](docs/evaluations/p1-completion.md) record their tested boundaries. VCP's installable application is not implemented. Capabilities below describe the intended product; they are not release claims.

## What VCP is intended to do

VCP should help a developer understand a repository, review changes, and implement work that fits the existing architecture. A task should retain its history, account for model costs, explain significant decisions, and recover when the terminal closes.

| Planned capability | Intended behavior |
|---|---|
| Coding and verification | Inspect the workspace, prepare changes, run relevant checks, and report what actually passed |
| Model routing | Select among model groups through OpenRouter using task needs, capabilities, cost, and project policy |
| Budget control | Reserve and account for model spend across the root task, helpers, and delegated work |
| Local memory | Retain versioned claims with evidence, provenance, disputes, and workspace scope |
| Local retrieval | Use Tantivy for lexical search, DiskANN for vector retrieval, and locally executed embeddings |
| Inspectable history | Preserve full observed work artifacts, expose history, notify about aging data, and prune only by user command or saved policy |
| Skills and MCP | Load development skills and broker Model Context Protocol tools through the same authority and accounting boundaries |
| Visible delegation | Show child tasks, isolate their work, and integrate results through one controlling engine |
| Pause and recovery | Pause root and child work with `/pause` while keeping the CLI open, or on close; reconcile effects and deliberately resume |
| Portable state | Transfer history, context, memory, and search state using encrypted snapshots and developer-controlled recovery material |

The complete first-release scope includes these capabilities together. A basic coding loop is an engineering milestone, not the release acceptance boundary.

## Architecture and data boundaries

The foundation is a Codex-derived Rust engine and CLI with maximum reasonable reuse of a working pinned baseline, selected Gemini behavioral adaptations and Munarium memory code. The [source map and setup](docs/development/p0-handoff.md) record exact imports, retained modules, candidate replacements and native qualification. Production integration and release support require the later task gates.

Selected Codex source is copied into `src/third_party/codex/` as ordinary repository files. The [native baseline build and source records](docs/development/codex-source.md) consume those files directly, without fetching Codex or applying patches. [ADR-013](docs/adr/013-upstream-reuse-and-vendoring.md) defines independent reconstruction and upstream maintenance. The [selected Munarium libraries](docs/development/munarium-source.md) share that Cargo workspace and have native test commands. A private [integration host](docs/development/p0-integration.md) qualifies VCP controls around the retained loop; the ordinary upstream CLI is not the VCP product.

One engine owns task state, authorization, the canonical store, and cost accounting. UI clients and adapters do not create competing schedulers or bypass those controls. SQLite is the proposed default canonical store; a files/journal preference is evaluated against the same durability and portability contracts.

The engine, memory, indexes, and embeddings are intended to run on the developer's machine without a hosted VCP backend. Coding requests and model-assisted work use OpenRouter, so selected prompt/context content is sent to the configured model service. Local embeddings do not imply that all model activity is offline. External MCP tools have their own declared access requirements.

Active local code, databases, history, and indexes remain plaintext. Every VCP snapshot object and manifest destined for a cloud sync folder must be encrypted locally before publication. Recovery keys remain under developer control and outside that folder. The [storage prototype](docs/development/portable-storage-spike.md) qualifies bounded encrypted handoff; production formats, activation and key UX remain planned.

See the [architecture](docs/architecture/vcp-what.md) for contracts, invariants, failure behavior, and open decisions.

## Repository layout

The initial roots are:

```text
docs/       Architecture, implementation plan, and project documentation
src/        Product source, test code, fixtures, and packaged assets as implemented
scripts/    Build, test, evaluation, and packaging entry points as implemented
.github/    Contribution templates and delivery CI
```

`src/tests/` contains the delivery harness and contract tests; `scripts/test.ps1` runs them. See the [harness setup](docs/development/delivery-harness.md). The [code layout plan](docs/plan/code-layout.md) defines the future subdirectories, dependency boundaries, upstream provenance locations, and ownership by plan segment. Directories are added when they contain useful work.

## Start here

| Goal | Document |
|---|---|
| Browse the documentation | [Documentation index](docs/README.md) |
| Understand the intended product | [Architecture and requirements](docs/architecture/vcp-what.md) |
| Find the next implementation task | [Plan and execution order](docs/plan/README.md) |
| Place code or scripts | [Code layout](docs/plan/code-layout.md) |
| Understand engineering conventions | [Delivery contract](docs/plan/00-delivery-contract.md) |
| Design tests and acceptance evidence | [Test fixtures and acceptance](docs/plan/16-test-fixtures-and-acceptance.md) |
| Trace dependencies and release coverage | [Work-item ledger](docs/plan/20-traceability.md) |
| Propose or submit a change | [Contributing](CONTRIBUTING.md) |

Research notes are indexed separately under [architecture documentation](docs/architecture/README.md). Model tables and upstream comparisons are research inputs, not a live compatibility or pricing catalog.

## Working with this checkout

To read the design or contribute documentation, Git and a text editor are enough:

```powershell
git clone -c core.longpaths=true https://github.com/iokaio/vcp.git
Set-Location vcp
git switch -c docs/my-change
```

Read the contribution guide, make a focused change, verify its relative links and status claims, and check whitespace:

```powershell
git diff --check
```

With Git, PowerShell 7 and Node.js 24 or later, run `npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund`, then `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`. This checks repository, harness and imported-source contracts. `scripts/build.ps1` builds the selected Codex baseline with the [documented Rust/native prerequisites](docs/development/codex-source.md). Packaging and VCP product runtime suites remain planned.

## Delivery roadmap

1. Qualify upstream reuse, native Windows execution, local search, storage, and encrypted portability.
2. Establish durable engine state, cost accounting, context, tools, and the coding loop.
3. Integrate the CLI, governed memory, retrieval, full history, and portable recovery.
4. Complete routing, optimization, skills, MCP, and visible delegation.
5. Pass integrated owner acceptance and package a native Windows release with notices and provenance.

The public API, TypeScript SDK, VS Code extension, executable hooks, foreign configuration import, and additional operating environments are deferred. Follow the [plan](docs/plan/README.md) for exact task dependencies; this roadmap does not promise dates or completed milestones.

## Contributing and community

Design review, corrections, test scenarios, and implementation contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Contributions use Developer Certificate of Origin sign-offs (`git commit -s`); no contributor license agreement is required. Disclose borrowed material, generated code, and AI assistance, and review everything you submit.

Use [GitHub Issues](https://github.com/iokaio/vcp/issues) for non-sensitive questions, defects, and proposals. Report vulnerabilities through the private routes in [SECURITY.md](SECURITY.md). [SUPPORT.md](SUPPORT.md) explains the current support scope, and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) applies to project participation.

## License and acknowledgements

VCP's original code and documentation are licensed under the [Apache License, Version 2.0](LICENSE). See [NOTICE](NOTICE), [third-party attribution](THIRD_PARTY_NOTICES.md), and the [name and trademark policy](TRADEMARK.md). Third-party material retains its applicable terms.

The project draws on [OpenAI Codex](https://github.com/openai/codex), [Gemini CLI](https://github.com/google-gemini/gemini-cli), and [Munarium](https://github.com/iokaio/munarium). The community-document structure and contribution conventions were adapted from Munarium for VCP's planning stage. The source records identify the actual Codex and Munarium selections and the bounded, attributed Gemini behavioral ports. These acknowledgements do not imply endorsement by their maintainers.
