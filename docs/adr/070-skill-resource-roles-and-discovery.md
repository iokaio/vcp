# ADR-070: Skill resource roles, helper materialization and description discovery

Status: accepted by owner direction, September 29, 2026.
Owning items: SU-00 decision; SU-02, SU-03 and SU-04 implementation in
[the skills upgrade plan](../research/skills-upgrade-plan.md).

## Context

[ADR-024](024-native-skill-packages.md) and [ADR-069](069-practical-skill-ports.md)
govern native skill packages and practical upstream ports. The review after
SP-12 found three runtime gaps:

* Activation injects the body and every UTF-8 resource into model context
  (`skill_parts` in `vcp-lifecycle/src/foundation/worker/skills.rs`). Licenses,
  provenance notes and helper source are 40–66% of each workflow package, so
  each active workflow spends most of its context on text that the model does
  not need to follow the workflow.
* Discovery context lists only skills whose cues match observed root markers.
  Twelve packages use `explicit:<id>` cues that the host never emits, including
  all eight workflows, so the model is never told they exist.
* Workflow bodies run `scripts/...` helpers, but the installed package lives
  beside the executable and context parts carry no file path. There is no
  tested path from an active skill to a runnable helper.

## Decision

1. **Resource roles.** A descriptor content reference may declare
   `"use": "context"` or `"use": "file"`. Omission means `context`, so current
   descriptors keep their meaning. Activation reads, hash-checks and counts
   every resource regardless of role. Only `context` resources become model
   context. License, notice, provenance, requirement and executable helper files
   use `file`.
2. **Helper materialization.** Add a skill control operation that copies one
   verified `file` resource of an active skill to a relative workspace
   destination. It uses the canonical write authority and policy denials, exclusive
   create and no overwrite, and records the copy in history. It revalidates the
   skill before copying. It grants no process or network authority. Running a
   materialized helper still requires the existing authorized execution tool.
3. **Description discovery.** Skills with empty cues are always eligible for the
   bounded description-only discovery context. Workflow and cue-less families
   use empty cues instead of the unused `explicit:*` form. Host marker detection
   is extended deterministically for common root layouts. Descriptions remain
   untrusted metadata. Listing a skill does not activate it or grant it tools.

## Consequences

Packages keep their licenses and provenance in the distributed files without
spending model context on them. Helpers become runnable through ordinary
authorized write and execute steps rather than by the model retyping source.
The catalog, asset staging and authoring validator mirror the new field.
Binaries built before this change reject descriptors that use `use`. That is
acceptable because the builtin catalog is embedded in the binary and verified
byte-for-byte. External packages that need older hosts should omit the field.
ADR-069's licensing, integrity and verification policy is unchanged.
