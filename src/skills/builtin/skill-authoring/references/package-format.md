# VCP skill package contract, version 1

This reference describes the existing `SkillDescriptor` contract. Check the
target VCP version and repository implementation before extending it. Do not add
Codex/Claude loader metadata or assume that another product's SKILL.md frontmatter
is a VCP descriptor.

A package directory contains `skill.json`, its declared body (normally `SKILL.md`)
and only resources that the workflow needs. `skill.json` has these fields:

| Field | Meaning |
|---|---|
| schema_version, vcp_version | Both integer `1` for this contract |
| id | Stable lowercase ASCII identifier; follow existing hyphenated package names |
| version | Explicit content version; do not reuse it for changed package bytes |
| description | Concise workflow and selection guidance, not the full instructions |
| source, license | Accurate attribution and applicable license; original VCP packages use `vcp-original` and `Apache-2.0` |
| cues | Caller-observed matching cues; not patterns granting filesystem traversal |
| environments | Compatible environment names; empty means no metadata restriction, not qualified execution on every OS |
| required_tools | Compatibility requirements, never grants; ordinary builtin analysis uses `vcp_list` and `vcp_read` |
| body | Object with package-relative `path` and lowercase SHA-256 `sha256` |
| resources | Array of the same content-reference objects, empty when unnecessary |

Unknown descriptor fields are rejected. Content paths use forward slashes and
normalized relative components, with no rooted path, traversal, Windows device
alias, descriptor alias or case-insensitive duplicate. Hash the exact stored bytes,
including line endings. Do not put credentials or private data in descriptors,
bodies, resources or examples.

For a builtin change, follow the existing inventory chain: body/resource bytes →
content hashes in the descriptor → descriptor hash and matching attribution/content
references in `catalog.json`; coverage bytes → catalog coverage hash. Advance the
catalog version for its change. Coverage must distinguish supplied guidance from
observed runtime/usefulness evidence. Do not label fixture expectations as results.
Use `scripts/skills/builtin-assets.cjs verify` and `stage` when working in VCP's
repository; an installed user's custom skill need not modify the builtin catalog.

Discovery reads descriptors. Activation revalidates the selected descriptor and
reads and verifies the body plus every declared resource within configured byte
limits. Keep the complete activation small even when references are separate files.
Explicit selection may resolve a compatible skill without a root-cue match;
automatic suggestions depend on the existing matching logic. Do not change that
logic, source precedence or activation authority in a content-only skill update.

Version/hash validation proves identity, not safe or useful behavior. Run the
owning fixture and artifact checks, retain prior evidence under its old identity,
and leave default promotion pending when its required evidence is unavailable.
