# Retained-controller lifecycle host

The [P0 integration guide](../../../docs/development/p0-integration.md) documents the bounded coding host, atomic synthetic accounting, governed memory and neutral Gemini adapters. Use `scripts/test-integration.ps1` locally; these are qualification APIs, not production formats.

Original Apache-2.0 P0-03 prototype, built in the committed Codex workspace.
It implements scoped admission, local/inherited holds, durable private checkpoints,
dispatch receipts, revision-checked control commands and native process ownership.
The synthetic owner example qualifies pause/reopen through retained controllers;
it is not the product VCP CLI or the selected production store.

See the [recovery implementation and limitations](../../../docs/development/lifecycle-recovery.md)
and run `scripts/build.ps1 -Mode RecoveryTests` and `-Mode LifecycleTests` from the repository root.
The native suite uses public synthetic fixtures and local scripted providers;
it does not call paid models. Tests live in `tests/controller.rs` and exercise
real retained controllers, alongside the imported core admission regressions.
