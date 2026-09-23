# VCP Local — initial read-only connection

P4-01 initial increment for local Windows and VS Code 1.138.0. This connects the Workspace view to the qualified SDK and compiled VCP engine. It does not complete P4-01's trust mutation, root rebind, cross-window handoff or reload acceptance requirements.

Set `vcp.engineExecutable` to an absolute trusted `vcp.exe` path in **User settings**. Set `vcp.dataDirectory` to the existing local VCP data directory when it differs from the engine default. Project and workspace overrides are ignored, including in trusted workspaces. The selected folder must already be initialized by VCP; the extension does not create a workspace or execute a project-defined configuration.

Open the VCP activity-bar view and choose **Connect…**, then select the full folder URI. Identical display names do not share identity. The connection uses local authenticated stdio with observer authority and no execution profile. **Refresh** reads a bounded current snapshot; **Disconnect** closes the owned client. Activation, webview restoration, editor trust changes and folder changes never connect or resume automatically. Restricted mode permits these read-only observations and displays editor trust separately from canonical engine trust.

The view shows engine build/path, protocol, execution host, canonical workspace/session/root, independent trust states, task count, pending decision count and snapshot watermark. Values are observations, not controller grants or completed-task claims. Full task content and approval controls belong to later work. If a native owner already holds the store, the connection fails safely; this increment does not discover or share pipe tickets.

Remote, virtual, UNC and non-Windows workspaces are unsupported. Moved/replaced roots require existing native reconciliation; root identity and binding revision are not invented from paths. Nothing stores credentials, task text or opaque attachment tickets in workspace/webview state. Webview messages select only fixed connection actions, and engine data is rendered as text under a restrictive content security policy.

## Build and qualify

`npm ci` installs the exact locked TypeScript 5.9.3, Node 24.10.1 types and VS Code 1.138.0 types. `npm test` builds the SDK/extension and runs portable bounded-connection/UI/package checks. `npm run stage` creates `artifacts/p4-vscode-extension`, including actual SDK distribution and protocol schema with no development links. No separate runtime npm dependency, provider gateway or engine binary is bundled.

Actual editor-host qualification passed with the official VS Code 1.138.0 Windows ZIP, an isolated user profile in genuine Restricted Mode, the staged package, and the compiled native engine on both stores. Registered observer commands and view initialization left canonical state unchanged. The native fixture requires `VCP_TEST_CODE` and explicit `--ignored` execution. Node tests and package import checks do not prove editor trust transitions or reload behavior. Native owner/rebind/control workflows are not claimed here. P4-05 separately owns installation/update/release packaging.
