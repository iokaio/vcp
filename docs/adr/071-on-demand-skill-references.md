# ADR-071: On-demand skill references and scoped vcp_skill

Status: accepted by owner direction, September 30, 2026.
Owning items: SH-00 decision; SH-04 implementation in the post-upgrade review
of [the skills upgrade plan](../research/skills-upgrade-plan.md).

## Context

[ADR-070](070-skill-resource-roles-and-discovery.md) introduced `context` and
`file` resource roles and helper materialization. The post-upgrade review found
four related gaps.

* **Every context reference is sent.** Four packages send all of their context
  references on activation, although their bodies say to read only what
  matches: llm-integration (about 31 KB), frontend-design (20 KB),
  mcp-development (19 KB) and document-authoring (10 KB).
* **The model cannot see what it may copy.** It cannot tell which `file`
  resources exist, because they never enter context.
* **`vcp_skill` is offered where it cannot work.** Skill state is per task, but
  the tool is offered wherever `vcp_patch` is allowed, including child tasks
  where no skill is active.
* **The implementation differs from ADR-070's wording.** ADR-070 describes
  materialization as a "skill control operation". It was implemented as the
  model tool `vcp_skill`, implied by the `vcp_patch` ceiling, with history
  recorded by the ordinary patch receipts.

## Decision

1. **A `reference` role.** A descriptor content reference may declare
   `"use": "reference"`. It is read, hash-checked, counted and captured like
   every resource, but it is never added to model context on activation.
2. **A `read` action on `vcp_skill`.**
   * **Input.** The action takes `{skill, resource}`. It returns the
     re-verified bytes of one `reference` resource of an active skill as a
     bounded tool result.
   * **Limits.** The resource must be UTF-8 and at most 64 KiB, and the action
     never writes to the workspace. It does not read `context` or `file`
     resources.
   * **Trust.** The returned text is skill guidance under the same precedence
     as the active skill: below user constraints and AGENTS.md, and never a
     grant of tools or permission.
3. **Ceiling.**
   * `vcp_skill` is implied by `vcp_read`, not by `vcp_patch`.
   * `materialize` additionally requires the `vcp_patch` permission and keeps
     ADR-070's patch path and exact-bytes check.
   * Neither action adds a recorded ceiling name, so stored ceilings and legacy
     defaults are unchanged.
4. **Scope.** `vcp_skill` is not offered in child tasks, because they have no
   skill state.
5. **Resource manifest.** Each active skill adds one small context part that
   lists its `file` and `reference` resources by path, byte size and role,
   without their content.
6. **Record of the implementation.** ADR-070's materialization operation is
   implemented as the model tool described here. ADR-070 is not edited.

## Consequences

Large packages can keep per-topic references out of context and still offer
them on request. The model can see what it may read or copy. Read-only
profiles can read references but cannot materialize helpers. The descriptor
role, catalog, asset stager and authoring validator all accept `reference`.
Binaries built before this change reject descriptors that use it, which is
acceptable for the embedded builtin catalog. External packages that must work
with older hosts should not use it.
