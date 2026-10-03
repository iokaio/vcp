# Publish beta downloads

## Current signed beta publication

The October 3 CLI scenario fixes and complete test plans were merged in
[PR #356](https://github.com/iokaio/vcp/pull/356). The owner authorized the next
synchronized build and release. Native, installer, SDK and VSIX versions were
incremented to `0.2.5` before compilation, with lockfiles and generated provenance.
[Main Delivery checks](https://github.com/iokaio/vcp/actions/runs/37140144115)
passed, followed by [candidate 37140357261/1](https://github.com/iokaio/vcp/actions/runs/37140357261/attempts/1)
through the signed `pair` checkpoint.
[Publication 37143014091](https://github.com/iokaio/vcp/actions/runs/37143014091)
passed Release and Pages, publishing the unchanged candidate bytes as
[0.2.5](https://github.com/iokaio/vcp/releases/tag/v0.2.5-26e08e9be854).

- Reviewed source: `59eaea1f3bad310269d4b6cb2120a59f5547515a`.
- Pair: `26e08e9be854c0769159bb1cbc8c1fbef240bcbe9b10184c76b04d04b2c3ad78`.
- Changes and failure evidence: [A run investigation](../test-plans/run-review-20261003-092744.md).
- Validation before packaging: all 15 offline scenario regression scripts, 167 CLI
  library tests, five native lifecycle verification tests, 35 SDK tests, 165 extension
  tests, the full fast suite and all four ordinary CI jobs.
- Packet admission, actual engine/launcher versions, installer ProductVersion,
  VSIX manifest and bundled SDK all passed at `0.2.5`; the VS Code range remains
  `^1.138.0`. All four timestamped Ioka LLC Authenticode signatures passed.

Local evidence: `artifacts/local-candidate/signed-0.2.5-37140357261/local-verification/local-verification.json`.
The original packet and prepared public assets are retained in sibling `packet/`
and `verified/` directories.
The signed binary's `setup provider-refresh --help` and `inspect-bundle --help`
both exited 0 in an isolated environment with inherited provider credentials cleared.
Literal commands and result paths are in `local-verification/command-verification/vcp-commands.log`;
structured results and retained output are alongside it. No inference or installation ran.

Public verification passed over ordinary HTTPS: all five GitHub assets matched
the prepared hashes and sizes, `downloads.ioka.io/latest.json` matched the release
manifest, and the homepage matched the rendered release page exactly. Evidence:
`artifacts/beta-delivery/public-verification-37143014091/verification.json` and
`artifacts/beta-delivery/public-verification-37143014091/asset-verification.json`.

This remains a limited beta: later native-boundaries, installed-native and
installed-editor pipeline stages were explicitly unrun. A full paid scenario A
rerun, clean-host qualification and Marketplace publication are not established
by this candidate. Existing account configuration and installed VCP were preserved.

## Historical signed 0.2.4 publication

The owner authorized building, signing and publishing synchronized `0.2.4`
artifacts, including the VS Code compatibility correction. The
[candidate run 37096239569](https://github.com/iokaio/vcp/actions/runs/37096239569/attempts/1)
completed successfully through `pair`.
[Publication run 37097991085](https://github.com/iokaio/vcp/actions/runs/37097991085)
completed successfully with both Release and Pages passing. The unchanged
candidate bytes were published without rebuilding or re-signing.

- [Published prerelease](https://github.com/iokaio/vcp/releases/tag/v0.2.4-1e791436ab85):
  synchronized native, installer, SDK and VSIX `0.2.4`.
- Source: `53e4d16667eed318208037b63f0f77db804c57b5`.
- Pair: `1e791436ab850192fba5e4c58f0c294db584d1a3f16d6630d80709a2618ffe55`.
- Actual engine, launcher, installer ProductVersion, VSIX manifest and bundled
  SDK versions all verified as `0.2.4`; extension ID `iokaio.vcp`, target
  `win32-x64`, VS Code engine range `^1.138.0`.
- Engine, launcher, setup and uninstaller passed Ioka LLC Authenticode
  verification, including the pinned identity EKU and timestamps.
- Full candidate-packet admission and isolated offline CLI smoke checks passed.
  The exact VSIX installed through the official CLI and activated on VS Code
  `1.138.0` and `1.140.0`; all 59 installed payload files matched the package.
  Installed `package.json` was compared after removing editor-added `__metadata`;
  the other payload files matched by hash.

The candidate remains a limited internal beta requiring qualification. These
editor checks do not complete the later native-boundaries, installed-native or
installed-editor pipeline gates, establish clean-host support, or prove a live
provider task. Successful Marketplace publication and exact public package
verification are recorded separately
in [Marketplace publication](marketplace-publication.md).

At publication, [downloads.ioka.io](https://downloads.ioka.io/) served this release over HTTPS
with normal certificate validation. All five public assets matched the prepared
SHA-256 hashes and sizes; public HTML matched the renderer exactly, and
`latest.json` matched the release manifest. Prior releases remain unchanged.
Public evidence: `artifacts/beta-delivery/public-verification-37097991085/verification.json`
and `artifacts/beta-delivery/public-verification-37097991085/asset-verification.json`.

Local verification: `artifacts/local-candidate/signed-0.2.4-37096239569/local-verification/local-verification.json`.
The admitted packet and prepared public assets are retained in that candidate's
`packet/` and `verified/` sibling directories. Editor evidence is retained in
`artifacts/local-candidate/editor024-final-1.138.0-retry1/verification.json` and
`artifacts/local-candidate/editor024-final-1.140.0/verification.json`.

## Historical signed 0.2.3 publication

On October 2, 2026 the owner authorized publishing the verified `0.2.3` artifacts
to downloads.ioka.io. BETA-11's limited-beta publication completed in
[run 37094167354](https://github.com/iokaio/vcp/actions/runs/37094167354), with both
Release and Pages jobs passing. The existing candidate bytes were published
without rebuilding or re-signing.

- [Published prerelease](https://github.com/iokaio/vcp/releases/tag/v0.2.3-f736a0d2c339):
  native, Windows installer, SDK and VSIX all `0.2.3`; extension ID `iokaio.vcp`.
- Source: `0866686492cd5e1df3378f6c208fd49b610d990e`.
- Pair: `f736a0d2c3394d4bda5c43c970c3a20a1aa40e3266979b1ebde484bee78a3a32`.
- Candidate: [37092293430, attempt 1](https://github.com/iokaio/vcp/actions/runs/37092293430/attempts/1),
  successful `pair` checkpoint. Later installed/native and clean-host qualification
  remain unrun for this exact pair; publication does not complete those gates.
- The engine, launcher, setup and uninstaller passed Windows signature validation
  for Ioka LLC, including the pinned identity EKU and timestamps. Actual native,
  installer, VSIX manifest and bundled SDK versions were verified as `0.2.3`.

[downloads.ioka.io](https://downloads.ioka.io/) served this release over HTTPS
with normal certificate validation. All five public assets were downloaded and
matched the prepared SHA-256 hashes and sizes. The public HTML exactly matched
the release renderer, and `latest.json` matched the published manifest.
Earlier release assets were retained. Marketplace publication is separate and
has not been performed for `0.2.3`.

Local evidence: `artifacts/beta-delivery/public-verification-37094167354/` and
`artifacts/local-candidate/signed-0.2.3-37092293430/local-verification.json`.

## Historical signed beta.2 publication

On October 1, 2026 the owner authorized publishing the matching signed artifacts
while manually uploading the renamed Marketplace VSIX. BETA-11's limited-beta
publication completed in [run 36943259079](https://github.com/iokaio/vcp/actions/runs/36943259079),
with both Release and Pages jobs passing.

- [Published prerelease](https://github.com/iokaio/vcp/releases/tag/v0.2.0-beta.2-e9e8ba52684a):
  native `0.2.0-beta.2`, SDK/VSIX `0.2.2`, extension ID `iokaio.vcp`.
- Source: `a4a5b93b0077eb8f6a63f005379c09ce0ff4773e`.
- Pair: `e9e8ba52684a67b726787911082ec683ed8fba33c507e2c496bf9765ff626984`.
- Candidate: [36938973679, attempt 1](https://github.com/iokaio/vcp/actions/runs/36938973679/attempts/1),
  successful `pair` checkpoint; later installed/native and clean-host qualification
  remain unrun for this exact pair. Publication does not complete those gates.
- The engine, launcher, setup and uninstaller passed Windows signature validation
  for Ioka LLC, including the pinned identity EKU and timestamps.

[downloads.ioka.io](https://downloads.ioka.io/) then served this release. All five
public assets were downloaded and matched the prepared SHA-256 hashes and sizes.
The public HTML exactly matched the release renderer; `latest.json` matched the
published manifest. HTTPS returned 200 with normal certificate validation and
HTTP returned 301 to HTTPS. The page links to `iokaio.vcp`; Marketplace upload is
the owner's separate action. Existing beta.1 release assets were retained.

Local evidence: `artifacts/beta-delivery/public-verification-36943259079/`.
The verified manual upload remains
`artifacts/marketplace-upload/iokaio-vcp-0.2.2/assets/vcp-0.2.2-win32-x64.vsix`.

## Historical unsigned beta publications

Status: **beta prereleases published, download page deployed and HTTPS enforced** on October 1.
The then-latest published pair was the [Marketplace-ready handoff](marketplace-publication.md),
`9694d258aeb5`, built from reviewed source `99de062d32c4c0fe20ddae119408e953e8f7243b`.
[Publication run 36900026024](https://github.com/iokaio/vcp/actions/runs/36900026024)
published its unchanged verified bytes and updated Pages. The owner subsequently
uploaded the extension; [VCP Local is live on Marketplace](https://marketplace.visualstudio.com/items?itemName=iokaio.vcp-local).
The initial publication below is retained
as history; its release assets have not been replaced.

[Publication run 36888330245](https://github.com/iokaio/vcp/actions/runs/36888330245)
passed both release and Pages jobs after [PR #326](https://github.com/iokaio/vcp/pull/326)
merged with all ordinary checks passing. The
[initial prerelease](https://github.com/iokaio/vcp/releases/tag/v0.2.0-beta.1-1dba45922e0c)
retains source `f81b2c5062dad1f8d13d72ffb5fb8006eeea9229` and pair
`1dba45922e0c34a04880f2ed4c5a07a65882b809559104aee70506b88838bcc7`.
All five public asset sizes and GitHub SHA-256 digests match preparation; the
three binary hashes are unchanged from the manual-testing handoff. No rebuild ran.

Pages uses workflow deployment with `cname=downloads.ioka.io` and an environment
restricted to `main`. The owner's CNAME resolves at authoritative and public DNS;
GitHub reports the domain valid and HTTPS-eligible. **Custom-domain TLS and HTTPS
enforcement are complete** as of October 1 at 16:44 UTC. Re-saving the domain did
not start issuance; removing and immediately restoring the same Pages custom
domain restarted provisioning successfully. GitHub reports the certificate
approved for `downloads.ioka.io`, expiring December 30, 2026. An ordinary HTTPS
request passed certificate validation and returned 200; HTTP returned 301 to the
HTTPS URL. No DNS or unrelated hosting resources were changed. GitHub manages
the certificate; it does not sign the native downloads.

Validation: 17 focused contracts, actual 88-file candidate preparation, repository
link checks, YAML parsing, desktop/mobile layout and keyboard checks passed.
Independent review found no blocker. Public assets contain no raw pipeline logs
or private qualification fixtures.

[ADR-074](../adr/074-github-beta-downloads.md) authorizes this limited publication.
The [manual candidate](../release-planning/05-manual-candidate.md) identifies the
existing verified bytes and focused results. Full qualification, stable-release
acceptance, signing and paid provider testing remain separate. Subsequent
Marketplace publication and checks are recorded in the [Marketplace handoff](marketplace-publication.md).

## Select and publish the existing pair

Use [publish-beta.yml](../../.github/workflows/publish-beta.yml) through
**Actions → Publish beta downloads → Run workflow**, selecting `main`, or its
filename with the GitHub CLI:

```powershell
gh workflow run publish-beta.yml --repo iokaio/vcp --ref main `
  -f candidate_run_id=36868151228 -f candidate_attempt=2 `
  -f expected_pair_id=1dba45922e0c34a04880f2ed4c5a07a65882b809559104aee70506b88838bcc7
```

Those values select the recorded October 1 pair; a later publication must name
its own reviewed run, attempt and full pair ID. The workflow validates a successful
`beta-candidate.yml` attempt originating from `main`, verifies the unchanged
packet and native/setup/VSIX bindings, and publishes without recompilation.
A successful `pair` checkpoint may retain `pipeline_status=incomplete` and
`status=qualification-required`; publication does not turn its unrun stages into
passes or relax the full-qualification runners.

The release tag is `v<native-version>-<pair-id-first-12>`, for example
`v0.2.0-beta.1-1dba45922e0c`. The full identity belongs in the published metadata
and notes. Upload the three binary artifacts, `release.json` and their
`SHA256SUMS` to a draft; verify their hashes before making the prerelease public.
The publication checksum list describes those public assets, independently of
the original full candidate packet's checksum list. Existing assets are never
overwritten. On failure, retain the draft and diagnostics and inspect the existing
identity before retrying; do not delete or replace artifacts merely to bypass a
collision. A Pages failure after release publication does not undo that release.

The Pages index is deployed after publication and links directly to GitHub
Release assets. It presents the latest **published beta** and its exact version,
pair, hashes and limitations. Do not use `/releases/latest` for this selection:
GitHub excludes prereleases from that endpoint.
([GitHub release API](https://docs.github.com/en/rest/releases/releases#get-the-latest-release))

## Configure downloads.ioka.io

The GitHub-side configuration is complete: `iokaio/vcp` under **Settings → Pages**
uses GitHub Actions and custom domain `downloads.ioka.io`. Keep this configuration
in place before changing DNS. With Actions deployment, a repository `CNAME` file
does not replace that setting.
([GitHub custom-domain instructions](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site))

The owner then adds this record at the DNS provider for `ioka.io`:

| Type | Host/name | Target/value |
| --- | --- | --- |
| CNAME | `downloads` | `iokaio.github.io` |

Use the hostname alone, without `https://`, `/vcp` or a release path. Some DNS
interfaces expect the full host `downloads.ioka.io`. Do not change unrelated
apex, mail or application records. Resolve any conflicting record at this exact
host before adding the CNAME. Verify the result with:

```powershell
Resolve-DnsName downloads.ioka.io -Type CNAME
```

After DNS validation, allow GitHub's managed certificate provisioning to finish
and enable **Enforce HTTPS** in Pages settings. Verify
`https://downloads.ioka.io` and its selected download links before marking the
custom-domain handoff complete. Certificate readiness may lag deployment or DNS;
record those states separately.
([GitHub HTTPS instructions](https://docs.github.com/en/pages/getting-started-with-github-pages/securing-your-github-pages-site-with-https))

The Azure alternative was abandoned. Existing `wfprodist`/`wfpro`/`install`
resources in `rg-wfpro-dist` are unchanged; this workflow needs no Azure resource
creation, storage credentials or Front Door configuration.
