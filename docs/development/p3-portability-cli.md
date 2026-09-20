# P3-06 portability CLI

This increment implements independent key enrollment, public configuration and
status, and explicit local storage selection. Snapshot creation and restored
workspace activation remain separate integration steps until their native
qualification is complete.

`vcp backup keys --workspace-id ID create --recovery-dir DIRECTORY` creates an
independent recovery copy and verifies it by rereading it. Import on a fresh
host requires `import --key FILE --lineage HASH --checkpoint FILE`; the checkpoint
contains the independently obtained sequence, deletion epoch and parent.
Neither workspace identity nor writer enrollment is learned authoritatively
from an archive. Existing selected workspaces can omit `--workspace-id`.

`verify --key FILE` verifies the selected enrolled key. `rotate --recovery-dir
DIRECTORY --expected-revision N` creates and verifies a new independent copy
before changing public enrollment. Old copies are retained for old snapshots.
Recovery directories must be outside workspace, canonical, trust, and declared
or detected sync roots. Commands accept paths, never literal secret keys, and
results contain public references only. Each process must explicitly load its
signing capability; public setup is not a secret cache.

`vcp backup configure --vault DIRECTORY --staging DIRECTORY --manual-only`
stores a revisioned setup under independent local trust. Subsequent updates
require `--expected-revision N`. The alternative `--automatic` records requested
policy; automatic lifecycle execution is not claimed by this partial increment.
Vault and staging directories must already exist. `backup status` reports local
publication and uncovered canonical sequence ranges independently from cloud
transfer and restore verification, which are not inferred from local copies.

`vcp storage configure --backend files --preview` reports the future-workspace
preference. Apply omits `--preview` and uses the displayed expected revision for
an existing preference. It does not change a running workspace backend.

`vcp storage migrate --backend sqlite --preview` returns an operation UUID and
exact descriptor digest. Apply repeats `--operation UUID --expected-descriptor
DIGEST` without `--preview`. The source remains retained; a converted candidate
must independently reopen with identical canonical state before selection.
Exact retries reconcile an already activated operation. A partially materialized
candidate is never deleted automatically; an invalid candidate remains visible
as a failed operation. Close all workspace owners and inspectors before apply.
No free-space reservation or power-loss qualification is claimed.

`vcp workspace rebind WORKSPACE_ID` is the structured alias for existing rebind.
It also holds the selection lease and does not silently grant execution rights.
