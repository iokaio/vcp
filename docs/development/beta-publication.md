# Publish beta downloads

## Current signed beta publication

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

[downloads.ioka.io](https://downloads.ioka.io/) serves this release over HTTPS
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
