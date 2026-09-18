# Classified upstream effects

The [boundary catalog](../../src/third_party/components/codex-boundaries.json)
uses schema version 2 to classify 47 named source entries or exported type
surfaces. It covers 159 packages in 23 ownership groups. Classification
is a source-reviewed effect envelope, including delegates and callbacks. It is
not enforcement, permission to call an entry, or an exhaustive proof of every
function in a package. The [selection gate report](../evaluations/p0-07-selection-gate.md)
records completed P0-07 selection acceptance. Two later effectful entries belong
to the [P0-02 corpus qualification executable](local-memory-spike.md), under the
qualification harness's ownership.

Each of the 23 groups assigns an explicit `effect_ceiling` and primary
`vcp_boundary` to its listed packages. The selected baseline conservatively
treats every group as effectful: an unreviewed exported helper cannot inherit
purity from its package name. The narrower named entries below refine that
envelope; they do not make an entire package pure. The checker rejects a missing
module ceiling/owner, a ceiling below its declared capabilities, or a named
entry with effects beyond that ceiling. Build/test-only modules are owned by
`qualification-harness`, not a product permission path.

## Classification contract

| Class | Meaning in this catalog | Current examples |
|---|---|---|
| `pure` | Computes from supplied data without declared ambient I/O or externally shared state changes | Munarium's snapshot/candidate governance gate entry |
| `read-only` | Reads named external inputs but does not declare writes, dispatch or publication | Explicit asset loading; ambient-key and environment-presence collectors |
| `effectful` | Can dispatch, publish, mutate shared state, write storage, start/stop work or invoke an effectful collaborator | Coding/helpers, MCP, process control, policy amendments, maintenance, diagnostics |

The 47 entries comprise one pure, three read-only and 43 effectful envelopes.
The corpus resource sampler owns a temporary measurement thread and reads native
process state; it does not implement product scheduling or resource limits.
The embedding qualification executable includes explicit network canaries; this
effect does not introduce a network route into the file-only embedding library.
Read-only credential discovery is still disallowed as implicit VCP authority.
The environment-presence collector is read-only; the later telemetry publisher
has its own effectful boundary. Pure gate output remains evidence subject to
scope and revision checks, not authority to apply a claim.

Each seam declares `effect_class`, `effects` and its primary `vcp_boundary`.
The validator requires known classes/effects/owners, unique effects, no effects
on a pure entry, and only `file-read`, `environment`, `credential-read` or `clock`
on a read-only entry. It also retains exact package ownership, source pin,
path containment, source-symbol and ledger-owner checks. It rejects version 1
catalogs rather than silently accepting entries without classifications.

These checks verify declared consistency. The source review must still follow
the implementation and its callees; a package name, empty effect list or matching
symbol cannot establish semantic purity. Allocation within an owned computation
is distinct from shared/global mutation. For example, the embedding helper's
CPU inference is classified conservatively for its worker-pool/global resource
behavior; this does not imply a model-provider request or remote fallback.

## Newly exposed paths

| Entry | Reviewed effect | Required VCP handling |
|---|---|---|
| TUI `get_upgrade_version` | Release-only background network discovery and version-cache writes | Disable the upstream route. Debug `exec` traces cannot prove release startup is quiet |
| Daemon updater `run` | Installer fetch, process execution/restart, mutable installation state | Disable independent daemon/update authority; VCP release delivery owns any replacement |
| Rollout maintenance lock | Creates a lock file/directory and takes an OS lock | Canonical-store ownership and controlled maintenance lifetime |
| Rollout compression worker | Fire-and-forget history replacement, clocks and diagnostics | Canonical receipts, retention policy, coherent publication and owner pause |
| Core API `ThreadManager` facade | Exports live controller/store/provider capabilities | Same controller admission as direct engine entry points |
| Feature `emit_metrics` | Publishes telemetry from a configuration module | Disable upstream publication; retain compatible configuration logic |
| ANSI `ansi_escape_line` | Can log formatted input on unexpected multiline data | Explicit diagnostic sink and content retention/redaction policy |

`codex-core-api` moves from the protocol/data group to controller ownership, and
`codex-features` moves to configuration. Rendering/configuration now declare their
diagnostic surface. This changes review ownership, not imported code or runtime
behavior. Useful data/formatting functions can still be retained; none inherits
permission to publish content merely because it formats output.

## Adapter ownership and evidence

`controller` owns admission and lifetimes; `model-gateway` owns every coding and
helper request; `tool-broker` owns prepared tool/process/MCP effects;
`canonical-store` owns durable facts, receipts and maintenance; `local-memory`
owns scoped local computation/indexing; `explicit-credentials` owns configured
credential access; `diagnostic-sink` owns intentional diagnostic capture.
`disabled-upstream-route` identifies source retained for qualification that VCP
must not expose independently. These are planned responsibilities, not implemented
interfaces or package permissions.

A primary owner does not bypass another boundary. A controller child or
model-assisted memory operation must still enter the model gateway; an MCP
launcher must still enter prepared execution; a diagnostic writer must still
obey content scope and retention. Follow the [engine design](../architecture/engine-execution-design.md),
[context/provider design](../architecture/context-provider-design.md),
[routing/extensions design](../architecture/routing-extensions-design.md) and
[upstream reuse ADR](../adr/013-upstream-reuse-and-vendoring.md).

[Nine native traces](native-cli-trace.md) provide representative coding, review,
compaction, retry and rejection observations. Their [helper effect map](helper-effect-traces.md)
explains the observed usage/history discrepancies. They do not execute the
release updater, background maintenance, all credential flows or all optional
feature combinations. P0-03/P0-05/P0-08 must implement and independently observe
the actual lifecycle, authority and accounting controls before product use.
