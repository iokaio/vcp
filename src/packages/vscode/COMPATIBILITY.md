# Installation and compatibility

This experimental extension admits the local Windows x64 extension host in
VS Code **1.138.0 and newer 1.x releases** (`^1.138.0`), including **1.140.0**.
The API types remain pinned to the `1.138.0` baseline. Admission to this range
does not claim full native qualification on every editor release. Remote
workspaces and web hosts remain excluded. The engine is installed separately; the VSIX contains the SDK
and protocol schema, but no engine, provider credentials or publisher keys.

Install the candidate with **Extensions: Install from VSIX**, select the supplied
`vcp-0.2.4-win32-x64.vsix`, and reload when requested. This is the internal beta
pre-release channel, paired with native 0.2.4 and SDK 0.2.4. For an isolated installation:

```powershell
Code.exe --user-data-dir <private-profile> --extensions-dir <private-extensions> --install-extension <absolute-vsix-path>
```

Follow [the setup guide](SETUP.md) to resolve `vcp.exe --resolve-installation`.
Configure `vcp.engineExecutable` in User settings with its absolute versioned
`vcp.exe` path, and `vcp.dataDirectory` with its reported data directory.
Use a trusted local workspace to request controller authority. Restricted Mode
allows observation and explicit recovery/trust-revocation operations; it does not
authorize execution or publication. Execution and encrypted publication require
explicit native profile selection; the extension does not provision credentials.

The adjacent `manifest.json` records the archive hash, every packaged file hash,
schema/SDK identity, build tools and exact engine executable hash. Its engine
provenance must be `verified-release-build` for the internal beta. Its release
identity must exactly match the native candidate and reviewed clean source.
Development fixtures can be `caller-supplied-unverified` or `recorded-local-build`;
they have no beta release identity. A matching version alone is not a compatibility guarantee.
Native qualification is separate evidence for that exact candidate pair. Startup
negotiates protocol methods and capabilities; unsupported features remain disabled.
This version requires the signed Ioka LLC native candidate; construction and
qualification are in progress. The previously published beta.1 is unsigned.
The `iokaio.vcp` VSIX is prepared for
owner-controlled Marketplace pre-release publication; the release handoff records
whether upload has occurred. Publication does not complete qualification.

Earlier candidates used `iokaio.vcp-local` or `vcp.vcp-local`. Disconnect and uninstall that
extension before installing `iokaio.vcp`; the name change creates a separate
extension identity, not an automatic update. Keep native data and User settings,
then reconnect explicitly. Extension-local connection state does not migrate.

To update, preserve the data directory, disconnect the extension, install the
reviewed replacement VSIX and matching qualified engine distribution, then reload.
Resolve the launcher again and explicitly update User executable/data settings
before reconnecting. The extension never follows an installation update silently.
Reload restores observer references and command IDs, never controller authority,
transient previews, credentials or mutation replay. Reconcile original commands
before issuing new ones. Do not downgrade a canonical store across unsupported
formats; use a validated backup/restore procedure and compatible native engine.

Uninstall through Extensions or `Code.exe --uninstall-extension iokaio.vcp`
with the same profile arguments. Uninstalling does not delete the engine registry,
canonical history, local vault, publisher keys or engine installation. Remove
those only through their documented native lifecycle and your retention policy.
