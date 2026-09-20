# ADR 023: Canonical selection leases and descriptor activation

Status: Accepted for P3-06 / P5-10 implementation.

Existing CLI workspaces use a direct canonical Store, including its retention
replay-root chain. Opening the separate ActiveRoot layout over that directory
would create a different empty layout rather than migrate retained history.

Canonical-root selection therefore uses the local registry descriptor. Every
CLI owner, inspector and rebind holds a workspace selection lease throughout
selection and canonical use. Activation holds an exclusive lease and compares
the exact previous descriptor digest. Windows handles reject reparse points and
deny deletion of the lock and pinned directory ancestors.

Version 1 descriptors retain their exact `entry/canonical` location. Version 2
may select only `entry/canonical-roots/<operation UUID>`. An operation records
its intent before preparing a candidate; arbitrary archive-provided locations
never become canonical roots. Conversion independently reopens and compares
the candidate state while retaining both source and destination owners before
publishing the descriptor. Restore must likewise retain its independently
enrolled trust owner and the opaque validated imported Store through activation.

Previous roots remain retained recovery material. Descriptor activation neither
resumes tasks nor grants execution authority. It does not mutate retention's
internal replay-root chain. Search derivatives require separate compatibility
validation or rebuild. The implementation qualifies process interruption and
Windows file-sharing behavior; it does not claim power-loss guarantees beyond
the underlying filesystem and platform flush behavior.
