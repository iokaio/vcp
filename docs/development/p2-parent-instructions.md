# Explicit parent instruction grants

P2-01 remains in progress. The focused native retained-host fixture passed on
both stores, and the corrected boundary passed the complete native integration
suite. This is not an installed CLI feature. See the
[qualification report](../evaluations/p2-parent-instructions.md).

After `configure_coding` and before the first request or verification baseline,
the owner can call `CanonicalHost::configure_instruction_roots` with up to 32
explicit ancestor roots. Each root must match the canonical workspace and binding.
The existing instruction loader rejects duplicate identities, the workspace itself,
and unrelated directories. No parent directory is discovered or granted implicitly.

The grant reads only each ancestor's `AGENTS.md`. It does not add search, file-tool,
check-discovery or mutation roots. Parent content has project-instruction trust,
per-path applicability and original source attribution. It cannot override the
owner's policy or operating instructions. A durable artifact records the granted
root identities and their instruction-only purpose. Reopening requires fresh
explicit grants; persisted text cannot recreate authority.

Request assembly, admission and tool dispatch revalidate instruction versions and
absence probes across the workspace and granted ancestors. Current read denials
for every registered coding tool also apply to parent reads. Creating a previously
absent instruction or editing an existing one invalidates stale actions.

Verification captures parent instruction sources and includes their versions in
its dependency probes, keeping them separate from ordinary workspace file paths.
Completion pins existing parent sources with the workspace sources and rechecks
probes before the canonical transition. A changed parent invalidates earlier
verification. Unread, ungranted parent content has no effect on applicability.

The synthetic fixture covers both stores, explicit and absent grants, post-request
edits and creation, a named read denial, attempted parent-file escape, immutable
setup, and changed parent guidance between verification and completion. The 12
native scenarios passed. Git/index context and broader P2-01 acceptance remain separate
work. No dependency or upstream-source change is included.
