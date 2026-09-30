# Orca review: features that fit VCP

Reviewed September 30, 2026 against [Orca's public documentation](https://www.onorca.dev/docs), VCP source `b0d84a2df6e5fa7ed3163a3b1920fcf620623213`, and [00-release-plan.md](00-release-plan.md).

**Recommendation:** borrow Orca's navigation, attention management and code-review workflows. Implement them through VCP's existing engine, public protocol, SDK and VS Code extension. Keep the native Windows beta release plan ahead of new product scope.

This is a feature/design review, not an Orca implementation audit or benchmark. Orca behavior below is what its documentation describes; the product was not installed or tested. VCP proposals and effort assessments are engineering judgments. No Orca code, assets or skills were copied, and no implementation or publication is authorized by this document.

## 1. Architectural fit

Orca describes a worktree-oriented desktop IDE that hosts multiple coding-agent CLIs, review surfaces and browser sessions. VCP already has its own coding engine, provider boundary, governed memory and execution lifecycle. Hosting independent agents as interchangeable terminals would be a different direction; many of Orca's user-facing workflows can instead be built on VCP's existing contracts. [Orca overview](https://www.onorca.dev/docs).

| Preserve in VCP | Consequence for adopting a feature |
| --- | --- |
| Canonical engine ownership and authenticated controller/observer roles | The extension projects state and submits authorized commands. It does not create a second scheduler, effect journal or authoritative task database. See [local attachment](../development/local-attachment.md) and [SDK contract](../development/typescript-sdk.md). |
| Revision-bound authority, durable command identities and uncertain outcomes | New buttons use current capabilities/revisions and reconcile the original command after a lost reply. A timeout is not permission to repeat an effect. See [editor task actions](../development/editor-tasks.md). |
| Explicit pause/resume and owner-loss handling | Restoring a view or focusing a workspace does not resume execution or acquire control. An observer closing must not terminate another controller's engine. See [ADR-016](../adr/016-history-and-pause.md) and [ADR-057](../adr/057-editor-trust-and-observer-recovery.md). |
| Root budgets and qualified model/provider routing | Comparison runs, repair prompts and AI-generated summaries use admitted, accounted VCP work. They are not invisible helper calls or external subscription-agent launches. See [routing](../development/p6-routing.md) and [child ownership](../development/p7-child-workspaces.md). |
| Versioned source/editor observations | Comments, previews and integration plans bind examined revisions. Reviewing a diff does not authorize overwriting a later human edit. See [ADR-059](../adr/059-versioned-editor-edits.md). |
| Governed memory and retention | Notes, issue text and model output remain attributed evidence. They do not become accepted durable memory or higher-priority instructions merely because a UI saves them. See [governed memory](../development/p5-governed-memory.md). |
| Native Windows CLI plus separately installed VSIX | Reuse VS Code's editor, navigation and preview facilities. Do not add an Electron IDE, daemon, remote host or engine auto-updater to the beta. See [release scope](00-release-plan.md#1-scope-and-release-baseline). |

The current code already supplies task/child presentation, history queries, cost and policy inspectors, versioned edit workflows, hooks, skills and isolated child work. The opportunities below are additions to those foundations. Existing P4/P7/P9/P10 work must not be relabeled unimplemented because Orca presents a more integrated interface.

## 2. Prioritized feature inventory

These `ORCA-*` identifiers are review references, not approved implementation work items. **Release fit** means an approach to an existing `BETA-*` item, without adding a new gate. **Next** means an optional increment after the beta baseline. **Later** means a larger additive contract or integration requiring its own work item and acceptance.

Effort is relative: **S** is mostly presentation/docs over existing behavior; **M** adds durable metadata or a bounded API adapter; **L** adds a tool/integration boundary. These are not delivery estimates.

| ID | Candidate | What would be added | Timing / effort |
| --- | --- | --- | --- |
| ORCA-01 | Guided first useful task | One coherent install → setup → task → review walkthrough and readiness presentation | Release fit / S–M |
| ORCA-02 | Attention inbox and notifications | A unified view of questions, approvals, failures, completed work and uncertain outcomes | Next / M |
| ORCA-03 | Workspace and child-work overview | Task/workspace cards with safe navigation, ownership and integration status | Next / M |
| ORCA-04 | Task/workspace Quick Pick | Keyboard navigation across scoped tasks, sessions and pending attention | Next / S–M |
| ORCA-05 | Searchable session history and view restoration | A session browser and structured timeline over canonical history | Next / M |
| ORCA-06 | Batched, anchored diff feedback | Review comments submitted together as revision-bound task guidance | Next / M |
| ORCA-07 | Evidence-linked change attribution | Trace changed ranges to observed tool/editor receipts and child proposals | Next / M |
| ORCA-08 | Compare alternative child solutions | A bounded experiment and comparison view with explicit integration | Later / M–L |
| ORCA-09 | Budget and provider-health summary | Compact known/reserved/unresolved cost and observed provider-limit status | Next / S–M |
| ORCA-10 | Linked issues, reviews and CI checks | Provider-neutral external context and check results beside a task | Later / L |
| ORCA-11 | Manual repair/workflow recipes | Reusable task starters for failed checks, reviews and release notes | Next / M |
| ORCA-12 | Checkpoint notes and handoff cards | Concise attributed progress notes separate from authoritative task status | Next / S–M |
| ORCA-13 | Skill and capability picker | Discover available skills, prerequisites and version-matched help | Next / M |
| ORCA-14 | Effective configuration and hook inspector | Explain selected profiles, imported restrictions and hook results | Next / M |
| ORCA-15 | Browser evidence and visual annotations | Bounded page/element evidence attached to a VCP task | Later / L |
| ORCA-16 | Safe artifact previews | Read-only previews of authorized images, diagrams and reports | Later / M |
| ORCA-17 | Clear diagnostics and support handoff | Build/compatibility/readiness facts with deliberate, sanitized export | Release fit for facts/docs; export later / S–M |

### ORCA-01 — Guided first useful task

**Orca reference:** its [first-session guide](https://www.onorca.dev/docs/first-session) connects repository selection, isolated work, review and delivery in a single walkthrough; [installation](https://www.onorca.dev/docs/install) includes first-launch guidance.

**VCP addition:** apply that coherence to BETA-03, BETA-06, BETA-07 and BETA-10: choose the installed engine/data root, diagnose profile/catalog/credential/tool prerequisites, initialize a workspace, set trust and budget, run one bounded task, inspect its changes, and pause/reopen deliberately. Start with a tested CLI guide and VS Code walkthrough; a new wizard is optional. Do not make three parallel agents the onboarding default.

**Acceptance:** a clean Windows user completes the documented path without a developer checkout or fixture profile. Missing/expired inputs explain the next action. Engine selection remains an explicit User setting; opening a repository grants neither trust nor execution. This directly addresses F02/F03 in the release plan.

### ORCA-02 — Attention inbox and notifications

**Orca reference:** [Notifications & Inbox](https://www.onorca.dev/docs/notifications) and the [Agents feed](https://www.onorca.dev/docs/activity) collect work requiring attention and link back to the relevant session.

**VCP addition:** build an inbox over [task projections and actions](../development/editor-tasks.md), distinguishing pending question, approval needed, failure, paused work, unknown outcome and verified completion. Existing task counts are useful but are not a global inbox. Begin with the connected session/task tree; a later aggregate needs separately authenticated session connections or a new scoped discovery/attachment adapter, even within one workspace. Read/unread state is presentation metadata, not task completion or approval.

**Acceptance:** duplicate/reordered events do not generate duplicate notifications; gaps force a current snapshot; revoked/purged content disappears. Opening a notification only navigates. Use content-minimal Windows notifications by default and never infer completion from terminal silence or a successful transport response.

### ORCA-03 — Workspace and child-work overview

**Orca reference:** [Worktrees](https://www.onorca.dev/docs/model/worktrees) combines project grouping, progress, filtering and task links.

**VCP addition:** extend the existing child presentation with cards for objective, registered workspace, model assignment, owner, state, last durable activity, pending inputs and proposal/integration status. Preserve non-Git support; not every root task needs a new Git worktree. [VCP's child workspace implementation](../development/p7-child-workspaces.md) already preserves dirty bases and supports guarded integration/cleanup.

The editor needs bounded, access-checked public adapters for child/workspace details and actions not currently exposed by the protocol. It must not deserialize private task graphs or treat an arbitrary Git path as an owned child.

**Acceptance:** identical display names remain distinct identities; moved roots and stale registrations are refused; view/filter actions have no effects. Cleanup uses existing ownership, reference and preservation rules and exposes any retained data instead of offering unconditional recursive deletion.

### ORCA-04 — Keyboard-first task/workspace navigation

**Orca reference:** [Quick Open & Jump Palette](https://www.onorca.dev/docs/model/quick-open) emphasizes recent sessions and quick movement between worktrees.

**VCP addition:** a VS Code Quick Pick for known sessions/tasks and pending attention, with deterministic sorting and stable rows while open. Show workspace/root identity and task status so similarly named entries are distinguishable. Reuse VS Code file navigation rather than rebuilding a file index. Begin with the active session or already authorized connections; discovering additional sessions needs the adapter described in ORCA-05. Query only authorized VCP metadata through the SDK.

**Acceptance:** the picker stays bounded with many tasks, cannot open a revoked scope, and never silently creates a workspace, launches an engine, acquires control or resumes work when Enter is pressed. This can ship independently of a full workspace dashboard.

### ORCA-05 — Session browser, structured timeline and view restoration

**Orca reference:** [session history](https://www.onorca.dev/docs/agents/session-history) offers filtering and conversation navigation; [native chat](https://www.onorca.dev/docs/agents/native-chat) presents tool activity and pending interactions; [session restore](https://www.onorca.dev/docs/model/session-restore) restores the workspace presentation.

**VCP addition:** build a paged history browser with task/model/date filters, turn navigation and retained evidence links. The existing session/read/snapshot and history/query methods provide scoped content, but `session/list` currently returns at most the single session visible to the current grant; see [query.rs](../../src/crates/vcp-engine/src/query.rs). A multi-session catalog needs separately authorized session connections or a new bounded discovery/attachment adapter, not merely another UI over that method. Omitting a task from `history/query` also does not search every session. Reopen selected views through authenticated observer recovery. Full-text search beyond existing queries needs a bounded derived index and current access checks. Prefer canonical typed events/artifacts over decoding terminal scrollback or scanning other agents' private directories.

**Acceptance:** reloading restores observation without inference, duplicate commands or credential persistence. Store non-secret references and presentation settings, not a second transcript in webview storage. Retention/revocation invalidate previews. [Session forks](../adr/046-atomic-metadata-session-forks.md) remain metadata operations, not code rollback, resumed execution or inherited budgets.

### ORCA-06 — Batched, anchored diff feedback

**Orca reference:** [Annotate AI Diff](https://www.onorca.dev/docs/review/annotate-ai-diff) collects line/range comments and sends them to an agent together; the [diff viewer](https://www.onorca.dev/docs/review/diff-viewer) supplies the review context.

**VCP addition:** attach review notes to an observed diff identity, path, side, range and base/current content hashes. Submit a reviewed batch as attributed task guidance through the engine, preserving existing objectives, constraints and acceptance criteria. Durable notes need a defined engine artifact/schema; temporary drafts can remain connection-local. This extends [versioned editor review](../development/editor-edits.md), not a second edit executor.

**Acceptance:** changed or ambiguous anchors are marked stale and require re-review. Sending notes is not an approval grant or implicit resume. Duplicate submission/lost replies reconcile one command. Resulting edits still require fresh per-file admission and preserve intervening user edits. This is one of the strongest post-beta additions.

### ORCA-07 — Evidence-linked change attribution

**Orca reference:** [Attribution](https://www.onorca.dev/docs/review/attribution) shows which ranges were observed being written by agents.

**VCP addition:** let a diff row open the task, tool/editor receipt, child proposal and examined revisions that explain a change. Use VCP's existing provenance to distinguish an observed VCP edit, a subsequent edit, and unknown origin. Model identity may be shown only where retained routing evidence supports it. Do not label every unmatched line as human-authored or equate attribution with correctness.

**Acceptance:** undo, manual edits, rebases and imported patches cannot retain falsely precise authorship. Purged or unavailable evidence is labeled as such. Read-only attribution must not alter Git history or add automatic commit trailers. Start with file/hunk evidence links before attempting persistent per-line tracking.

### ORCA-08 — Compare alternative child solutions

**Orca reference:** the [first-session workflow](https://www.onorca.dev/docs/first-session) compares multiple approaches, while [orchestration](https://www.onorca.dev/docs/cli/orchestration) tracks tasks, attempts, workers and decision gates.

**VCP addition:** an explicit “compare approaches” task template creates a small bounded set of VCP children from the same captured dirty base. Show their diffs, findings, test evidence, known/reserved/unresolved costs and limitations side by side. Reuse [P7 delegation/integration](../development/p7-child-workspaces.md), qualified model assignments and root allocation; do not launch foreign agent CLIs or duplicate root budgets.

**Acceptance:** concurrency, deadline and aggregate cost stay within the root envelope. Parent edits invalidate stale integrations. The user/controller explicitly selects a proposal; integration is prepared against current parent state and reverified. Unselected children and uncertain liabilities remain inspectable, with no automatic destructive cleanup. Existing child process-isolation/verification limits must remain visible.

### ORCA-09 — Budget and provider-health summary

**Orca reference:** [usage tracking](https://www.onorca.dev/docs/agents/usage-tracking) makes limits and unavailable data visible in a compact status surface.

**VCP addition:** improve the existing [cost/routing inspectors](../development/editor-inspectors.md) with a root-budget summary and actionable states for exhausted request limits, stale provider evidence, rate limiting and unresolved charges. Use canonical `usage/read` values; never sum children into the root total twice. Rate-limit/reset information must come from an observed provider response or supported adapter, with freshness and unknown status shown.

**Acceptance:** missing prices or unknown liabilities never render as zero cost. The UI does not release reservations, raise caps, retry paid work or switch accounts/models automatically. Provider-specific fields stay behind the provider boundary. This is primarily presentation over existing accounting, not a replacement billing system.

### ORCA-10 — Linked issues, reviews and CI checks

**Orca reference:** [hosted reviews, issues and Actions](https://www.onorca.dev/docs/review/github) puts external work and check status beside the worktree.

**VCP addition:** attach an issue/review URL and immutable retrieved context to a canonical task, then show bounded read-only review/check status. Begin with one service through an explicit governed MCP/process adapter; keep a provider-neutral task-link representation. A later “address failed checks” action creates normal VCP work with explicit scope and budget. Reuse VS Code source-control facilities for ordinary Git presentation.

**Acceptance:** external text/logs remain untrusted and cannot authorize tools, merge or deployment. Credentials stay in the approved credential path. Responses are paged, rate-limited and freshness-labeled; failures remain distinguishable from “no checks.” Publishing comments, pushing and merging are separately governed effects, never consequences of selecting a UI row.

### ORCA-11 — Manual repair and workflow recipes

**Orca reference:** [source-control actions](https://www.onorca.dev/docs/review/commit-push) include repair prompts for failed hooks and configurable action recipes.

**VCP addition:** offer “explain failed check,” “review this change,” “draft release notes,” and “prepare a fix” as explicit task starters using existing skills and bounded evidence attachments. A recipe selects objective, acceptance checks and allowed inputs; it supplies no additional authority. Reuse the engine's qualified profile and tools. Repository recipes are suggestions until adopted through trusted user configuration.

**Acceptance:** preview the proposed task, relevant artifacts and budget before dispatch; preserve existing session authorization without repeated unnecessary approvals. A failed check never produces a recipe that disables the check. Generated commit/PR text remains a draft until the corresponding external action is authorized. No cron runner or hidden helper model call is needed.

### ORCA-12 — Checkpoint notes and handoff cards

**Orca reference:** [Worktree checkpoints](https://www.onorca.dev/docs/cli/worktree-checkpoints) documents a short status comment and optional phase label. It does **not** describe filesystem snapshots or transactional rollback.

**VCP addition:** expose a concise “last result / next step / blocker / evidence” card based on retained commentary, with optional user-authored notes. Separate editable user intent from agent progress; use revision-checked updates if notes become durable. Link each factual claim to its source when available and keep historical notes labeled with their examined revision.

**Acceptance:** an agent cannot overwrite user constraints or mark canonical work complete by changing a note. Stale notes remain distinguishable from current state. Saving a summary does not automatically admit it as Munarium memory. Do not turn this feature into code rollback or a session-fork shortcut.

### ORCA-13 — Skill/capability picker and version-matched help

**Orca reference:** [skills registry and MCP](https://www.onorca.dev/docs/cli/skills) describes discoverable skills and guides matched to the running CLI; [native chat](https://www.onorca.dev/docs/agents/native-chat) exposes a skill picker.

**VCP addition:** present the existing skill catalog with description, origin/version, activation state, available resources and missing prerequisites. Load detailed help on demand from installed package resources through the engine and actual negotiated capabilities. Reuse VCP's descriptors, resource roles and [completed skills work](../research/skills-upgrade-plan.md); do not add a competing skill format or imply discovery/reference reading is missing. The public method registry does not currently expose skill catalog/activation/resource-read APIs; define additive, access-checked SDK methods before wiring an editor picker, rather than reading engine files directly or using a private control protocol.

**Acceptance:** picker entries match installed hashes and supported runtime features. Selection makes activation intent explicit, without granting tools. Unsupported commands are not offered. Skill updates continue through reviewed, pinned packaging with attribution; no background `npx` installer or automatic rewrite of user/global skill directories is introduced.

### ORCA-14 — Effective configuration and hook inspector

**Orca reference:** [hooks and memory](https://www.onorca.dev/docs/agents/hooks-memory) exposes agent configuration and lifecycle hooks.

**VCP addition:** show a sanitized projection of effective configuration, selected imported restrictions, hook order and last governed receipts. Make a narrower MCP allowlist or deadline visible and explain which source supplied it. Build on [configuration imports](../development/configuration-imports.md) and [existing hooks](../development/hooks.md), not automatic execution of checked-in configuration. Configuration/hook inspection and import preview/apply/rollback public adapters are additional work. Define an explicit field allowlist; exclude raw profiles, process environments, credential references/values and unrestricted paths.

**Acceptance:** complete release-plan **BETA-02** first so CLI and editor execution use identical effective restrictions. Preview/apply/rollback stays revision-bound. Hooks use existing broker authorization, limits and uncertain-effect reconciliation; merely opening a repository or inspector runs none. Show current limits: authorization/rewrite hooks support native file/process requests; configured authorization hooks make MCP/verification wrappers reject rather than bypass the gate. Executable MCP completion hooks can be blocked by the active broker and pause the task. Editing `AGENTS.md` or a hook file does not bypass durable-memory admission or confer authority.

### ORCA-15 — Browser evidence and visual annotations

**Orca reference:** [Design Mode](https://www.onorca.dev/docs/browser/design-mode) captures element context, styles and screenshots; the [worktree browser](https://www.onorca.dev/docs/browser/overview) keeps page context associated with work.

**VCP addition:** extend the existing webapp-testing workflow with a deliberately selected page/element evidence bundle: URL/origin, capture time, viewport, bounded DOM/style excerpt, screenshot digest and optional source-map location. Attach it to a task through governed artifacts/MCP/tools. Reuse a supported external browser/helper or qualified VS Code integration; embedding an entire Chromium IDE is unnecessary. The current skill is a foundation, not proof that an interactive picker already exists.

**Acceptance:** explicit origins and browser ownership, no imported everyday browser cookies, no cross-workspace credential reuse, bounded/redacted capture, untrusted DOM handling, and stale-evidence labels. Source-map locations are hints until independently validated. A screenshot or changed page is not proof that the code is correct. This needs a separate tool/security contract and packaging qualification after beta.

### ORCA-16 — Safe artifact previews

**Orca reference:** [format viewers](https://www.onorca.dev/docs/editing/viewers) make diagrams, images and reports accessible beside the code.

**VCP addition:** add read-only previews for authorized task artifacts, starting with images and plain Markdown, then selected diagram/report formats. Reuse VS Code facilities where they preserve the artifact access boundary. The [current inspectors](../development/editor-inspectors.md) already provide bounded authorized artifact reads; richer rendering is the addition. Prefer file/version comparisons with explicit identity over arbitrary file-path loading.

**Acceptance:** strict size/type/resource bounds, restrictive CSP, no script execution or implicit external requests, and current access checks on every read. Clear previews on disconnect, scope change, revocation or purge. A safe local preview is not authorization to publish a public artifact URL; notebook execution and active HTML remain separate tool actions.

### ORCA-17 — Diagnostics and deliberate support handoff

**Orca reference:** [troubleshooting](https://www.onorca.dev/docs/troubleshooting) links setup failures to concrete next steps; [privacy documentation](https://www.onorca.dev/docs/telemetry) distinguishes product telemetry from voluntarily shared diagnostic detail.

**VCP addition:** use BETA-03/BETA-10 to expose build/pair identity, connection role, profile/catalog readiness, model/helper prerequisites and specific recovery actions. Start with documented diagnostics and a user-reviewed copy of non-secret facts. A later diagnostic export command can assemble an allowlisted bundle rather than asking for full logs or histories.

**Acceptance:** no prompts, credentials, recovery keys, raw environment, sensitive paths or active plaintext stores leak into support output. Export is explicit and previewable; upload is a separate action. This does not propose cloud analytics. A new comprehensive export command is optional post-beta work, not an extra release blocker added to `00-release-plan.md`.

## 3. Behaviors to adapt or leave outside this plan

These features may be useful in Orca, but their documented mechanisms do not transfer directly to VCP's present release envelope.

| Orca mechanism | VCP disposition |
| --- | --- |
| External agent CLIs and subscription-account selection ([overview](https://www.onorca.dev/docs), [accounts](https://www.onorca.dev/docs/agents/codex-hot-swap)) | Do not replace VCP's engine/provider routing with a multi-CLI terminal host. Any future provider/account support belongs behind the provider boundary with current pricing, authority and accounting. Orca's account page itself says already-running sessions keep their account until restart; do not describe this as live migration of an in-flight VCP request. |
| Daemon-preserved agent processes and foreground-triggered restart ([restore](https://www.onorca.dev/docs/model/session-restore), [hibernation](https://www.onorca.dev/docs/agents/hibernation)) | Adopt view restoration only. Preserve explicit resume, scoped ownership and owner-loss behavior. Focus/open/reload must not silently admit work. No background service in this beta. |
| Cron/RRULE automations ([automations](https://www.onorca.dev/docs/cli/automations)) | Defer. VCP's current effect scheduling is not an unattended job scheduler. Later support needs explicit durable schedule authorization, occurrence IDs, overlap/missed-run policy, bounded aggregate spending, credential availability and owner lifecycle. Start with manually triggered recipes. |
| SSH, remote servers, mobile and cloud workspaces ([deployment modes](https://www.onorca.dev/docs/ways-to-run)) | Outside the Windows/local beta. These require new authentication, transport, host qualification, secret/data-placement and possibly infrastructure-budget decisions. Do not expose the local named-pipe contract over an improvised network tunnel. |
| Shared ignored paths, copied `.env` files and automatic setup hooks ([worktrees](https://www.onorca.dev/docs/model/worktrees), [hooks](https://www.onorca.dev/docs/agents/hooks-memory)) | Preserve VCP's captured dirty base, explicit inputs and path/isolation rules. Do not import secrets or link writable caches across children by default. Repository content may suggest setup; only trusted, governed configuration can authorize it. |
| Cookie-importing browser profiles and broad desktop control ([browser profiles](https://www.onorca.dev/docs/browser/profiles), [computer use](https://www.onorca.dev/docs/cli/computer-use)) | Begin with isolated browser evidence for an explicit task. General desktop accessibility control needs a new tool/permission contract and Windows qualification; browser-profile isolation alone does not grant authority to reuse a user's logged-in sessions. |
| Automatic updates and public artifact sharing ([install](https://www.onorca.dev/docs/install), [browser artifacts](https://www.onorca.dev/docs/browser/overview)) | Preserve explicit compatible upgrades and separately authorized publication. Neither a model response nor a preview should upload code/history. No engine auto-updater in the current beta. |
| Product telemetry ([privacy](https://www.onorca.dev/docs/telemetry)) | Adopt clear privacy/support documentation, not an analytics dependency. Optional future telemetry would need its own justified data contract and consent design. No external logging is needed for this feature review. |

VS Code already supplies file exploration, split editors, terminal panes, text editing and common navigation. Those are opportunities to integrate, not reasons to build a second IDE. Custom sounds, emoji naming and broad theme parity are lower-value than release readiness, attention handling and review quality.

## 4. Recommended adoption sequence

1. **Finish the beta foundation.** Use ORCA-01 and the facts/docs portion of ORCA-17 to improve existing BETA-03/06/07/10 deliverables. Resolve BETA-02's imported-configuration parity defect before adding new execution controls. Leave the release plan's provenance, installer and exact-artifact qualification gates intact.
2. **First post-beta usability increment:** ORCA-04 quick navigation and ORCA-02 attention handling. Add ORCA-12 handoff cards if they can reuse retained commentary without a new durable-note schema. These should improve ordinary use without additional model spend.
3. **Review and visibility:** ORCA-06 batched feedback, ORCA-05 session navigation, ORCA-09 budget status and ORCA-07 provenance. Persist only the metadata each feature needs and extend protocol capabilities explicitly.
4. **Workflow management:** ORCA-03 workspace/child overview, ORCA-11 manual recipes, ORCA-13 skill/help picker and ORCA-14 effective configuration. Add missing public adapters before UI actions; never bypass the engine by reading its private store.
5. **Separate larger proposals:** ORCA-08 alternative-solution comparison, ORCA-10 external issue/CI integration, ORCA-15 browser evidence and ORCA-16 richer previews. Each needs a scoped work item, additive schemas/tools where necessary, license/dependency review and targeted native acceptance.

No feature in this review is automatically added to beta acceptance. If selected before the candidate freeze, update the owning implementation/release item and its tests explicitly. Any later shipped-code or packaged-doc change creates new artifact hashes and requires affected qualification, as `00-release-plan.md` already specifies.

## 5. Common acceptance and review limits

For each selected feature, require the smallest meaningful checks at its boundary:

- UI projections: bounded pagination/rendering, event-gap recovery, explicit stale/disconnected states, keyboard accessibility and no effects from navigation.
- Mutations: current controller/trust/revision checks, durable command identity, lost-reply reconciliation and no automatic uncertain-command replay.
- Content: hostile Markdown/DOM/issue text, safe artifact reads, current access/retention checks and no credential persistence in webview/workspace state.
- Child work: dirty-base preservation, root budget/concurrency/deadline limits, visible results/liabilities, current-parent integration and safe cleanup.
- Release: actual installed Windows/VSIX behavior for the selected change, both stores where canonical state changes, and evidence bound to final bytes.

Documentation reviewed covers Orca onboarding/install, worktrees/navigation, session history/restore/chat, notifications/feed, diffs/annotations/attribution, source-control/review integrations, usage, hooks, orchestration, skills, browser/design/profiles, viewers, automation, computer use, deployment modes, privacy and troubleshooting. Links beside each feature identify the supporting pages. Orca docs are mutable and include experimental surfaces; this review does not certify their implementation, security, performance or licensing.

VCP comparison used the release plan, current protocol/SDK/editor surfaces and targeted architecture/development records. New opportunities are distinguished from existing mechanisms and missing public adapters. Historical ADR status text is not used to reopen work completed in the current ledger. Review checks confirmed 17 unique feature headings, existing local link targets and no diff whitespace errors. The release plan's SHA-256 remained unchanged. No application builds, model calls or runtime feature tests were run for this documentation-only comparison. Any future code port must inspect a pinned source revision and applicable licenses separately.
