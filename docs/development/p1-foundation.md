# P1 durable foundation

The first P1 increment implements typed domain transitions, private command
admission, binary artifact staging and two canonical storage backends. It is a
library foundation in the retained Cargo workspace. The retained controller's
production integration remains P1 work; [accounting and history](p1-accounting-history.md)
are implemented by the next library increment;
this increment does not introduce a second scheduler or a usable product CLI.

## Source and commands

| Package | Working boundary |
|---|---|
| `src/crates/vcp-domain` | Opaque IDs, decimal-string revision domains, stable workspace identity, objective history, task/turn/effect transitions and applicable verification |
| `src/crates/vcp-protocol` | Version 1 command/event/receipt JSON, canonical command digest, bounded subscription cursor |
| `src/crates/vcp-store` | Shared transactions, SQLite, files journal, immutable artifact chunks, owner locks, snapshots, conversion and controlled activation |
| `src/crates/vcp-engine` | Authenticated command handlers over `CanonicalStore`, current host evidence, interactive/JSONL parity, pull subscriptions and capture fencing |

Run from the repository root on native Windows with Rust 1.98.0, Visual C++ x64
tools and the already resolved Cargo dependencies:

```powershell
pwsh -NoProfile -File scripts/test-foundation.ps1
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

The first command builds and runs real Rust contracts and child-process crash
fixtures. It writes source hashes, actual commands, toolchain/filesystem identity,
test names and log hashes under ignored `artifacts/foundation/<run>/`. It rejects
missing prerequisites as `not_run`, uses `--locked --offline`, and never calls a
paid provider. Package tests also run directly through Cargo; the crash fixture
requires `--features vcp-store/qualification`. Qualification observers and the
fixture binary are absent without that feature.

## Private command semantics

UUID v4 identities are opaque, validated strings. Counter types distinguish
entity, steering, authority, owner, session, memory and deletion revisions;
their complete `u64` range serializes as canonical decimal strings. Moving a
workspace changes its binding and invalidates execution authority while retaining
its identity and historical references.

The version 1 digest is SHA-256 over the whole command envelope serialized with
recursively sorted object keys. Array order is significant. Authenticate current
caller/workspace/session access first, then resolve a matching durable command
receipt, then check the current owner and entity revisions. This lets an
authenticated retry recover a lost reply after restart. A changed target, payload,
caller or expected revision under the same command ID conflicts.

Handlers commit entity changes, ordered events and the original result together.
An acknowledgement means accepted durable state, not task completion. Completion
requires applicable evidence and completed output artifacts; exit zero alone is
insufficient. Objective questions leave objectives unchanged. Decisions bind the
actor, policy, operation hash, effect revision, steering revision and expiry.
Host revalidation facts are in-process capabilities, never JSON booleans granting
authority. P2 owns the complete execution/autonomy policy.

Subscriptions use pull-based backpressure: at most 128 events per page, 16 active
cursors per engine and a 60-second snapshot lifetime. The boundary is exclusive
of `after` and inclusive of the captured end sequence. Reconnection can replay a
page, but it cannot invent a second logical event. Scope/retention changes,
expired snapshots and unavailable sequences return an explicit gap with a restart
route. Final results remain in canonical history; no producer queue drops them.

## Capture and storage formats

Artifact schema 1 publishes immutable chunks of at most 64 KiB. Every successful
write reports its confirmed byte range. The default object capacity is 1 GiB with
at most 65,536 chunks; hitting either limit fails admission rather than silently
truncating. Full binary bytes and stdout/stderr identity are independent of UI
rendering. A sealed descriptor records digest, length, media/schema, scope,
omissions and complete/aborted state. Unsealed captures expose an exact retained
prefix and an unobserved tail. Older pending descriptors remain valid prefixes
when more data arrives. Publication flushes bytes before immutable references.

Request-body input has no credential-header or recovery-key field. Credentials
are non-serializable transport inputs. This does not claim that arbitrary user
content can be heuristically stripped of every possible secret. A failed or
interrupted capture fences further dependent capture/dispatch; reopening finds
unfinished objects even without a final canonical error event. Configured sync
destinations are rejected before creating plaintext. Unconfigured third-party
synchronization cannot be inferred from folder names.

Canonical format 1 uses bounded transactions (8 MiB), records (1 MiB), 100,000
records and a 64 MiB materialized logical view. Reaching a bound returns a typed
limit error; it never prunes work history. Capacity expansion requires an explicit
format/implementation change. Logical records, references, scoped event sequences,
receipts and optimistic revisions share one validator across both backends.

SQLite uses normalized records/edges/events/commands plus checksummed immutable
commit payloads, schema version 1, WAL, `synchronous=FULL`, foreign keys and a
100 ms busy timeout. Compare-and-set mutations and receipts commit atomically.
Reopen verifies SQLite integrity and compares materialized rows with canonical
replay. A failed or cancelled in-flight commit poisons that handle until reopen;
it cannot return a later false acknowledgement.

The files backend uses `VCPJ0001`, little-endian payload length and its inverse,
the preceding frame's SHA-256, canonical JSON, a SHA-256 over header/payload, and
`VCPCMIT1`. A synced immutable tip is published before acknowledgement. It makes
truncated acknowledged history distinguishable from an incomplete new tail.
Only an uncommitted tail is quarantined; committed corruption fails closed.
Sealed checkpoints bind full logical state, watermark, hash and journal chain.
The complete journal remains the replay source; checkpoints currently validate
recovery state rather than accelerate replay or authorize journal deletion.

## Ownership, snapshots and activation

The canonical root has one real OS owner lock spanning state and accounting.
PID/nonce contents are diagnostic; abrupt process death releases the lock.
Snapshots hold a coherent logical view and artifact reader pins. Orphan collection
requires absence from all canonical references plus exclusive capture and reader
locks. It does not implement automatic retention or physical history purge.

Conversion creates a new root, copies verified retained artifact bytes and
replays immutable transactions without issuing tools or model requests. It
requires quiescent writers, preserves receipts/IDs/sequences and compares complete
logical state. `migration::ActiveRoot` serializes activation with a separate root
set lock, reopens the candidate for validation, then publishes a chained immutable
activation record. A publication failure fences writes until reopen. Old roots
remain intact for explicit recovery/retention policy; post-switch writes are
never merged back automatically. Unsupported format versions/preferences fail
explicitly. There is no older production schema to migrate in this first format.

Native forced-process tests cover eight transaction barriers and six conversion
barriers across the two backends. An independent observer inspects SQLite rows
or journal markers before library recovery. These tests establish the reported
Windows filesystem/process-failure envelope, not hardware power-loss durability,
every filesystem, encrypted cloud transfer or final product installation.

See the [P1 plan](../plan/02-engine-state-and-capture.md) and
[storage/accounting plan](../plan/03-storage-and-budget.md) for remaining
acceptance. The next increment covers the shared ledger and fresh-process history
rebuild; retained model/helper transport observations remain outstanding.
