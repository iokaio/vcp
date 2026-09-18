# Retained-controller lifecycle host

Original Apache-2.0 P0-03 prototype, built in the committed Codex workspace.
It implements thread-scoped admission, local/inherited holds, revision-checked
readmission and owned retained-controller interruption. It is not a runnable
VCP CLI or a durable pause implementation.

See the [implementation and limitations](../../../docs/development/scoped-lifecycle.md)
and run `scripts/build.ps1 -Mode LifecycleTests` from the repository root.
The native suite uses public synthetic fixtures and local scripted providers;
it does not call paid models. Tests live in `tests/controller.rs` and exercise
real retained controllers, alongside the imported core admission regressions.
