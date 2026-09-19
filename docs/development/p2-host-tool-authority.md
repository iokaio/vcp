# P2 host tool ceilings and native preflight

P2-03 remains in progress. `foundation::Config::host_tool_denials` is mandatory
explicit trusted configuration for each owner, including an explicitly empty
list when no host ceiling is required. Rules are validated before opening the
canonical store: bounded identifiers, reasons, selectors and relative paths,
unique IDs and `Host` origin. The worker owns its configuration copy. User
policy, scoped grants, repository text and serialized history cannot change it.
Changing these ceilings requires closing the owner and reopening with newly
supplied trusted configuration; there is no live model/user-command setter.

The broker supplies these rules separately to the existing pure evaluator.
Denials precede grants and presets. Scoped opaque-operation denials preserve
their conservative interpretation. Preparation reads also check user and host
read denials, including the process executable root before opening or hashing
its executable. A path-scoped read denial conservatively excludes preparation
across the matching root, because preparation may observe ignore files and
source dependencies beyond the eventual mutation path.

Each proposal's `vcp-prepared-tool-v2` artifact records the owner/controller,
host rules and original prepared evidence. This is inspectable historical
evidence, not a portable executable capability. Older proposal artifacts remain
unchanged. These are native tool ceilings; they do not replace canonical data
access, context permission checks or an OS filesystem/network sandbox.

Before native file revalidation or process pinning, the broker checks canonical
task/owner availability, actor/scope, authority/steering/policy revisions,
registered roots, trusted profile identity, grants and applicable denials. This
preflight does not assert that source observations are current. Dispatch still
requires actual native revalidation followed by a fresh authority check. File
mutations repeat current checks for each file under the existing worker and
lifecycle ordering. Denied or stale tickets never acquire dispatch authority.

Late process output and known exit/job observations remain captured after a
hold or authority change. A fresh workspace scan is separately admitted under
current authority. If it cannot be admitted, the outcome explicitly records an
incomplete workspace observation rather than scanning under old permission or
discarding already-observed output.

See [authority coordination](p2-authority-coordination.md), [native tools](p2-tools.md)
and [process execution](p2-process.md). The [qualification report](../evaluations/p2-host-tool-authority.md)
records native evidence. Retained model-facing tool wrappers, the coding loop,
interactive terminal controls and full recovery/context refresh acceptance
remain separate P2 work.
