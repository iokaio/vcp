# Installation and compatibility

This experimental extension supports the local Windows x64 extension host in
VS Code **1.138.0**. Remote workspaces, web hosts and other editor versions are
not qualified. The engine is installed separately; the VSIX contains the SDK
and protocol schema, but no engine, provider credentials or publisher keys.

Install the candidate with **Extensions: Install from VSIX**, select the supplied
`vcp-local-0.2.1-win32-x64.vsix`, and reload when requested. This is the internal beta
pre-release channel, paired with native 0.2.0-beta.1 and SDK 0.2.1. For an isolated installation:

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
The candidate is unsigned and is not published to the Marketplace.

To update, preserve the data directory, disconnect the extension, install the
reviewed replacement VSIX and matching qualified engine distribution, then reload.
Resolve the launcher again and explicitly update User executable/data settings
before reconnecting. The extension never follows an installation update silently.
Reload restores observer references and command IDs, never controller authority,
transient previews, credentials or mutation replay. Reconcile original commands
before issuing new ones. Do not downgrade a canonical store across unsupported
formats; use a validated backup/restore procedure and compatible native engine.

Uninstall through Extensions or `Code.exe --uninstall-extension vcp.vcp-local`
with the same profile arguments. Uninstalling does not delete the engine registry,
canonical history, local vault, publisher keys or engine installation. Remove
those only through their documented native lifecycle and your retention policy.
