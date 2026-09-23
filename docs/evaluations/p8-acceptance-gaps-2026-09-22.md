# P8-01–04 acceptance reconciliation — 2026-09-22

Status: partial qualification; P8-01 through P8-04 remain open. This record maps
the [work-item contracts](../plan/15-integration-and-release.md) to committed
evidence. It does not add implementation requirements or waive existing ones.
P8-06 maintenance completion is separate from these release gates.

The subsequent [P8-02 boundary audit](p8-local-recovery-boundary-map-2026-09-22.md)
and [bounded local increment](p8-local-recovery-increment-2026-09-22.md) refine the
recovery gaps below with retained receipt mapping and selected new evidence.
The [remaining local recovery follow-up](p8-local-recovery-followup-2026-09-22.md)
then closes the selected local missing cases with forty process kills, bounded
storage-full injections, divergence and content-authority checks. An actual
pending cloud-hydration helper was interrupted and its ciphertext recovered.
These explicit fault-surface limits apply; they do not complete production or
independent-environment P8-02 qualification.

The [current-machine report](p8-local-qualification-2026-09-22.md) originally
recorded 44 passing matrix rows and two missing executable receipts. The
[bounded follow-up](p8-release-scorecard-2026-09-22.md#complete-receipts-for-the-two-remaining-executable-cases)
now records both MCP content conversation and malformed-native-executable
verification passing with complete command receipts. The aggregate has **46
passing executable rows and one owner-skipped independent-machine row**. This
does not establish full P8 acceptance or a passing 47-row campaign. Original
interrupted and failed receipts remain preserved.

The owner directed this continuation to skip machine handoff and validate on
the current workstation. Independent-machine recovery and a clean Windows
installation remain not run. Fresh local profiles and cross-backend restore
are useful evidence but cannot replace either environment. No additional
environment, model call or implementation is requested by this reconciliation.

## Evidence baseline

The original matrix package is an unsigned debug qualification candidate:
CLI SHA-256 `afdd9e0011601c059d82f6f1cc264b59e5a2627f20db36774c701ce5734f4bbd`,
ZIP SHA-256 `f4045381457ddc84e1c32ff4108622523dba1e6851d779628ed2eb42101064b7`.
The current-machine report binds source, build, package, native logs and the
bounded disposition. Only the history fixture changed between its two runs;
product and package identities matched. Earlier failures and interrupted
attempts remain recorded.

The [earlier native report](p8-native-qualification-2026-09-22.md) contributes
historical 21-row qualification, installer cases and CPU subsystem observations.
Its older binary/toolchain identities are not silently relabeled as current
package evidence. [Distribution operations](../development/p8-distribution.md)
document implemented behavior and limitations; documentation alone is not an
execution receipt. A gap below means the cited evidence does not establish the
full acceptance condition, not that the implementation is necessarily absent.

## P8-01 — Native Windows support matrix

| Acceptance | Observed proof | Remaining gap | Next action |
|---|---|---|---|
| Declare tested OS/architecture/filesystem/shell/terminal, installation, toolchain and runtime identities | Windows 11 build 26200, x64/NTFS, PowerShell 7.6.6, native ConPTY, explicit Rust 1.95/MSVC and exact package/model identities are recorded. | One workstation does not establish broader Windows or shell support, clean-OS independence, or minimum hardware. | Publish only the observed support envelope; identify required advertised combinations and attach evidence or explicit not-run disposition. Keep clean-OS coverage open. |
| Paths, locks/junctions, process trees, policy, missing dependencies and local model setup | Native policy/process tests; installer redirects, locks and preservation; Unicode/spaces exact-ZIP smoke and missing-profile diagnosis; existing ten pinned model files verified. | Existing asset verification does not demonstrate a fresh model download/setup or every missing-toolchain diagnostic on the final package. | Map each advertised dependency/setup behavior to its exact package case; run only unmapped required variants in a later bounded qualification. |
| Same-process pause/inspect/resume and separate console closure, with distinct acknowledgement and termination latency | Registered native pause/resume/hard-close cases and packaged long-check completion/pause pass; actual process termination is asserted. | Current reports do not provide a complete per-environment acknowledgement/stop/termination latency distribution or establish every packaged interactive combination. | Retain the native results; bind any advertised packaged interactive claims and latency measurements to explicit receipts. |
| Selected search/read, output decoding, helper diagnostics, review and M9 enabled-consumer refinements in packaged/delegation qualification | Packaged skills, long checks and MCP setup diagnostics pass; P7 provides separate feature/delegation evidence. | No complete current-package mapping for all selected refinements or M9 enabled consumers and their actual-delegation acceptance is established by the 46-row count. | Reconcile selected requirements against existing P7/P8 receipts first; list only missing packaged/consumer variants, without reopening deferred features. |
| U09 CPU resources and measured performance/support targets | Earlier CPU embedding processes report approximately 5.5–5.9 seconds and 220–233 MB sampled working set; current matrix exercises real embeddings/cache/resources/publication/query. | High-memory workstation subsystem results do not establish a minimum machine or full packaged end-to-end resource envelope. | Record current-package workload/resource distributions and disposition provisional performance claims before advertising them. |

## P8-02 — Recovery and portability campaign

The table below preserves the original release-level gaps. The subsequent
[finite follow-up schedule](p8-local-recovery-followup-2026-09-22.md) is the
current disposition for its selected boundaries: artifact seal/publication,
prune/cleanup, root/child model dispatch, MCP reply ownership, interrupted vault
copy following artifact owner loss, private staging/restore capacity errors,
offline divergence and hostile native/MCP content now have passing receipts.
The earlier increment already qualifies competing-root restore activation and
canonical-versus-derived corruption. No whole matrix was rerun. Remaining
environmental gaps are a physically full volume and qualification against the
production artifact; machine handoff stays owner-skipped. The cloud receipt now
chains the interrupted helper, exact ciphertext recovery and exact-package VCP
restore with complete source/history/artifact comparisons and fresh reopens.
It does not claim a nonzero partial cloud transfer or interruption while VCP
restore itself is active.

| Acceptance | Observed proof | Remaining gap | Next action |
|---|---|---|---|
| Current authorization, outside-root/configuration writes, junctions, owned cleanup, locked roots and ordinary granted edits | Native policy revalidation, both-store child/integration write interruption, independent child stopping, cleanup reference/receipt recovery and installer path/lock checks pass in their stated scopes. | These cases are not a complete mapping of the integrated native-tool, MCP and child boundary variants required by CR-10a/CC-08. | Join existing exact cases to each required boundary; qualify unmapped variants without broadening denials or prompts. |
| Real process termination at write/dispatch/publication/restore barriers, with acknowledgement/effect/history/accounting oracles and no replay | Store, snapshot, restore, child/integration/cleanup and generation-publication cases pass; sixteen generation-kill boundaries and local cross-backend encrypted restore are recorded. | Current reports do not establish the complete boundary × before/after × backend × root/child × seed schedule, including all artifact, vault-copy and restore-activation points. | Produce the finite coverage mapping from retained supervisors; identify exact absent points before selecting further runs. Do not rerun already-qualified cases solely to fill a table. |
| Disk exhaustion, canonical versus derived corruption, locked files, interrupted cloud hydration and offline divergence | Locked-path, selected recovery and key replay mechanisms have component evidence; current report preserves their limited scope. | A complete current-package campaign for disk exhaustion, corruption distinction, cloud hydration and offline divergence is not documented. | Specify the missing fault and expected durable outcome, then schedule bounded current-artifact cases; leave unavailable cloud variants not run. |
| Selected combined fault sequences, generation-lag/prune/restore visibility and validated-root activation | Individual pause/owner-loss/cleanup/publication cases, retained-vector restore and retention obligations pass. | Individual cases do not prove combined sequences or every lag/restore visibility and competing-root activation condition. | Map individual evidence, then predeclare a small required combined schedule with external markers; do not claim exhaustive interleavings. |

## P8-03 — Full-history and encryption review

| Acceptance | Observed proof | Remaining gap | Next action |
|---|---|---|---|
| Full ordinary bytes versus prompt tails, child history, provenance and explicit compaction/purge gaps | The [packaged follow-up](p8-history-security-followup-2026-09-22.md) passes exact MCP catalog/resource/prompt bytes, descriptors and event links through compaction, fresh-owner connection and identity checks, revoked resource access, purge gaps and no replay on both stores. Earlier oversized-output, child transcript/recovery and finding evidence remains valid. | These local debug receipts do not qualify the intended production artifact or satisfy latency acceptance. Startup cost grows materially with retained history in the measured sequence. | Preserve the bounded receipts and failed attempts; qualify production behavior and latency separately. |
| Exclude typed credentials/recovery keys before serialization across request, environment, artifacts, terminal/JSONL, inspectors, diagnostics and export | The [sensitive-surface map](p8-sensitive-surface-map-2026-09-22.md) binds exact generated markers to passing both-store packaged request/environment, capture, paginated inspector, JSONL and ConPTY checks; private staging and independent encrypted export/restore checks also pass. | No diagnostic-bundle export command exists to qualify. These local debug qualification receipts do not establish production-artifact or independent-environment acceptance. | Preserve exact artifact identities and repeat the required cases against the intended production candidate. |
| Thirty-day notices, filtered purge, physical cleanup and retained external-backup disclosure | Four retention rows pass, including repeat notices, snapshot obligations, copy-after-purge and owned-generation cleanup; packaged purge produces explicit gaps. | This evidence does not establish universal erasure or every packaged filtered-purge/external-copy presentation variant. | Preserve active-exclusion/physical-cleanup/external-copy distinctions; map remaining user-visible variants without claiming external ciphertext deletion. |
| Independent encryption, writer/recipient authority, wrong-key/tamper/rotation/replay and ciphertext-only publication through failure | Exact packaged snapshots pass pinned Go-age and independent Node signature/inventory verification, wrong-key/tamper controls, retained old-key recovery and local opposite-backend restore; component writer/replay/rotation cases also pass. | Intermediate/failing sync-folder publication observations, full packaged negative-control mapping and two-Windows-environment repetition remain incomplete. Local publication/decryption is not cloud transfer or independent-machine recovery. | Retain local proof; map missing publication/failure points and packaged controls. Keep the owner-skipped machine row not run; revisit only under renewed owner direction. |

[ADR-019](../adr/019-cloud-encryption-and-keys.md) requires locally encrypted
cloud-bound objects, developer-controlled recovery and independent format
verification. It also preserves the offline freshness and previously copied
ciphertext limits. Neither provider encryption nor an absent plaintext marker
substitutes for observing the encrypted object and its publication boundary.

## P8-04 — Windows distribution

| Acceptance | Observed proof | Remaining gap | Next action |
|---|---|---|---|
| Explicit package inventory, notices/provenance, runtime/model terms and checksums | Exact unsigned ZIP includes selected payloads, skills, notices, compatibility and build receipts; model specification/provisioner pins ten assets; inventory and extracted-byte identities are verified. | No signed artifact exists; a signed/repacked candidate would have a new identity. | Retain the unsigned designation and existing hashes; document signing prerequisites and requalify any changed final artifact. |
| Clean install/start, data/vault/key diagnosis, upgrade/uninstall and preservation | Exact-ZIP fresh-profile Unicode/spaces smoke passes; workspace/history/key/vault sentinels survive uninstall. Native synthetic installer cases cover locks, interruption, ownership and rollback. | Same-host smoke is not a clean Windows install; synthetic payload upgrade tests are not all final-executable upgrade workflows. | Keep clean-OS not run; map exact-artifact upgrade and first-run/dependency/key workflows to required cases before completion. |
| Explicit compatibility, recoverable migration and safe old-binary rollback | Manifest declares store/config/index compatibility; installer refuses incompatible declarations and validates compatible previous-release rollback. | Cross-format migration is not implemented or qualified, and actual old-binary/new-state rollback cannot be inferred solely from synthetic metadata refusal. | Declare the supported compatible-upgrade boundary; test required old/new binary state refusal. Any future cross-format migration requires its own snapshot/staged-restore qualification. |
| Exact installed package performs coding, pause/resume, model setup and encrypted recovery without developer-cache assumptions | Packaged synthetic coding/history, long-check completion/pause, local crypto restore and skill relocation pass; exact ZIP smoke and cached model verification pass. | Full clean-profile integrated workflow without developer caches, fresh model acquisition and independent-machine encrypted recovery remain unproven. | Reuse exact-package results within scope; enumerate only absent integrated variants. Preserve the owner-skipped machine condition rather than replacing it with local roots. |

## Completion boundary

P8-01–04 require their acceptance mappings and evidence beyond the executable
matrix. Closing the two previously missing command receipts alone cannot close these work
items. The independent-machine row remains an explicit unmet gate under the
owner's current scope, not a passing exemption. [ADR-018](../adr/018-release-acceptance.md)
separately requires complete integrated U01–U09/FR/I evidence and factual owner
acceptance at P8-05. This record grants no release approval, signing claim or
publication authorization. The root evidence ledger owns subsequent receipt
updates and plan-status decisions.
