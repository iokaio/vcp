# Portable workspace capture

`CanonicalHost::capture_backup_checkpoint` requires current workspace trust,
read authorization, canonical ownership and quiescent local effects. It does
not resume paused tasks or admit model work. Native repository discovery and
Git observation happen outside the canonical worker. Admission rechecks the
workspace binding, authority/deletion state, controller, owner epoch and source
versions. A second Git observation rejects changes during preparation.

The checkpoint retains source bytes, staged and unstaged diffs, Git status and
index observation as artifact records. Its `vcp-workspace-checkpoint/1` manifest
links every source and Git artifact. All descriptors and the manifest enter one
canonical transaction. Failed preparation may leave unreferenced private spool
objects for existing orphan cleanup; those objects cannot enter a canonical
snapshot merely because their files exist.

Discovery is bounded to 4,096 entries, depth 32, 4 MiB per source and 8 MiB total
source bytes, using the existing generated-file exclusions. Incomplete discovery
or an oversized artifact fails explicitly. This qualified increment does not
claim an arbitrary-size filesystem backup or capture excluded generated trees.

`capture_backup_generation` accepts an opaque pinned publisher view. Component
capture validates the publisher identity, canonical generation, inventory,
lexical manifest and every component checksum while native reader protection
holds. Admission checks current authority and deletion again. Every captured
component records both its generation and direct canonical source references;
retention can follow those references without searching copied text.

Generation capture allows at most 256 lexical components, 4 MiB per component
and 16 MiB total. Snapshot packaging applies its own separately configured
archive limits. Preserved components still require validated reopen on restore;
capturing bytes does not establish search readiness on another engine version.

Dropping either host capture future signals cancellation. The blocking component
worker observes cancellation at bounded component boundaries. Canonical admission
checks cancellation immediately before committing. Cancellation after an admitted
transaction does not undo its retained checkpoint.

## Native evidence

The canonical host regression
`paused_backup_captures_native_dirty_untracked_and_generation_lineage_without_model_work`
passed for Files and SQLite in 3.30 seconds. It creates a real Git repository,
retains staged/unstaged/untracked files, verifies unchanged Git index bytes,
captures a nonempty native lexical generation and checks direct source links.
The task remains paused, no provider attempt exists, and an already cancelled
checkpoint does not advance the canonical watermark. The test uses explicit
local trust and authorization; the untrusted fixture was correctly rejected.

This evidence covers capture and admission. Encryption, interrupted publication,
restore activation and the actual two-Windows OneDrive handoff have separate
acceptance gates; this test does not substitute for them.
