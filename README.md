# VCP — Vibe Code Pro

VCP is an open-source local coding agent that combines repository-aware coding, model selection through OpenRouter, persistent memory, and visible delegated work. It includes a native Windows CLI, a TypeScript SDK and a VS Code extension, licensed under Apache 2.0.

**Status: unsigned beta downloads available for manual testing.** Download the Windows x64 installer, portable ZIP and matching VSIX from [GitHub Releases](https://github.com/iokaio/vcp/releases). Full qualification remains incomplete; this is not a stable-release or clean-host support claim. See the [publication record](docs/development/beta-publication.md), [release plan](docs/release-planning/00-release-plan.md) and [implementation status](docs/release-planning/02-implementation-status.md) for exact artifacts, actual checks and remaining gates.

## What VCP does

VCP supports repository analysis, change review and implementation through one engine that owns task history, model accounting, authorization and recovery. The beta qualification matrix checks these integrated behaviors against the final installed artifacts.

| Capability | Behavior |
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

The first-release scope includes these capabilities together. Implementation and historical qualification records do not replace current beta acceptance.

## Architecture and data boundaries

The foundation is a Codex-derived Rust engine and CLI, selected Gemini behavioral adaptations and Munarium memory code. The [source map and setup](docs/development/p0-handoff.md) record the initial imports and qualification. Current source inventories and release receipts bind the selected implementations to each candidate.

Selected Codex source is copied into `src/third_party/codex/` as ordinary repository files. The [native baseline build and source records](docs/development/codex-source.md) consume those files directly, without fetching Codex or applying patches. [ADR-013](docs/adr/013-upstream-reuse-and-vendoring.md) defines independent reconstruction and upstream maintenance. The [selected Munarium libraries](docs/development/munarium-source.md) share that Cargo workspace and have native test commands. A private [integration host](docs/development/p0-integration.md) qualifies VCP controls around the retained loop; the ordinary upstream CLI is not the VCP product.

One engine owns task state, authorization, the canonical store, and cost accounting. UI clients and adapters do not create competing schedulers or bypass those controls. SQLite and Files canonical stores follow the same durability and portability contracts and both require beta qualification.

The engine, memory, indexes, and embeddings run on the developer's machine without a hosted VCP backend. Coding requests and model-assisted work use OpenRouter, so selected prompt/context content is sent to the configured model service. Local embeddings do not imply that all model activity is offline. External MCP tools have their own declared access requirements.

Active local code, databases, history, and indexes remain plaintext. VCP encrypts snapshot objects and manifests locally before publishing to a cloud sync folder. Recovery keys remain under developer control and outside that folder. Follow the [recovery instructions](docs/usage/beta-recovery.md); independent-machine recovery remains a current beta qualification gate.

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
| Prepare an internal beta installation | [Installation and prerequisites](docs/usage/beta-installation.md) |
| Set up a provider and run a task | [First task and metadata renewal](docs/usage/beta-onboarding.md) |
| Check support limits and report a problem | [Known issues and safe support](docs/usage/beta-known-issues.md) |
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

With Git, PowerShell 7 and Node.js 24 or later, run `npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund`, then `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`. This checks repository, harness and imported-source contracts. `scripts/build.ps1` builds the selected Codex baseline with the [documented Rust/native prerequisites](docs/development/codex-source.md). VCP's [production build and distribution guide](docs/development/p8-distribution.md) describes the separate product build, package receipts and native qualification runners. A fast-suite pass does not qualify an installed beta.

## Delivery roadmap

1. Qualify upstream reuse, native Windows execution, local search, storage, and encrypted portability.
2. Establish durable engine state, cost accounting, context, tools, and the coding loop.
3. Integrate the CLI, governed memory, retrieval, full history, and portable recovery.
4. Complete routing, optimization, skills, MCP, and visible delegation.
5. Pass integrated owner acceptance and package a native Windows release with notices and provenance.

The public API, TypeScript SDK, VS Code extension, governed executable hooks and foreign configuration import are implemented. The [internal beta plan](docs/release-planning/00-release-plan.md) owns current packaging and acceptance work. Non-Windows and remote workspaces remain outside the selected beta scope.

## Contributing and community

Design review, corrections, test scenarios, and implementation contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Contributions use Developer Certificate of Origin sign-offs (`git commit -s`); no contributor license agreement is required. Disclose borrowed material, generated code, and AI assistance, and review everything you submit.

Join the [Ioka Discord server](https://discord.gg/YdDsb8Eeb) to discuss VCP, share ideas, and connect with the community.

Use [GitHub Issues](https://github.com/iokaio/vcp/issues) for non-sensitive questions, defects, and proposals. Report vulnerabilities through the private routes in [SECURITY.md](SECURITY.md). [SUPPORT.md](SUPPORT.md) explains the current support scope, and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) applies to project participation.

## License and acknowledgements

VCP's original code and documentation are licensed under the [Apache License, Version 2.0](LICENSE). See [NOTICE](NOTICE), [third-party attribution](THIRD_PARTY_NOTICES.md), and the [name and trademark policy](TRADEMARK.md). Third-party material retains its applicable terms.

The project draws on [OpenAI Codex](https://github.com/openai/codex), [Gemini CLI](https://github.com/google-gemini/gemini-cli), [Munarium](https://github.com/iokaio/munarium), and [Anthropic Skills](https://github.com/anthropics/skills). The community-document structure and contribution conventions were adapted from Munarium for VCP's planning stage. The source records identify the actual Codex and Munarium selections and the bounded, attributed Gemini behavioral ports. The [skills plan](docs/research/skillsplan-new.md) records the eight workflow additions and per-skill license review; GPL, LGPL, restrictive and unlicensed skill material is excluded. These acknowledgements do not imply endorsement by their maintainers.
