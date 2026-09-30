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
| description | What the skill does plus a "Use when ..." clause; it is the only text shown in discovery, so make it specific |
| source, license | Accurate attribution and applicable license; original VCP packages use `vcp-original` and `Apache-2.0` |
| cues | Root marker cues observed by the host (see below); empty means always listed by description |
| environments | Compatible environment names; empty means no metadata restriction, not qualified execution on every OS |
| required_tools | Compatibility requirements, never grants; ordinary builtin analysis uses `vcp_list` and `vcp_read` |
| body | Object with package-relative `path` and lowercase SHA-256 `sha256` |
| resources | Array of the same content-reference objects, plus optional `use`: `context` (default), `file` or `reference`; empty when unnecessary |

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
Use `scripts/skills/builtin-assets.cjs rehash` to recompute content, descriptor,
coverage and catalog digests after edits (versions stay your decision), then
`verify` and `stage` when working in VCP's repository; an installed user's custom skill need not modify the builtin catalog.

Metadata discovery reads descriptors, not bodies or resources. Observe both
content-read counters when testing that boundary. Activation revalidates the
selected descriptor and reads and verifies the body plus every declared resource
within configured byte limits.

Resource roles (ADR-070). A `context` resource (the default) becomes model
context on activation, so keep the body and context resources small. Mark
licenses, notices, provenance records, requirement files and helper source
`"use": "file"`: they are still read and hash-verified, but never sent to the
model. A body that needs a helper tells the model to copy it with `vcp_skill`
(action `materialize`) into an existing workspace directory and to run it only
through an authorized `vcp_exec` process profile. Materialization accepts
non-empty LF text up to 96 KiB that ends with a newline. The body is always context.

A `reference` resource (ADR-071) is verified but not sent on activation either.
The model reads it on request with `vcp_skill` action `read` (UTF-8, at most
64 KiB), which writes nothing. Use it for guidance only some tasks need, such as
per-provider or per-language notes, and keep the rules every task needs in the
body. Each active skill lists its `file` and `reference` resources to the model
by path and size. `vcp_skill` follows the read ceiling; `materialize` also needs
`vcp_patch`. Descriptors are limited to 16 KiB, as discovery reads them.

Cues and discovery. The host emits a cue for each of these root files when it
exists: `Cargo.toml`, `package.json`, `pyproject.toml`, `requirements.txt`,
`setup.py`, `Pipfile`, `go.mod`, `go.work`, `pom.xml`, `build.gradle`,
`build.gradle.kts`, `settings.gradle`, `settings.gradle.kts`, `CMakeLists.txt`,
`meson.build`, `Gemfile`, `composer.json`, `pubspec.yaml`, `Package.swift`,
`global.json`, `deno.json` and `deno.jsonc`. It also emits `*.sln`, `*.slnx`,
`*.csproj`, `*.fsproj`, `*.vcxproj` or `*.vbproj` for ordinary root files with
those extensions. A skill with cues is
listed only when one matches. A skill with empty cues is always listed by its
description, within a bounded budget. Listing never activates a skill or grants
tools. Explicit selection may resolve a compatible skill without a cue match. Do
not change matching logic, source precedence or activation authority in a
content-only skill update.

Example descriptor (hashes shortened for display; use real lowercase SHA-256):

```json
{
  "schema_version": 1,
  "id": "csv-cleanup",
  "version": "1.0.0",
  "description": "Normalize delimiters, encodings and headers in local CSV files without losing rows. Use when a task cleans or reshapes CSV data.",
  "source": "vcp-original",
  "license": "Apache-2.0",
  "vcp_version": 1,
  "cues": [],
  "environments": [],
  "required_tools": ["vcp_list", "vcp_read"],
  "body": {"path": "SKILL.md", "sha256": "<sha256>"},
  "resources": [
    {"path": "references/dialects.md", "sha256": "<sha256>"},
    {"path": "scripts/clean_csv.py", "sha256": "<sha256>", "use": "file"}
  ]
}
```

Version/hash validation proves identity, not safe or useful behavior. Run the
owning fixture and artifact checks, retain prior evidence under its old identity,
and report unavailable checks honestly. For current skill delivery use scoped
functional/package checks; comparative model campaigns are optional research.
