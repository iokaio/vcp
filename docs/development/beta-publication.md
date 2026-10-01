# Publish an unsigned beta download

Status: **GitHub Pages configured; release publication and site deployment
pending**. On October 1, GitHub configuration was verified as `build_type=workflow`
with `cname=downloads.ioka.io`; the `github-pages` environment permits deployments
from `main` only. The owner created the CNAME and local DNS resolves
`downloads.ioka.io` to `iokaio.github.io`. The certificate is not yet provisioned
and HTTPS enforcement is off pending GitHub validation. Record the actual release URL, workflow
run, Pages deployment and domain results here when observed.

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
