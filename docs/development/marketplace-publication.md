# Marketplace publication

## Current 0.2.4 publication: `iokaio.vcp`

The owner explicitly authorized publishing the verified `0.2.4` candidate to
Marketplace. VSCE accepted the exact VSIX upload using the existing authenticated
Azure publisher-owner login. Public Marketplace validation completed, and the
exact `0.2.4` Windows x64 prerelease is publicly downloadable. This
authorization supersedes the earlier manual-first-upload restriction for this
candidate; [ADR-075](../adr/075-marketplace-manual-upload.md) and the historical
manual-upload evidence below remain unchanged.

- Candidate: [37096239569, attempt 1](https://github.com/iokaio/vcp/actions/runs/37096239569/attempts/1).
- Source: `53e4d16667eed318208037b63f0f77db804c57b5`.
- Pair: `1e791436ab850192fba5e4c58f0c294db584d1a3f16d6630d80709a2618ffe55`.
- Package: `vcp-0.2.4-win32-x64.vsix`; publisher `iokaio`, name `vcp`, target
  `win32-x64`, VS Code engine range `^1.138.0`. The VSIX manifest, extension
  package and bundled SDK all declare `0.2.4`, matching the native installer.
- Official CLI installation and actual extension activation passed on VS Code
  `1.138.0` and `1.140.0`; all 59 installed payload files matched the exact VSIX.
  Installed `package.json` was compared after removing editor-added `__metadata`;
  the other payload files matched by hash. These were direct-VSIX installations.

Public gallery verification confirmed version `0.2.4`, target `win32-x64`,
pre-release status and engine range `^1.138.0`. The CDN download's whole-file
SHA-256, gallery-advertised SHA-256 and all 59 extension payload entries exactly
matched the submitted VSIX. Marketplace did not alter the container or payload.
Evidence: `artifacts/beta-delivery/marketplace-024-37096239569-4/verification.json`.
The first three public checks ran before validation completed and are retained
as pending observations; no republish was performed.

The submitted VSIX SHA-256 is
`084932ba6e508b52981f796d36f08fc74da9074fc76e9dda085701db89feda53`.
The exact package is retained at
`artifacts/local-candidate/signed-0.2.4-37096239569/verified/assets/vcp-0.2.4-win32-x64.vsix`.
VSCE reported `Published iokaio.vcp (win32-x64) v0.2.4`; that upload response was
observed in the tool result, with no separate upload log retained. Editor
verification receipts are
`artifacts/local-candidate/editor024-final-1.138.0-retry1/verification.json` and
`artifacts/local-candidate/editor024-final-1.140.0/verification.json`.

The listing identity is
[iokaio.vcp](https://marketplace.visualstudio.com/items?itemName=iokaio.vcp).
The matching signed native downloads were published through
[run 37097991085](https://github.com/iokaio/vcp/actions/runs/37097991085), as recorded
in [beta publication](beta-publication.md). Four native executable signatures,
actual product versions, packet admission and offline CLI checks passed.
Clean-host support, installed-native qualification, provider tasks and full
qualification remain incomplete; the editor compatibility checks do not satisfy
those separate gates.

## Historical 0.2.2 handoff: `iokaio.vcp`

The owner requested the shorter extension ID `iokaio.vcp` for the next candidate.
Its display name remains **VCP Coding Agent** and its category is **AI**.
The matching signed pair is [published on GitHub](https://github.com/iokaio/vcp/releases/tag/v0.2.0-beta.2-e9e8ba52684a)
and [the downloads page](https://downloads.ioka.io/); its `pair` checkpoint passed,
with later qualification still pending. The owner is manually uploading its exact
`vcp-0.2.2-win32-x64.vsix` through **New extension → Visual Studio Code** under
[publisher iokaio](https://marketplace.visualstudio.com/manage/publishers/iokaio).
This is a new listing, not an update to the existing `vcp-local` entry.
Verified local file: `artifacts/marketplace-upload/iokaio-vcp-0.2.2/assets/vcp-0.2.2-win32-x64.vsix`.
SHA-256: `9d8f43f896aa2b3ebedd8e3dd9795cf2ab4331dd889d2a49ffd82ef7b6ab8098`.
The public GitHub VSIX has the same hash. Marketplace upload completion has not
yet been independently confirmed. The owner reports no end-user installations
of `iokaio.vcp-local`; no end-user migration campaign is needed.

Disconnect and uninstall `iokaio.vcp-local` (or the earlier `vcp.vcp-local`) before
installing `iokaio.vcp`; retain the native installation, data and User settings,
then reconnect explicitly. Extension-local connection state does not migrate.
The existing listing remains available; removal or deprecation has not occurred.
The records below describe the completed first upload and retain its original IDs.

## Historical first upload

Status: **owner upload completed; public Marketplace listing live** on October 1,
2026. [VCP Local](https://marketplace.visualstudio.com/items?itemName=iokaio.vcp-local)
returns HTTP 200. Post-upload Marketplace installation verification passed.
Contract: [ADR-075](../adr/075-marketplace-manual-upload.md), BETA-07/BETA-08/BETA-11.
The owner performed the first upload manually. The older GitHub release's
`vcp.vcp-local` package is not the upload candidate for publisher `iokaio`.

## Select the verified package

The uploaded package is **`vcp-local-0.2.1-win32-x64.vsix`** from the
[new release](https://github.com/iokaio/vcp/releases/tag/v0.2.0-beta.1-9694d258aeb5)
([direct VSIX download](https://github.com/iokaio/vcp/releases/download/v0.2.0-beta.1-9694d258aeb5/vcp-local-0.2.1-win32-x64.vsix)).
The prepared local copy is
`artifacts/marketplace-upload/9694d258aeb5/vcp-local-0.2.1-win32-x64.vsix`.
Its SHA-256 is:

```text
4ae7861f32908d4e63b99c9912146420ebb023cebc34d8e62dd5696736245c34
```

Independent archive inspection confirms publisher `iokaio`, name `vcp-local`,
version `0.2.1`, target `win32-x64` and VSCE pre-release metadata. Filenames and
versions alone do not distinguish it from the earlier manual candidate.

- Source: `99de062d32c4c0fe20ddae119408e953e8f7243b`, merged [PR #328](https://github.com/iokaio/vcp/pull/328).
- Pair: `9694d258aeb519b03c962191be137758283723818d3f4d1bd3340301bba71395`.
- [Successful candidate attempt 1](https://github.com/iokaio/vcp/actions/runs/36894802050/attempts/1)
  and [retained packet](https://github.com/iokaio/vcp/actions/runs/36894802050/artifacts/11180457602).
- Full packet `SHA256SUMS` SHA-256: `49dd1d31fa94cc472004b2b012029b5581fa196aee94f495bf9cd9cac80e25c8`.

| Matching native artifact | SHA-256 |
| --- | --- |
| `vcp-0.2.0-beta.1-windows-x64-unsigned-setup.exe` | `ac950882bb8d38efffc6e24053c2870e9d284039eb66c58ae804cde8ecceca31` |
| `vcp-0.2.0-beta.1-windows-x64-unsigned.zip` | `9b3d72d5825c4a57e6bf36c9611a18e6066f31f1c13d3281f0c418c7675a0efa` |

Keep the matching native installer available at [VCP downloads](https://downloads.ioka.io/).
The extension contains the SDK and schema, not the native engine. Its README
describes installation, exact editor/platform scope, unsigned native binaries,
support and incomplete qualification.

[Publication run 36900026024](https://github.com/iokaio/vcp/actions/runs/36900026024)
passed both release and Pages jobs. All five public asset digests match the
prepared output. The prior `1dba45922e0c` release remains intact. The download page
uses GitHub-managed HTTPS with HTTP-to-HTTPS enforcement. The owner subsequently
completed the manual Marketplace upload; the instructions below are retained as
the first-upload procedure.

## Verified and remaining evidence

PR and [exact-main Delivery checks](https://github.com/iokaio/vcp/actions/runs/36894438344)
passed. Candidate stages through `pair` passed on `vcpwin`, using 16 jobs and
Windows build image `win25-vs2026` (`10.0.26100`). Production compilation and
verification took 23 minutes 47 seconds. All 88 packet checksums, independent pair
reconstruction, 643 native archive entries and 61 VSIX entries verified locally.

Focused checks used these exact bytes on developer Windows `10.0.26300.0` and
pinned VS Code `1.138.0`:

| Check | Result and limit |
| --- | --- |
| Registered native installation, launcher/engine identity and removal | Pass; custom Unicode paths, both storage preferences and synthetic data sentinel preserved. |
| Offline onboarding | Pass; three help commands and two missing-input cases. No live provider setup or task. |
| Installed VSIX connection and removal | Pass on fresh Files and SQLite fixtures; observer saw retained paused history and canonical assertions passed after removal. No development extension path. |
| Owned processes | All three smoke phases exited successfully, with no forced cleanup and zero remaining owned processes. |
| Packaged setup guide | Present and hash-verified; command invocation subsequently passed in the Marketplace acquisition check below. Visual usability remains unassessed. |

Private evidence is indexed by `artifacts/beta-delivery/marketplace-installed-results.json`
(SHA-256 `514cedb2e9db978a867cd391b935e9fb86ea7c2331f50055b939ba1904bb9793`).
It binds the actual result/supervision files and archive verification. The existing
qualification executable was reused only after checking its compile receipt,
executable hash and unchanged fixture/harness source hashes. Production artifacts
were newly built. Raw logs and private fixture roots are not publication assets.

The editor/package suite passed 164 tests. Review corrected one remaining old
publisher selector and a relative Marketplace README link. A local broad harness
attempt failed on the newly added ADR count and a Node junction; later diagnostic
reruns encountered sandbox `.git` write restrictions. The ADR inventory was corrected and checked; affected
diagnostic reruns with physical pinned Node and normal filesystem access produced
276 passes and one optional PTY skip, recorded in tool results. The original
failed local manifest remains retained; these diagnostics are not a replacement
full fast-harness run. Both PR and exact-main hosted gates passed.

The packet remains `selection_status=pass`, `pipeline_status=incomplete` and
`status=qualification-required`. Its later native-boundaries, installed-native
and installed-editor pipeline stages remain **not run**; focused local checks do
not relabel them. Clean-host, paid/live task, reviewed-edit, distinct-build
upgrade/rollback and full qualification remain open. Zero provider calls ran.

## Owner upload (completed)

1. Sign into [Manage Ioka extensions](https://marketplace.visualstudio.com/manage/publishers/iokaio)
   using the account that owns the publisher.
2. Choose **New extension → Visual Studio Code**.
3. Select the verified `.vsix` from the completed handoff. Do not upload the native
   setup EXE, ZIP, entire candidate packet or the older publisher's VSIX.
4. Confirm the publisher/name/version and pre-release designation, then submit.
5. Wait for Marketplace validation to succeed. If validation fails, retain the
   error and fix/rebuild the package; do not relabel an unverified package.

Manual dashboard upload does not require configuring a CLI publishing PAT.
[Microsoft publishing instructions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#publish-an-extension)

## After upload

The public listing is
[iokaio.vcp-local](https://marketplace.visualstudio.com/items?itemName=iokaio.vcp-local),
confirmed available after the owner's upload.

The October 1 acquisition check installed `iokaio.vcp-local@0.2.1 --pre-release`
directly from Marketplace into a fresh VS Code 1.138.0 profile on Windows x64.
Gallery metadata confirmed publisher, version, target and pre-release designation;
all 60 installed payload files matched the uploaded VSIX receipt. The setup-guide
command opened its preview tab. The extension connected and disconnected as an
observer to the matching production engine and one synthetic Files fixture with
retained paused history. Fixture bytes were preserved except the excluded
`owner.lock`; temporary extension/native installations were removed. Supervision
completed naturally, with no forced cleanup and zero remaining owned processes.
No provider calls ran.

Private evidence is indexed by `artifacts/beta-delivery/published-marketplace-index.json`
and hashed in `artifacts/beta-delivery/published-marketplace-evidence.json`.
The report SHA-256 is
`cd0de968e8002a9e4010853873be05d51da5f5945c4d89eb092305872cea4ef8`.
The initial disposable assertion failed because pinned Code records gallery
pre-release metadata in `extensions.json`, not package `__metadata`. That attempt
is retained; correcting the assertion and retrying passed without changing the
published package. This check covers Files acquisition on a developer workstation,
not SQLite acquisition, visual usability, a first useful task or full qualification.

The downloads-page renderer now includes the live Marketplace listing alongside
the matching direct VSIX download. Publishing this page reuses the existing
candidate and release assets without rebuilding the package.

For users of the earlier `vcp.vcp-local`, disconnect and uninstall the old
extension first. Retain native data and User settings and reconnect under the new
identity. The publisher change does not automatically migrate extension-local
state or update the native engine.

Full qualification, clean-host support, native signing and paid provider testing
remain separate. HTTPS on the downloads site is transport encryption, not native
code signing or proof of qualification.
