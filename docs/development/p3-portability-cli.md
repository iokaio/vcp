# P3-06 portability CLI

The portability controls keep independent key enrollment, local publication,
cloud transfer and restored execution authority distinct. Native qualification
and the external two-machine/cloud acceptance are recorded separately; a local
success never proves cloud transfer.

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
policy for completion and controlled pause/exit hooks.
Vault and staging directories must already exist. `backup status` reports local
publication and uncovered canonical sequence ranges independently from cloud
transfer and restore verification, which are not inferred from local copies.

`vcp backup create --key FILE --git ABSOLUTE_EXECUTABLE` explicitly loads a
verified signing capability and starts a native snapshot without a model call.
Use `--operation UUID --retry` to continue the same durable cut. A live owner
returns preparation progress immediately; standalone maintenance waits for local
publication. `backup cancel --operation UUID` requests cancellation of the live
owner's matching operation. Failed or reopened pre-admission jobs can also be
cancelled using independent public enrollment and configured staging, without
loading a signing key. This releases local source pins after copying has stopped;
it does not delete vault copies. Admitted copies remain reconciliation-required,
and a published job requires the independently accepted checkpoint before its
remaining pins can be released. Inspect status for retained cleanup or copy obligations.
If configured staging has changed, select the job's original staging directory
or reconcile the retained obligation; a missing file in a different directory
does not prove cleanup of the original job.

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

`vcp --workspace NEW_DESTINATION restore --workspace-id ID --source OBJECT
--key FILE --staging DIRECTORY --preview` authenticates the signed archive using
independent enrollment and reports its operation UUID, ciphertext digest/length,
current descriptor digest, destination collision, retained old root, and separate
volume-capacity observations. Apply repeats those values with `--operation UUID
--ciphertext-sha256 HASH --bytes N` and, for an existing registry entry,
`--expected-descriptor HASH`, omitting `--preview`. `--backend files|sqlite`
selects the new canonical backend. The destination must be new; divergent work
is never overwritten. Native paths and ciphertext are checked again on apply.

Restore verifies and selects the sanitized canonical candidate before ordinary
rebind. An interrupted activation remains explicitly `rebind_pending`, which
blocks normal startup. Exact retries reconcile the recorded operation without
reimporting a root that has subsequently received legitimate writes. The old
canonical root remains retained. Source files are materialized; Git index and
diffs remain historical evidence, and executable Git metadata is not recreated.
Local search rebuild reports canonical, lexical and semantic readiness separately.
It can rebuild semantic search from compatible retained vectors when they cover
the freshly authorized source inventory, without running a model. Missing
coverage stays pending; stale source fingerprints require reauthorization.

`vcp workspace rebind WORKSPACE_ID` is the structured alias for existing rebind.
It also holds the selection lease and does not silently grant execution rights.
Its result includes `workspace_revision`. An explicit `vcp workspace trust
WORKSPACE_ID --expected-revision N` changes only local workspace trust after
native binding verification. It preserves Plan policy, existing denials, paused
tasks and provider configuration; no model request or execution grant is created.

`vcp doctor [--vault DIRECTORY] [--staging DIRECTORY] [--sync-root DIRECTORY]`
checks explicit path separation and native junction/reparse behavior without
opening an owner or loading keys. The selected workspace and data directory must
exist so the report describes actual paths. Undeclared synchronizers are not
inferred. Capacity figures are point-in-time lower bounds excluding backend
overhead; they neither reserve free space nor guarantee materialization fits.

Public vault paths and ciphertext reads admit the Windows Cloud Files tag family
used by OneDrive. The native check pins ancestors and the object, exposes the
actual placeholder tag for that thread, and rejects junctions, symbolic links and
unknown tags. Private staging and recovery paths still reject cloud placeholders.
Hydration failures remain explicit failures; recognizing a cloud path does not
establish that an upload or download completed.

Automatic hooks use only the verified signing capability explicitly loaded into
the current owner by backup controls. They do not reopen a recovery file or use a
secret cache. Configured owners without loaded material report backup pending;
local task state remains unchanged. Paused tasks can inspect or cancel backup
progress without resuming provider work. Repeated observations of the same task
state do not enqueue repeated backups. A newer lifecycle boundary queues one
coalesced follow-up behind an existing operation; that older snapshot does not
claim coverage of the newer state. Existing owner ticks start the follow-up after
the prior operation finishes. Failed preparation stays visible for deliberate retry.

A controlled CLI exit allows at most two seconds for local backup progress, then
requests cancellation before closing canonical ownership. Already admitted copy
obligations remain durable for reconciliation; an accepted preparation that has
not yet created a canonical snapshot job is reported as in-memory progress, not
as a saved snapshot. No process is launched to continue publication after exit.
Abrupt process termination relies on durable job recovery, not an exit hook.
