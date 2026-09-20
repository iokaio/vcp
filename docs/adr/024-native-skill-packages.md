# ADR-024 — Native skill packages and activation authority

Status: implementation selected for P7-01; integrated qualification in progress.

## Decision

Use an original, versioned VCP JSON descriptor (`skill.json`) separate from each
package's body and resources. Discover descriptors only, with bounded native
traversal and no-follow repository handles. Explicit source registration supplies
scope and stable root identity. No home-directory scan, executable configuration,
network discovery or foreign configuration import is implied.

Resolve unqualified component IDs by workspace, user, then built-in source
precedence. Same-precedence ambiguity is an error; a qualified
`source::package::component` selects an exact candidate. This controls package
selection, not instruction authority. Current user requests and scoped AGENTS
guidance remain above activated skill content; provider conversion keeps skill
content out of the developer role. Skills never create tool or process grants.

Activation validates current setup requirements, exact descriptor/body/resource
identities, and current read policy. Sources overlapping the workspace (inside it
or containing it) must also pass the workspace root's read policy even when
registered under another root ID.
Capture activation reason, version, source and content in canonical artifacts and
task state. Revalidate selected dependencies and the task's logical skill revision
before model and tool dispatch. Preserve original artifacts and already-dispatched
effects when content changes or activation is disabled.

## Alternatives and provenance

The pinned Gemini G06 candidate has upstream mocked discovery/lifecycle evidence,
but [its source record](../../src/third_party/components/gemini-cli.md) explicitly
does not qualify a VCP extraction. Its eager body parsing and implicit source
combination are not imported. The new Rust implementation uses VCP's existing
repository, context, artifact and policy contracts. No Gemini/Pi source or runtime
dependency is copied by this increment. Foreign skill/configuration compatibility
remains governed by ADR-014 and the deferred import work.

Keeping descriptors separate makes body loading measurable and permits closed
schema validation before content acquisition. It requires native package metadata;
an arbitrary `SKILL.md` alone is not a supported package. Descriptor contract
version 1 is exact; compatibility with additional versions requires explicit
validation rather than an inferred version range.

## Evidence and reconsideration

P7-01 qualification covers actual file read counts, malformed/duplicate metadata,
native junction containment, selected-source changes, canonical activation,
instruction roles, denied broker dispatch, and prepared-work invalidation. The
[frozen experiment](../../src/evals/skills/README.md) measures local discovery and
activation costs; it does not establish skill usefulness. Bundled coverage and
live task quality remain P7-02 gates, and MCP remains P7-03.

Reconsider the format only for an explicit compatibility requirement with a
qualified migration path. Do not relax source identity or authority to accommodate
a foreign package.
