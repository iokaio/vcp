# Explicit configuration imports

P10-02 imports a narrow, versioned MCP preference subset into an existing VCP
profile. The original profile remains unchanged. No import command starts a
provider, MCP process, hook or network request.

## Compatibility matrix

Select a compatibility profile explicitly; familiar filenames do not select one.
Neither source requires an intrinsic schema version. These profiles bind the
tested subset to immutable upstream source, not every future configuration.

| Profile | Source revision | Input |
|---|---|---|
| `codex-8b78600d-v1` | `8b78600dc85cc265d7e7e827f6aa903875405287` | Codex TOML |
| `gemini-6a466a7e-v1` | `6a466a7e2fe2b1255752c1e74f69b31f0216084d` | Gemini JSON |

The pure implementation is `vcp-extensions/import`; native selection and
persistence are in `vcp-cli/config_import`.

| Source field | VCP mapping |
|---|---|
| Codex `mcp_servers.<name>.enabled_tools` / `disabled_tools` | Intersect/subtract from the matching registration's permitted tools |
| Gemini `mcpServers.<name>.includeTools` / `excludeTools` | Same restriction, with literal tool names only |
| Codex MCP `tool_timeout_sec` / `startup_timeout_sec` | Checked integral seconds to milliseconds; tighten the shared MCP deadline |
| Gemini MCP `timeout` | Tighten the shared MCP deadline in milliseconds |

Exact names and transport identity must match an existing native registration.
HTTP uses Codex `url`, Gemini `httpUrl`, or Gemini `url` with explicit HTTP type.
Stdio matching requires the exact configured absolute executable, literal
arguments and working directory; no PATH search, shell expansion or command
execution occurs. Imported environment changes and credential values do not apply.
Credential references require explicit native setup. Gemini SSE/automatic/TCP
transport modes and function-like tool selectors are unsupported.

The native profile's tool allowlist and deadline remain ceilings. Imported
preferences cannot add a server, widen tools, extend a native deadline or grant
workspace trust. VCP uses a shared MCP deadline, so tightening an upstream startup
or tool deadline intentionally tightens all corresponding VCP phases.

Provider/model selection, endpoint qualification, pricing, authority, trust,
hooks, skills, extension discovery, includes and unsupported settings receive
diagnostics. They are not silently applied. Codex individual skill paths cannot
be treated as VCP discovery roots, and model names cannot replace qualified
provider evidence. Source secrets are omitted from diagnostics and history.

## Preview and selection

Use an existing native profile outside repositories and sync roots. The source
must be one explicitly selected file inside an explicit source root. Paths with
traversal, redirected ancestors or file aliases are rejected. Inputs are bounded
to 512 KiB and 16 nesting levels; multiline TOML is outside this subset.

```powershell
vcp --workspace D:\work\project --config D:\private\profile.json config import preview --source D:\imports\config.toml --source-root D:\imports --source-format codex-8b78600d-v1
```

Preview returns `preview_id`, source identity, base-profile hash, current import
revision, per-field diagnostics and concrete old/new restrictions. It writes
nothing. Diagnostic locations use deterministic field/array positions so unknown
source keys cannot leak secrets. Review partial support and select only desired change IDs:

```powershell
vcp --workspace D:\work\project --config D:\private\profile.json config import apply --source D:\imports\config.toml --source-root D:\imports --source-format codex-8b78600d-v1 --preview <preview_id> --select server.remote.allowed_tools --select server.remote.timeout_ms
```

Apply rereads and recomputes the preview. A source, profile, identity or revision
change requires a fresh preview. The selected IDs must be unique and present.
The source file and native profile are never overwritten.
Preferences take effect when the profile is next loaded. Import does not hot-reload
or change the authority of an already running owner.

## Recovery and rollback

`profile.json.vcp-imports` stores safe immutable preference revisions and source
provenance. It does not store raw source, credentials or old native profile
contents. The native profile still supplies endpoint/process/credential and
permission configuration; runtime intersections cannot exceed it.

```powershell
vcp --workspace D:\work\project --config D:\private\profile.json config import status
vcp --workspace D:\work\project --config D:\private\profile.json config import rollback-preview --revision 0
vcp --workspace D:\work\project --config D:\private\profile.json config import rollback --revision 0 --preview <preview_id>
```

Revision zero means the original native preferences. Rollback is a new revision
under the current native profile; it cannot resurrect revoked grants or copy
old credentials. Editing the native profile invalidates an old imported binding.
Use a fresh import or rollback preview to reconcile that edit before startup.

Publication selects either the preceding or new complete revision. Unpublished
staging files are ignored after interruption. Reopening never repeats an apply;
inspection reports the selected revision. A broken chain fails closed. History
is limited to 256 revisions; automatic pruning is not provided.

See [ADR-066](../adr/066-versioned-configuration-imports.md) for design choices.
Run `./scripts/test-imports.ps1` for native qualification and
`./scripts/test.ps1 -Suite fast` for repository gates.
