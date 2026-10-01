# First Marketplace upload

Status: **verified package ready for the owner's first manual upload** on October 1,
2026. No Marketplace upload has been performed.
Contract: [ADR-075](../adr/075-marketplace-manual-upload.md), BETA-07/BETA-08/BETA-11.
The owner performs the first upload manually. The older GitHub release's
`vcp.vcp-local` package is not the upload candidate for publisher `iokaio`.

## Select the verified package

Upload **`vcp-local-0.2.1-win32-x64.vsix`** from the
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
uses GitHub-managed HTTPS with HTTP-to-HTTPS enforcement; Marketplace publication
is still pending the owner upload below.

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
| Packaged setup guide | Present and hash-verified; guide-command invocation and visual usability were not run for this pair. |

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

## Owner upload

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

The expected listing is
[iokaio.vcp-local](https://marketplace.visualstudio.com/items?itemName=iokaio.vcp-local);
its availability is not established before the manual upload.

Install the pre-release from Marketplace in an isolated VS Code 1.138.0 profile on
Windows x64. Verify the displayed publisher/version, setup-guide command and
connection to the selected matching native engine. Record Marketplace results
separately from local VSIX installed checks. Then add the live Marketplace link to
the downloads page; do not advertise it as available while upload is pending.

For users of the earlier `vcp.vcp-local`, disconnect and uninstall the old
extension first. Retain native data and User settings and reconnect under the new
identity. The publisher change does not automatically migrate extension-local
state or update the native engine.

Full qualification, clean-host support, native signing and paid provider testing
remain separate. HTTPS on the downloads site is transport encryption, not native
code signing or proof of qualification.
