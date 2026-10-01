# ADR-074 — GitHub beta downloads

Date: October 1, 2026. Status: accepted by explicit owner direction; publication
and site deployment pending.
Owning items: BETA-08/BETA-11 in the [release plan](../release-planning/00-release-plan.md).

## Context and authority

After the [manual-testing handoff](../release-planning/05-manual-candidate.md), the
owner explicitly authorized downloadable unsigned beta artifacts and selected
GitHub Releases plus GitHub Pages. This is new, limited publication authority;
[ADR-073](073-manual-testing-candidate.md) and its historical handoff did not
authorize publication and remain unchanged.

The owner abandoned the Azure proposal. Discovery found existing resources
`wfprodist`, `wfpro` and `install` in `rg-wfpro-dist`; this decision authorizes no
changes to those resources. Reusing GitHub avoids introducing an Azure download
service and its additional infrastructure and operating costs. It does not claim
that every GitHub operation is free or authorize unrelated spending.

## Decision

Publish selected, already verified candidate bytes as a GitHub **prerelease** in
`iokaio/vcp`. The manually dispatched `publish-beta.yml` workflow runs from `main`
and requires `candidate_run_id`, `candidate_attempt` and `expected_pair_id`.
It admits a successful `beta-candidate.yml` attempt from `main`, verifies the
retained artifact checksums and exact pair, and performs no production rebuild.

Use a unique tag `v<native-version>-<first-12-characters-of-pair-id>`, retaining
the full pair ID and original source in the publication record. Upload to a draft,
verify the complete selected asset bytes, then publish. Existing assets must not
be overwritten to make a retry succeed. Any changed artifact requires a new
verified pair and publication identity.

After release publication, deploy a Pages download index at
`https://downloads.ioka.io`. It links the published release assets and identifies
the latest published beta explicitly. GitHub's latest-release endpoint excludes
prereleases, so it cannot select this beta channel.
([GitHub release API](https://docs.github.com/en/rest/releases/releases#get-the-latest-release))

Configure the custom domain on GitHub before the owner creates the DNS CNAME
`downloads` → `iokaio.github.io`. GitHub-managed HTTPS follows DNS validation.
The [publication guide](../development/beta-publication.md) records the operating
steps and separate release, Pages, DNS and HTTPS status.

## Limits

Publication authorizes access to a clearly labeled experimental beta; it does
not complete the full qualification matrix, establish clean-host support or
approve a stable release. Preserve the selected packet's `qualification-required`
and incomplete pipeline status, the focused installed observations, known
failures and unrun checks. Historical failed runs remain failed.

Unsigned disposition, permissions, accounting, credential handling and data
preservation remain unchanged. Signing, Marketplace publication, paid provider
calls and broader release acceptance require their own authority. Publish only
reviewed public artifacts and records, never private fixture histories, profiles,
credentials or raw local diagnostics.
