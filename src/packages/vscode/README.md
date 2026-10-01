# VCP Coding Agent

**Vibe Code Pro — Plan, code, and review—with you in control.**

Vibe Code Pro is an AI coding agent for your software projects. Plan tasks,
review proposed edits and control execution from your workspace. This extension
connects VS Code **1.138.0** to your separately installed VCP engine on Windows x64.
This is an experimental pre-release;
full release qualification and clean-host support remain incomplete.

## Install

1. Download the matching native installer from [VCP beta downloads](https://downloads.ioka.io/).
   This version requires the signed Ioka LLC native candidate. Check its release
   record and signature; the previously published beta.1 is unsigned. PowerShell 7
   and local NTFS paths are required.
2. Install this Windows x64 pre-release extension, then run **VCP: Open Setup Guide**.
   Installing the extension does not install or update the native engine.
3. Follow the guide to select the installed engine and private data directory in
   User settings. Live provider use requires your credentials and explicit spending caps.

The publisher is **Ioka** (`iokaio`); the extension ID is `iokaio.vcp-local`.
If you installed the earlier `vcp.vcp-local` VSIX, disconnect and uninstall that
extension first, then install this one and reconnect explicitly. This is a new
extension identity, not an automatic update. Keep your native installation,
data directory and User settings; extension-local connection state does not migrate.

VSIX/SDK 0.2.2 pairs with native 0.2.0-beta.2; candidate qualification is in progress.
Use the exact pair and hashes in
the download record; matching version numbers alone do not identify matching builds.
Remote/WSL/container workspaces, ARM64, other operating systems and other VS Code
versions are not qualified. See [compatibility](https://github.com/iokaio/vcp/blob/main/src/packages/vscode/COMPATIBILITY.md)
and [known limitations](https://github.com/iokaio/vcp/blob/main/docs/usage/beta-known-issues.md).

Report reproducible problems through [GitHub issues](https://github.com/iokaio/vcp/issues),
following the [safe reporting instructions](https://github.com/iokaio/vcp/blob/main/docs/usage/beta-known-issues.md#report-a-problem-safely).
Do not include credentials, private histories or raw provider logs.

## Use your workspace

For first use, choose **VCP: Open Setup Guide** or the Workspace view's **Setup guide…** button. The [setup walkthrough](https://github.com/iokaio/vcp/blob/main/src/packages/vscode/SETUP.md) covers installation, the first native task, explicit User settings, credentials and recovery; an offline copy is bundled.

Resolve the installer launcher with `vcp.exe --resolve-installation`, then set `vcp.engineExecutable` and `vcp.dataDirectory` to its reported native executable and data directory in **User settings**. Project and workspace overrides are ignored, including in trusted workspaces. The selected folder must already be initialized by VCP; the extension does not create a workspace or execute a project-defined configuration.

Open the VCP activity-bar view and choose **Connect…**, then select the full folder URI. Identical display names do not share identity. Connections use authenticated local Windows pipes. Observation is the default; **Connect as controller…** explicitly acquires an available lease. Reload restores observation from a non-secret authenticated reference, without acquiring control or resuming work. Restricted mode displays editor trust separately from canonical engine trust and permits controller trust revocation.

The Workspace view shows engine build/path, protocol, execution host, canonical workspace/session/root and binding revision. **Reconcile moved root…** uses the existing native command after the owner closes; it preserves identity/history, resets trust and leaves work paused. Self-launched observer engines retain ownership for 30 seconds after disconnect, so switching to a controller can require waiting and retrying. **Observe existing engine…** accepts its non-secret observer reference without changing another client's ownership.

The Tasks view reads canonical root/child status, objectives, retained commentary/model selection, ledger cost, questions and bounded tool/evidence history. Missing evidence is unavailable. Events invalidate presentation; gaps trigger a new snapshot. Select a task for details and use explicit controller actions for guidance, pause, resume, cancellation or approval. Guidance preserves existing constraints and acceptance criteria. Approval does not resume paused work. Unknown command outcomes retain the original command identity across reload for inspection, without automatic replay.

Remote, virtual, UNC and non-Windows workspaces are unsupported. Nothing stores credentials, task text or controller attachment tickets in workspace/webview state. Pending command storage contains only non-secret identity and outcome metadata, partitioned by the configured engine/data profile. Webview actions select host-registered opaque IDs; engine content is text under a restrictive content security policy. Artifact reads remain authorized and bounded, with content cleared on invalidation.

P4-03 versioned edits are accepted for the qualified Windows/editor envelope. **Start Execution-backed Task** selects a trusted execution profile and collects an ephemeral provider credential; ordinary connections remain inspection hosts. Select that task and a source document, use **Create Editor Draft**, edit its separate untitled copy, and **Review Editor Drafts** for native diffs. Resolve policy questions and use explicit **Resume** when required before **Apply Reviewed Editor Changes**. Each file receives fresh engine admission and a version-bound buffer edit; VCP neither saves automatically nor rolls back earlier successful files after a later conflict.

Source changes invalidate review and cancel eligible undispatched proposals. Use a fresh draft to replan. **Inspect Editor Change Outcomes** queries durable identities without replaying uncertain edits. **Refresh Editor Observations** refreshes tracked metadata and reconciles pending close commands; known close revision conflicts have bounded retries, while unknown outcomes receive only original-command reads. Reload restores observation, discards transient previews and never reapplies an edit. Active buffers remain unverified by disk-only checks until exact actual-close retirement. See [the workflow and qualification limits](https://github.com/iokaio/vcp/blob/main/docs/development/editor-edits.md).

The Inspectors view provides bounded history, memory versions, evidence, cost,
policy, routing, optimization, pruning and encrypted publication views. Every
page and artifact read goes through the engine's current access checks. Hidden
views and reload discard content and previews. Select a synchronized task for
task-scoped queries. Observers can create read-only pruning previews; applying
them requires controller authority. Controller actions require explicit review and retain only
command references for reconciliation; a reload never repeats a mutation.
Encrypted publication concerns the whole workspace and reports local publication
separately from cloud transfer and restore verification. P4-04 native inspector
qualification passed on Files and SQLite; see [its acceptance record](https://github.com/iokaio/vcp/blob/main/docs/development/editor-inspectors.md#p4-04-acceptance).

## Build and qualify

`npm ci` installs the exact locked TypeScript 5.9.3, Node 24.10.1 types and VS Code 1.138.0 types. `npm test` builds the SDK/extension and runs portable bounded-connection/UI/package checks. `npm run stage` creates `artifacts/p4-vscode-extension`, including actual SDK distribution and protocol schema with no development links. No separate runtime npm dependency, provider gateway or engine binary is bundled.

P4-01 actual editor-host qualification passed with the official VS Code 1.138.0 Windows ZIP, an isolated profile in genuine Restricted Mode, the staged package and native engine. It covers moved-root identity, canonical trust revocation and real reload with pending input and an external controller. The native fixture requires `VCP_TEST_CODE` and explicit `--ignored` execution. The development driver uses normal persistent storage; VS Code's extension-test mode uses memory storage. P4-02 qualification is recorded in [the task-view contract](https://github.com/iokaio/vcp/blob/main/docs/development/editor-tasks.md). P4-03 full native engine/editor qualification passed on Files and SQLite, with the staged extension loaded normally from a private extensions directory for dirty-buffer reload. See [installation and exact candidate compatibility](https://github.com/iokaio/vcp/blob/main/src/packages/vscode/COMPATIBILITY.md).

Build the internal beta with `node scripts/package.cjs --engine <absolute-vcp.exe> --engine-manifest <absolute-native-result.json> --output <fresh-artifacts-directory> --reviewed-commit <40-hex-commit> --build-receipt <absolute-original-build-receipt.json>`. The verified production native candidate and clean extension source must share that commit and release identity. Packaging compiles the SDK/extension, verifies native archive and receipt bytes, marks the VSIX as pre-release and retains the exact release identity in its external `manifest.json`. It refuses a version override or existing output. Installed-product qualification remains separate.

For development fixtures, run `npm run build`, then omit both `--reviewed-commit` and `--build-receipt` from the package command. This accepts explicitly unverified or recorded local native builds. Those packages have no beta release identity and cannot replace a beta output directory. Packaging uses pinned official VSCE, independently verifies archive bytes and does not publish anything.
