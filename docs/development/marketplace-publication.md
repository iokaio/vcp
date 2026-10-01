# First Marketplace upload

Status: preparation in progress; no Marketplace upload has been performed.
Contract: [ADR-075](../adr/075-marketplace-manual-upload.md), BETA-07/BETA-08/BETA-11.
The owner performs the first upload manually. The older GitHub release's
`vcp.vcp-local` package is not the upload candidate for publisher `iokaio`.

## Select the verified package

Use only the new reviewed candidate identified in the completed handoff record.
It must contain publisher `iokaio`, name `vcp-local`, version `0.2.1`, target
`win32-x64` and VSCE pre-release metadata. Match its SHA-256 to the new pair's
checksum record before upload. Filenames and versions alone do not distinguish
it from the earlier manual candidate.

Keep the matching native installer available at [VCP downloads](https://downloads.ioka.io/).
The extension contains the SDK and schema, not the native engine. Its README
describes installation, exact editor/platform scope, unsigned native binaries,
support and incomplete qualification.

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
