# ADR-075 — Ioka Marketplace pre-release handoff

Date: October 1, 2026. Status: accepted by explicit owner direction; preparation in progress.
Owning items: BETA-07/BETA-08/BETA-11 in the [release plan](../release-planning/00-release-plan.md).

## Decision and authority

The owner created Marketplace publisher `iokaio` and requested completion of the
publishing preparation, reserving the first Marketplace upload for manual action.
Prepare extension `iokaio.vcp-local`, its matching native candidate, listing,
focused installed evidence and exact upload instructions. Finish GitHub Pages
managed HTTPS and publish the verified replacement pair through the existing
GitHub prerelease/Pages workflow. Do not perform the Marketplace upload.

This extends [ADR-074](074-github-beta-downloads.md) for the new candidate and
Marketplace preparation. Preserve the earlier public release and historical
evidence. Marketplace installation and listing availability remain unrun until
the owner uploads and Marketplace processing succeeds. Do not present a future
listing link as an available installation route before then.

## Package and migration

Use publisher `iokaio`, name `vcp-local`, numeric version `0.2.1`, VSCE pre-release
metadata, Windows x64 target and exact VS Code `1.138.0` compatibility. This is the
first package for the new publisher identity. Native/SDK versions remain those
in the channel input. Strict source, build and final-byte pair checks still apply;
changed package bytes require a fresh reviewed paired build and new checksums.

The earlier `vcp.vcp-local` is a different extension identity. Users disconnect
and uninstall it before installing `iokaio.vcp-local`, retain native installation,
data and User settings, and reconnect explicitly. No automatic migration of
extension-local connection state is claimed. Marketplace extension updates do
not update the native engine or silently change its selected executable.

## Verification and limits

Run affected package/identity contracts, ordinary Delivery checks, the existing
pair checkpoint and focused exact-byte installed checks. Retain the
`qualification-required` disposition and unrun full qualification rows from
[ADR-073](073-manual-testing-candidate.md). No paid provider calls, broader
qualification infrastructure, native signing or certificate purchase is authorized.
GitHub-managed website TLS does not provide native executable signing.

The [Marketplace handoff](../development/marketplace-publication.md) records the
upload procedure and, when produced, the exact artifact and verification results.
