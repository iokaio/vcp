# Publish an unsigned beta download

Status: **beta prerelease published and download page deployed** on October 1.
[Publication run 36888330245](https://github.com/iokaio/vcp/actions/runs/36888330245)
passed both release and Pages jobs after [PR #326](https://github.com/iokaio/vcp/pull/326)
merged with all ordinary checks passing. The
[published prerelease](https://github.com/iokaio/vcp/releases/tag/v0.2.0-beta.1-1dba45922e0c)
retains source `f81b2c5062dad1f8d13d72ffb5fb8006eeea9229` and pair
`1dba45922e0c34a04880f2ed4c5a07a65882b809559104aee70506b88838bcc7`.
All five public asset sizes and GitHub SHA-256 digests match preparation; the
three binary hashes are unchanged from the manual-testing handoff. No rebuild ran.

Pages uses workflow deployment with `cname=downloads.ioka.io` and an environment
restricted to `main`. The owner's CNAME resolves at authoritative and public DNS;
GitHub reports the domain valid and HTTPS-eligible. The page returns HTTP 200 with
the selected pair and correct HTTPS GitHub download links. **Custom-domain TLS
provisioning and HTTPS enforcement remain pending**; a normal HTTPS request has
not yet passed certificate validation. GitHub release downloads already use HTTPS.

Validation: 17 focused contracts, actual 88-file candidate preparation, repository
link checks, YAML parsing, desktop/mobile layout and keyboard checks passed.
Independent review found no blocker. Public assets contain no raw pipeline logs
or private qualification fixtures.

[ADR-074](../adr/074-github-beta-downloads.md) authorizes this limited publication.
The [manual candidate](../release-planning/05-manual-candidate.md) identifies the
existing verified bytes and focused results. Full qualification, stable-release
acceptance, signing, Marketplace upload and paid provider testing remain separate.

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
