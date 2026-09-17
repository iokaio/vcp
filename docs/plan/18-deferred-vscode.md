# 18 — Deferred VS Code client

Status: deferred. Owns P4-01 through P4-05. Starts after P8 and P9; file/phase numbering does not place it before CLI memory or routing. Architecture section 18 governs the editor design.

## Code organization

Under `packages/vscode/`, separate extension-host modules `engine_connection`, `workspace_map`, `trust`, `document_context`, `edit_bridge`, `commands` and `diagnostics` from webview presentation `session`, `agents`, `diff`, `inspector` and `questions`. Bind transport to `sdk-ts`; never expose provider or recovery keys to the webview.

Reuse qualified G07 editor-context/diff utilities and inspect Cline/Continue candidates where useful. Engine scheduling, policy, canonical memory and cost remain in Rust. Editor packages must not embed a second model gateway.

## P4-01 — Connection and workspace mapping

Implement launch/attach/version negotiation through the SDK, map VS Code roots to durable VCP identities and enforce workspace trust. Handle reload/disconnect and controlling versus observing clients according to P9's explicit owner contract.

Test multiple roots with identical names, a moved root, untrusted workspace, missing/incompatible engine and extension reload during a pending question. Start with the qualified local Windows host; remote workspace support needs P10-04 environment adapters.

## P4-02 — Task and child views

Display objective, current status, model/group/cost, steering, questions, tool/evidence links and attributed child commentary from the same events as the CLI. Bound webview messages and sanitize external content/links. Question responses carry durable IDs and revision checks.

Test CLI/editor observation of the same trace, dropped cursors, noisy child streams, duplicate clicks, stale question response and malicious output markup. A disconnected UI must not report completion without the engine result.

## P4-03 — Versioned document edits

Capture selected/dirty document content with URI, version, scope and artifact reference. Prepare edits against expected document versions and disk state. Revalidate at apply and record actual application receipts; concurrent typing causes a conflict/replan rather than silent overwrite.

Implement multi-file partial-result handling and undo-aware state reconciliation. Saving a dirty document changes the disk fingerprint and invalidates stale verification. Never treat editor APIs as a cross-file atomic transaction unless their tested contract provides it.

Test unsaved buffer divergence, typing between preview/apply, rename/delete, multi-root files, encoding/CRLF, partial application, save/undo and checks run against stale disk. E05/R02 now includes dirty-editor-buffer cases.

## P4-04 — Inspectors

Expose full history/memory/evidence, cost/policy/routing, optimization proposals and pruning previews through engine query services. Respect current retention/access for historic content; do not cache purged text indefinitely in webview state. Cloud export still uses the encrypted engine publisher.

Test inspector parity with CLI, paged large output, purged artifacts, restricted scope, policy rollback and webview reload. Secret references remain opaque and secret values never enter messages.

## P4-05 — Packaging and compatibility

Package the extension with a documented compatible engine/SDK range and update path. Validate installation without a development checkout, extension restart, engine replacement, incompatible schema and failed upgrade. Existing local state and independently held recovery keys must survive upgrades.

Run actual extension-host integration tests plus real editor interaction for typing/undo/reload cases. Record supported VS Code/Windows versions and not-run environments. Done when the editor preserves established CLI behavior and user buffers under tested races, rather than merely presenting a working chat panel.
