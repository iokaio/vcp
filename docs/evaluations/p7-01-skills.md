# P7-01 native skill discovery and activation

Qualification is in progress. This increment adds original VCP skill packages,
explicit source registration, bounded descriptor discovery and lazy activation.
It integrates activated content with canonical history, context manifests and
the existing model/tool admission boundaries. Package selection and instruction
authority are separate; skill text cannot grant broker permissions.

## Evidence collected

The core suite passed ten tests, including small/large lazy catalogs, malformed
metadata, duplicate resolution, selected-source invalidation, unrelated-source
isolation, and actual Windows junction escape rejection. The provider boundary
suite passed twelve tests, including preservation of skill attribution while
mapping skill content to the user role below project instructions.

The CLI library passed 42 tests, with one existing cloud-directory qualification
case explicitly ignored. The actual executable suite passed all twelve workflows
in 221.26 seconds, including provider-free descriptor inspection and terminal
activation/source/version/reason, missing-skill diagnostics, disabling and
inspection through the running owner. The native terminal test and all three
local optimizer tests also passed (`artifacts/p7-cli-native-tests.log`).
The strengthened terminal regression subsequently reopened the store and verified
the exact disabled ID, empty active set and logical revision 2, with zero provider
calls (`artifacts/p7-cli-skills-durable-final-tests.log`). The final general CLI
suite passed 69 tests with the same one explicit cloud-directory gate ignored
(`artifacts/p7-cli-general-final-tests.log`).

Two focused retained-host tests passed across Files and SQLite. They cover
discovery-only context, activation, hostile instructions under a trusted write
denial, changed content, and disabling a skill after a tool action is prepared.
The source-policy fixture also denies workspace aliases and containing source
roots across all three source kinds. The measured request grew from 6,589 to
7,083 bytes after activation; the tool schemas were unchanged. These observations
come from actual retained requests and broker dispatch, not model quality grades.
The final focused run also passed the reopen scenario on both stores: unchanged
activation history remains visible, while an omitted executable prerequisite
blocks native admission and produces zero HTTP requests and zero canonical
attempts (`artifacts/p7-skills-host-reopen-tests.log`, 2 tests, 11.33 seconds).

The first serial host run under the restricted sandbox passed 39 cases and
explicitly ignored six asset/export gates, then stalled on the existing invalid
executable spawn-failure fixture. The same copied binary passed that exact case
in 1.39 seconds under the repository owner; an isolated sandbox retry exceeded
its 30-second bound and was stopped. The initial partial results remain in
`artifacts/p7-host-sandbox-stalled-tests.log`, and the successful isolated result
is in `artifacts/p7-host-spawn-isolated.log`. Native qualification therefore uses
the repository-owner environment; the sandbox run is not reported as green.

The selected upstream workspace patch round-tripped byte-for-byte, and source
verification passed for all 7,939 selected files. No third-party skill loader or
skill text was imported, and no external dependency version changed.
All eight repository harness gates passed in run
`09c3ce2b-003a-4e8a-86e0-6ab2277839fe`. Rustfmt and Clippy over all targets in
extensions, models, lifecycle and CLI passed; Clippy retained existing warnings
outside the new skill implementation (`artifacts/p7-clippy-final.log`).

## Frozen discovery experiment

The frozen filesystem experiment passed all sixteen assertions across catalogs
of four and 128 packages. Discovery read 1,960 and 62,866 descriptor bytes,
respectively, with zero body/resource reads. Serialized listing projections were
729 and 23,443 bytes. Each activation loaded one 32,768-byte body and one
1,024-byte resource, plus three explicit dependency revalidation reads totaling
34,282 bytes. Discovery took 4.62 and 75.95 ms; activation took 3.76 and 3.62 ms
on this host. These single-run timings do not control filesystem cache effects
and are not model-token estimates. No model calls or spend occurred.

The run manifest is
`artifacts/p7-skill-discovery-qualification/06d2d422-c4fa-4ebd-91fc-0962c7614b21/manifest.json`;
it records unchanged source identity before/after, toolchain, hardware and hashes.
Source content identity is
`75b997f3107720a3a7fe173471beeb2fdc03f62af37a6e4eb91a6a487f1ad9bf`;
the report SHA-256 is
`96152c33cd977b95845c241e528659b68ee0821fc05d90c408cdc8b5d263d969`.

## Remaining qualification

The final full retained-host regression in the repository-owner environment is
pending.
This record does not yet declare P7-01 complete.

Bundled language coverage and live model task usefulness belong to P7-02. MCP
belongs to P7-03. This increment makes no claim about either, performs no paid
model calls, and does not qualify arbitrary foreign SKILL.md packages.
