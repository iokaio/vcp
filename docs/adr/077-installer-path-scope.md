# ADR-077 — Windows installer PATH and data scope

Date: October 2, 2026. Status: selected for implementation.
Owning item: BETA-06, extending [ADR-072](072-internal-windows-beta.md).

## Decision

Setup asks whether VCP is installed for the current user or all users. The
current-user mode keeps the existing private installation and chosen data root.
The all-users mode requires elevation, installs program files under Program
Files, and adds that directory to the machine PATH. Its installation metadata
records `data_scope=user` instead of a fixed data root. On each launch, the
launcher and native engine resolve that Windows account's LocalAppData known
folder and use its `VCP` child. The installation resolver reports that account's
data directory. Uninstall removes only the PATH entry that setup added and leaves
every account's data untouched.

The shared installer cannot inspect other accounts' private stores during an
upgrade. It requires manifest format compatibility with the previous release;
each account's engine still validates its own state before opening it. Retained
prior releases remain available for a compatible rollback. This mode does not
authorize sharing credentials, history, workspace state or durable memory.
