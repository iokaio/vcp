# Native skill discovery and activation

P7-01 uses `vcp-extensions::{skill_manifest,discovery,activation}` for bounded
package data and native reads. `vcp-lifecycle::foundation::skills` binds those
observations to current canonical state, context and broker admission. CLI setup
and controls live in `vcp-cli::skills`. The mechanism is recorded in
[ADR-024](../adr/024-native-skill-packages.md).

## Package format

A package contains a closed-schema `skill.json` descriptor and separate body and
resource files. `SkillDescriptor` is the authoritative typed schema. Required
fields are schema version, stable lowercase component ID, component version,
short description, source/license attribution, exact supported VCP skill-contract
version, cue/environment/tool sets, a body path/hash and resource path/hash rows.
Versions are explicit metadata; a display name never determines identity.

Paths use normalized relative slash-separated components and lowercase SHA-256
digests. Traversal, rooted paths, alternate streams, content aliases and linked
escapes are rejected. Source URIs are attribution only and are never fetched.
Bodies must be UTF-8. Binary resources may be captured as artifacts but are not
silently converted to model instructions.

Descriptor discovery does not open bodies/resources. Defaults bound source count
to 32, traversal to depth 4 and 8,192 entries, descriptors to 1,024, individual
descriptor size to 16 KiB and total descriptor bytes to 4 MiB. Activation bounds
individual body/resource reads to 256 KiB and their combined bytes to 1 MiB.
Caller-supplied limits are themselves bounded. Read counters distinguish loading
from the additional reads needed to revalidate selected dependencies.

## Configuration and inspection

The existing explicit user-owned profile has an optional `skills` member with
`version: 1`, a revision, explicit sources and an optional disabled-identity set.
Each source has a stable ID, kind (`builtin`, `user`, or `workspace`), enabled flag,
stable `root_id`, and absolute local collection path. Workspace sources stay in
the registered workspace. User/built-in sources are explicit read roots; they do
not become writable tool roots. Current canonical read denials apply before
discovery, including denials on an enclosing workspace root.

`vcp skills list --offset <n>` inspects short descriptors and setup diagnostics
without loading provider credentials or starting inference. It uses the current
owner or the existing store's exclusive access boundary. `/skills list --offset
<n>` provides the active session view. Listings and task-disabled identities are
paged with next offsets using the same command; large catalogs are not silently
hidden behind a fixed first page.

Workspace sources outrank user sources, which outrank built-ins for an unqualified
ID. Same-priority duplicates require explicit qualified selection. A qualified ID
is `source-id::relative-package::component-id`; a collection-root package uses
`.` as its package component. Filesystem enumeration order does not pick a winner.

Matching uses host-observed project markers, native environment and configured
adapter/process availability. Cues suggest candidates; an explicit selection can
bypass cue matching but cannot bypass environment or tool prerequisites. Tool
availability does not grant permission to use a tool or install a missing one.

## Activation and change handling

Use `/skills activate <qualified-id> [reason]` to select a package explicitly.
Missing, ambiguous, disabled, incompatible or stale content produces a visible
diagnostic. Activation captures source/version/reason and exact content artifacts
in task-scoped canonical history. It does not submit a model request. Active
skill text is attributed and mandatory in subsequent context; compaction cannot
silently discard its requirements. Trusted operating instructions state that
current user requests and scoped AGENTS guidance outrank package instructions.
The ordinary broker still enforces every actual tool effect.

`/skills disable <qualified-id>` removes the package from that task's future
context. A later explicit activation can re-enable it, subject to the current
source registry's disabled set and all normal checks. Changes to selected source identity, descriptor,
body or resources invalidate dependent prepared work. Active requirements are
checked again against current setup on reopen. Historical artifacts remain
inspectable, and already-dispatched effects retain normal reconciliation.

The logical skill revision is separate from the generic projection row revision:
the first activation advances the logical revision from 0 to 1 while creating
row revision 0. This preserves both canonical publication rules and invalidation
of work prepared before the first activation.

## Qualification scope

The [filesystem experiment](../../src/evals/skills/README.md) measures synthetic
small/large catalogs and retains all assertion outcomes. Host tests exercise
current policy, instruction roles, actual denied dispatch, and selected-source
invalidation on both Files and SQLite. CLI tests cover provider-free inspection
and explicit activation in the retained terminal. None of these tests establish
bundled language coverage or model task quality; those remain P7-02.
