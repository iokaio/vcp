# ADR-076 — Signed beta and final-artifact qualification

Date: October 1, 2026. Status: accepted by explicit owner direction; implementation in progress.
Owning items: BETA-04/BETA-06/BETA-08/BETA-09/BETA-10/BETA-11 in the [release plan](../release-planning/00-release-plan.md).

## Decision and authority

The owner requested completion of the remaining release plan, including reuse of
the existing Azure signing service, signed native artifacts, final qualification,
guided manual work, owner review and publication of the resulting verified pair.
This extends the bounded work authorized by [ADR-075](075-marketplace-manual-upload.md).
It does not rewrite the published unsigned candidate or its historical evidence.

The owner subsequently selected **Qwen 3.8** and authorized **$40 total in new
provider spending**, and confirmed a clean Windows machine is available for a
guided test. Select the current exact OpenRouter model `qwen/qwen3.8-max-0902`
and endpoint `alibaba` after checking fresh catalog identity and eligibility.
Preserve prior campaign costs and unresolved liabilities separately. New setup,
task, child and evaluator calls share the new $40 ceiling; per-command budgets
are subordinate limits, not additional allowances. Reserve admitted exposure
before launch and retain unresolved charges until reconciled. Do not silently
change models, retry failed inference or increase the total ceiling.

Credentials remain in the supported process or extension credential boundary.
No credential belongs in a command argument, committed file, profile or evidence
packet. Provider metadata expires after twelve hours; refresh only when needed
and within the remaining authorized budget.

## Signing identity and authentication

Reuse Azure Artifact Signing account `ioka-llc-signing` in West US 2 and its
existing Public Trust profile `WritingForgePro`, through
`https://wus2.codesigning.azure.net/`. Require publisher subject
`CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US` and profile identity EKU
`1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669`.
The profile name is an existing Azure resource name; the application remains VCP.

Use GitHub OIDC authentication with an exact trusted repository/ref or protected
environment subject. Grant the workflow identity only the Certificate Profile
Signer role at this certificate profile's scope. Do not add a client secret,
export a private signing key, grant subscription-wide signing access, or expose
signing credentials to package tests. Validate the selected Azure account,
profile and identity before signing. Renew Azure identity validation if needed;
a successful resource lookup alone does not prove a signing operation works.

Pin Microsoft Trusted Signing Client `1.0.95` and Windows SDK BuildTools
`10.0.26100.4188` packages by SHA-256, verify their extracted dependency closure,
and require the x64 .NET 8 runtime on the Windows build runner. These signing
tools are build prerequisites, not application payload or user prerequisites.

## Artifact and provenance contract

Advance native to `0.2.0-beta.2` and SDK/VSIX to numeric `0.2.2`, retaining the
Marketplace pre-release flag, publisher `iokaio`, Windows x64 target and pinned
VS Code `1.138.0`. These are new artifact identities. Preserve public beta.1 and
its checksums; do not replace published assets in place.

Keep the strict unsigned compiler output and original build receipt immutable.
Sign the production engine and stable launcher before packaging. Sign the Inno
uninstaller during setup construction and sign the final setup executable. Use
SHA-256 signatures and trusted timestamps; require a valid chain, the selected
publisher/profile identity and timestamps on every required executable. The
portable ZIP contains the signed engine. The ZIP and VSIX retain ordinary
whole-file checksums; website TLS and Marketplace signing cannot substitute for
these native signatures.

An explicit signing receipt binds each original executable hash to its signed
hash, signer/profile identity, timestamp, tools and verification result. Package
manifests bind the signed bytes while retaining the original build provenance.
Recompute final ZIP/setup/VSIX hashes and the pair identity after signing; never
edit an original compiler hash to make signed bytes appear compiler-produced.
Reject missing, invalid, mismatched or unstamped signatures rather than falling
back to unsigned artifacts. Preserve verification evidence with the release.

## Qualification and completion

Run the full selected candidate pipeline and applicable independent installed
tests on the final signed bytes. Use the prior published strict production pair
for genuine distinct-version upgrade/rollback observations where compatible.
Keep Files and SQLite variants and retained user-data/accounting preservation
explicit. Source tests and prior unsigned observations remain supporting evidence,
not substituted final-byte passes.

Prepare the clean-host package and exact walkthrough before asking the owner to
run it. Machine/environment inputs and human quality judgments that automation
cannot supply remain manual work. Independent-machine recovery, controlled
full-volume and production network-denial observations retain their own gates;
do not infer them from same-host fixtures or configuration. Only the owner can
accept the completed evidence packet; the instruction to proceed is not a
passing result for observations that have not run.

Update the current [implementation ledger](../release-planning/02-implementation-status.md)
and [acceptance map](../release-planning/03-acceptance-map.md) as evidence arrives.
Publish the verified signed pair after required checks and the factual owner
decision are recorded. A valid code signature authenticates the publisher; it
does not establish SmartScreen reputation, clean-host support or product quality.
