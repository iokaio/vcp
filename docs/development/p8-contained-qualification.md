# P8 qualification without workstation test-machine access

Status: retained test design, not execution evidence. Owning items: P8-01 through
P8-05, now closed by [owner-directed closure](../adr/042-owner-directed-p8-closure.md). This design preserves the
work prepared before closure; its unexecuted scenarios no longer block P9.
The historical qualification results and unknown liabilities remain unchanged.

## Immutable inputs and independent observations

Use the corrected production package recorded in
[the allowance-package report](../evaluations/p8-allowance-package-2026-09-23.md):
ZIP SHA-256 `d6a5daaf2e4efd931bb6988ecb7f4800bdff070f625955157db41d6675c607f5`,
executable SHA-256 `b8d8ab89a5580a1b726144137c38c2bc6898da01964c4fd6f155b2e474ea2f40`.
Bind every report to the package, runner, fixture and model-asset hashes. A rebuild
is a different candidate until its bytes are checked; source equality alone is
insufficient. Do not repeat the already-passing distribution campaign locally.

Separate test supervision from the product. The supervisor observes process
termination, network delivery, volume capacity and original fixture hashes.
The product's own success message is never the sole oracle. Keep failed runs.

## Provider uncertainty: retain the hold, isolate subsequent accounting

The failed renewal is immutable and consumed. Preserve its full $12 campaign
reservation and $6.397576 canonical unresolved liability; do not infer zero cost
from a transport failure. Its billing reconciliation is a separate accounting
case, not a prerequisite imposed by the current campaign admission algorithm.

The recorded campaign has $3.637947 settled and $78.127269 reserved, leaving
$18.234784. A distinct $12 qualification can technically fit without releasing
any historical hold. A new proposal is a separate paid execution; this design
does not turn the earlier no-retry authorization into permission for another run.

Before a separately authorized execution, run the existing renewal and owner
coordinator controls without credentials or inference. The campaign regression in
`src/tests/contracts/p8-profile-renewal.test.cjs` now exercises the retained totals,
successful synthetic settlement, concurrent-launch refusal and the exact owner
admission boundary. The complete campaign schedule is:

1. Preserve every historical row and reserve the new $12 atomically.
2. Reject concurrent owner admission while the qualification is running.
3. For timeout, missing usage, contradictory accounting or unreaped process,
   retain the new reservation and stop. Replaying its claim must fail.
4. For a separately witnessed successful two-request probe, settle only its
   actual charge. Join authenticated generation receipts before promoting the
   provider profile; synthetic receipts cannot qualify the real endpoint.
5. Run the six frozen owner slots sequentially, reserving $8 before each launch.
   The $48 cohort limit is not a simultaneous $48 reservation. Admit a slot only
   when new qualification cost plus preceding new task costs is at most
   $10.234784. Test both sides of that boundary and unchanged campaign bytes
   after refusal. Any unknown task charge stops the cohort.

This can remove unnecessary dependence on resolving a historical charge, but it
does not prove the charge, guarantee sufficient headroom, or replace live task
quality with deterministic protocol fixtures.

## Disposable Windows environment

Use the existing standard GitHub-hosted Windows runner path for this public
repository, not workstation Hyper-V or a new paid runner class. Add an opt-in
package-only job with a 45-minute deadline, one run at a time and read-only
repository permissions. First qualify its VHD and firewall capabilities using
synthetic data; an unavailable primitive fails preflight before any product run.

Package transfer must be explicit: consume an Actions artifact from an identified
run, verify the expected ZIP hash before extraction, and verify the extracted
inventory. The local ZIP is not currently an uploaded Actions artifact. Do not
publish a release, commit the binary, accept an arbitrary download URL, or rebuild
silently to get around this transfer step. Provisioning that immutable artifact
is an implementation prerequisite of the job.

Run packaged checks under a new standard-user profile, with an empty VCP data
root and minimal environment. Record OS build, processor features, RAM,
filesystem and preinstalled dependencies. This qualifies the actual hosted
Windows Server image; it is not evidence for an untested Windows 11 image or a
bare OS without the runner's preinstalled software.

## Real filesystem exhaustion on an owned virtual disk

Create a fixed-size disposable VHD/VHDX of at most 1 GiB inside the run's owned
temporary directory. Verify sufficient backing-volume reserve before creation.
Bind the disk identity, image path and volume identity to a random run marker;
never select a disk by drive letter alone. All formatting and filling must be
restricted to that newly created disk. Keep reports on the backing volume.

For each store backend, seed acknowledged records and independent digests, then
fill only the test volume using allocated, non-sparse bytes. Exercise canonical
writes, artifact finalization, snapshot publication and restore staging as
separate cases. Require a real storage-full failure; permission denial, timeout,
synthetic injection or a log containing the words "disk full" does not pass.

Assert no false acknowledgement, intact previously acknowledged records, no
partial artifact/generation activation, retained recovery state and no duplicate
effect. Remove only the owned filler, reopen in a fresh process, and verify
recovery/retry through the same product path. Repeat with termination during the
failed operation where an independent marker can establish the boundary.

Always stop the owned process tree and detach the exact created disk. Retain
cleanup failures. Full NTFS-volume exhaustion on a VHD is real filesystem-full
evidence; it does not establish physical-device failure or power-loss durability.

## Network denial without AppContainer ancestor changes

Preload and verify pinned local model assets. On the disposable runner, apply
outbound firewall denial to the exact product executable and every executable
in the declared child closure. Do not disable networking for the Actions agent
or change workstation firewall rules or ancestor ACLs.

Use independent positive/negative/positive traffic controls around the blocked
interval. A different canary executable by itself cannot prove the product's
path-based firewall rule: verify each product path's installed rule and exercise
a bounded connection attempt from that path, or use an independently observed
network enforcement mechanism that covers the entire job. Never make a billable
provider POST merely to test denial.

Under verified denial, run production local-memory build, query, reopen and
scope-isolation cases on both stores. Observe the destination listener and
network enforcement events as well as product results. Missing denial evidence,
unexpected children or an untested executable path fails the case. Remove only
the run's own rules in unconditional cleanup.

## Resource envelope and owner acceptance

Measure the actual hosted runner first. Declare its tested hardware envelope
before scoring rather than borrowing the workstation's measurements. Additional
whole-tree Job Object memory/CPU limits provide bounded stress cases: assign the
process before first execution, prevent child escape, and record effective limits,
peak usage, wall time and recovery after limit refusal. These are resource-limit
tests, not a claim of equivalent physical minimum hardware.

Use the unchanged held-out owner fixtures, graders and predeclared thresholds.
Synthetic providers are appropriate for deterministic failure paths; the six
owner tasks still need actual model outputs and separate usefulness, correctness
and architecture-fit review. Any later full release qualification joins those results with the
measured support envelope. A narrowed support claim must be explicit and cannot
silently mark an untested OS or physical-hardware row passed.

## References

Local verification of this design's accounting boundary: all 21 tests passed in
`node --test src/tests/contracts/p8-profile-renewal.test.cjs src/tests/contracts/p805-owner-runner.test.cjs`.
No provider requests, real campaign mutations or infrastructure tests were run.

- Existing runners and their inputs: [P8 distribution](p8-distribution.md).
- Campaign coordinators: `scripts/evals/p8-profile-renewal.cjs` and
  `scripts/evals/p805-owner-runner.cjs`.
- Existing CI: `.github/workflows/ci.yml`.
- [GitHub-hosted runner isolation, privileges and specifications](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
- [Microsoft virtual disk management](https://learn.microsoft.com/en-us/windows-server/storage/disk-management/manage-virtual-hard-disks).
- [Microsoft Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
