# P8-02 local recovery follow-up — 2026-09-22

This follow-up closes additional finite local schedules from the
[boundary map](p8-local-recovery-boundary-map-2026-09-22.md), after the
[first recovery increment](p8-local-recovery-increment-2026-09-22.md).
Existing passing evidence is retained; the full matrix was not repeated.
These are native debug component/qualification tests. They do not qualify the
intended production package, complete P8-03, or establish owner acceptance.
Machine handoff remains owner-skipped.

## Selected bounded results

Each command below had a 600-second deadline covering compilation and execution,
with process-tree termination on expiry and retained command, logs, outcome and
before/after source inventories. Every selected result reports exit zero,
`timed_out: false` and stable source during the command. Commands use locked,
offline Cargo with two jobs and one test thread. Test fixture roots and
supervisor observations are retained; their local locations appear in the logs.
All five referenced files in every selected result were rehashed and matched.

| Boundary | Passing finite schedule and oracle | Wall seconds | Receipt |
|---|---|---:|---|
| Artifact seal/publication | Ten actual kills: Files/SQLite × before publication, after publication, after reply, after canonical reference, and injected seal `StorageFull`. Previously acknowledged binary chunks remain readable; physical completion is distinguished from canonical completion, and retry does not fabricate an earlier success. | 19.904 | `artifacts/p80203-recovery/artifact-59258a72-c637-48f2-a197-e73fac22aec5/result.json` |
| Ciphertext staging capacity | Three injected `StorageFull` positions: encryption header, body and finalization. Failure does not publish plaintext or a completed object; capacity-return retry publishes decryptable exact bytes. | 7.957 | `artifacts/p80203-recovery/staging-b990dc94-1af0-4205-9cad-aad13ee5828f/result.json` |
| Restore staging capacity | Four injected private short writes: both cross-backend directions × replay base/artifact chunk. The 17-byte private partial and encrypted source remain intact; reopen/retry imports exact history and artifact bytes with one authority reset. No active-root activation is performed by this test. | 8.086 | `artifacts/p80203-recovery/restore_stage-f1e5b6b2-caad-4e70-a5b4-ce7379885366/result.json` |
| Offline divergence | Independently advanced local heads reject each other's divergent object, including a higher-sequence descendant. Rejection preserves the local checkpoint. A fresh anchor admits either valid initial branch without claiming knowledge of the global newest head. | 6.483 | `artifacts/p80203-recovery/offline-01f1744b-6ab3-42f1-b4dd-0db61e82572f/result.json` |
| Prune | Eight actual kills: Files/SQLite × tombstone, before activation, activation and cleanup. Fresh queries keep selected content logically unavailable. Saved acknowledged transactions and foreign-workspace records remain unchanged before/after cleanup; fresh reopen retains the same nonzero placeholder count. Cleanup/retry preserves unrelated records, prior receipts and event prefixes; another cleanup attempt can append its own receipt without repeating redaction or rewrite. | 11.888 | `artifacts/p80203-recovery/prune-a90d9479-233f-498e-9c72-e00ca246eb62/result.json` |
| Combined artifact owner loss/vault copy | Six actual kills: both stores × artifact sealed, partial ciphertext copy and completed copy, resumed sequentially on the same fixture. Reopen preserves exact acknowledged artifact/history; partial ciphertext is refused, and completion retains one published object. | 11.397 | `artifacts/p80203-recovery/combined-0c99f5aa-0bae-4ab6-889c-c1ec2b9f6b78/result.json` |
| Root/child model dispatch | Twelve actual kills: root/registered child × Files/SQLite × durable intent before transport, independently observed request arrival, and validated response before settlement. Two reopens retain quotes, intent, unknown liability, artifact bytes and receipt/event prefixes; tasks reopen paused with no automatic replay or fabricated charge. Repeated after the startup-classifier correction; these are the same twelve cases. | 18.328 | `artifacts/p80203-recovery/model_kill-78e79e94-e110-46b8-8805-42dbefc0063f/result.json` |
| MCP response before receipt | Four actual owner kills: Files/SQLite × stdio/HTTPS after a valid reply, before durable receipt. Independent request/effect observations and fresh reopen prove unknown outcome and no automatic replay. The retained dispatch task, completion-before-kill failure marker and owner liveness check exclude a request-timeout completion masquerading as this crash boundary. Repeated after the P8-03 startup-classifier correction; these are the same four cases. | 14.338 | `artifacts/p80203-recovery/mcp_kill-ec9ee337-7b44-4c2a-bf90-a464f23e962d/result.json` |
| Native content-driven authority | Both stores: hostile repository bytes reach the model as tool output, followed by permitted ordinary edit/delete controls and denied traversal, Git metadata, host control, ignore-policy and setup operations. Outside/protected sentinels remain unchanged, setup does not dispatch, and fresh reopen retains the exact effects and receipts. | 68.718 | `artifacts/p80203-recovery/content_native-d8f15e32-bd92-42bb-b411-7ca85dc40b4c/result.json` |
| MCP content-driven authority | Both stores: hostile resource/prompt bytes and cached provenance reach the provider as untrusted data, then native edit/delete succeeds within prior owner-granted Write authority while traversal and Git metadata requests fail before native preparation. Outside/protected sentinels remain unchanged; independent TLS observations remain six protocol requests and two content effects, with no MCP replay. | 125.827 | `artifacts/p80203-recovery/content_mcp-2da6d1df-36f6-4fcd-b72b-5c2950134e64/result.json` |

The model supervisor owns the HTTP peer independently of the killed canonical
owner. Before-transport cases observe zero requests; arrival and validated-reply
cases observe exactly one. The latter retains full response capture before
settlement. These are deterministic local provider fixtures, with no paid model
calls. MCP stdio/HTTPS fixtures similarly distinguish remote observation from
the owner's missing durable completion receipt. Neither schedule authorizes
automatic retry of an unknown external effect.

The prune supervisor's historical `prune_acknowledged: false` means the child
published no external/user acknowledgement. It does not mean every internal
operation was unacknowledged: `retention::apply` had returned before the
`before_activation`, `activation` and `cleanup` barriers. Cleanup completion
remained unacknowledged. This interpretation preserves the original receipt
bytes and distinguishes logical tombstone application from completed cleanup.

### Receipt and source identities

The base commit recorded by all selected commands is
`4551fa0cc735948819efbf246101f8e2ff0450bd`, with dirty source explicitly recorded.
The command-specific inventories, rather than the base commit alone, identify
what ran. Different inventories reflect fixture development; no older receipt
is relabeled as proof for an entire later source tree.

| Receipt | `result.json` SHA-256 | Source content SHA-256 |
|---|---|---|
| artifact | `b2fd406c3cebdcf0450d0b8cefec96ec6ff8741adaebe91151be2027d4306231` | `5dceac430c3757b6fdec7ed736bfd97e0d5aa8fe91130e41f4e785a4b11b338f` |
| staging | `7deb975b8087d9bccd086e75f8634434313cc77738089419b9840a098f6213eb` | `5dceac430c3757b6fdec7ed736bfd97e0d5aa8fe91130e41f4e785a4b11b338f` |
| restore staging | `f9498819fb93cd8e98f92897ca8f0277cee7be5e4054536c1241fa6a4aec08dd` | `5dceac430c3757b6fdec7ed736bfd97e0d5aa8fe91130e41f4e785a4b11b338f` |
| offline | `6e3678d177d46c9878f079006ddee186a6abc080ba4f2082223bc360ed50a6e5` | `5dceac430c3757b6fdec7ed736bfd97e0d5aa8fe91130e41f4e785a4b11b338f` |
| prune | `e7569b058ddf1481d848611644d14d8f8b24ee74994c6d59c2aaf195c79959e2` | `3de7996927f178ee400642fdd47d35875d9f82636a138ed29044d10c76e19778` |
| combined | `d7fa09ff66d0a218d02cfd57fe1c7cfaea2fcf2d8d6f9fd89f58a2e9546b616d` | `ddbe282d0ce88747b1aca61b30f0dcf528cdbd894a450a5f3916d248d1108792` |
| model dispatch | `a3b51d336a6ca49563ab507ff8d5b50d595fdb39e6d65d067164e033d0d43fa7` | `71e9b567a1e078719965d8354508e1428ade99c783a11e5e587e40ed004ecbdb` |
| MCP owner kill | `2d23089dc708e75aadbcf68cef0dfac2f8633ea65a6193292851186a0e4fca86` | `71e9b567a1e078719965d8354508e1428ade99c783a11e5e587e40ed004ecbdb` |
| native content authority | `14e3865e08d879320bd802d31d2555298e130057b395c837f1d9d900dc586c83` | `28e815fa435ddb87d0655e45cd22110e85fcbcda74b1c693d96afd3855257b0b` |
| MCP content authority | `97ee87dbdb9034e9894001017e8caa02281f8d7cf6ba5fdde2f1427f6a71d2db` | `3de7996927f178ee400642fdd47d35875d9f82636a138ed29044d10c76e19778` |

The exact selectors and log hashes are in each listed receipt. Relevant fixtures
are [artifact recovery](../../src/crates/vcp-store/tests/artifact_recovery.rs),
[staging](../../src/crates/vcp-store/tests/staging_recovery.rs),
[restore capacity](../../src/crates/vcp-store/tests/restore_capacity.rs),
[offline heads](../../src/crates/vcp-store/tests/key_publication.rs),
[prune](../../src/crates/vcp-memory/tests/governed.rs),
[snapshot jobs](../../src/crates/vcp-store/tests/snapshot_jobs.rs),
[model kills](../../src/crates/vcp-lifecycle/tests/support/model_dispatch_crash.rs)
and [MCP kills](../../src/crates/vcp-lifecycle/tests/support/mcp_process_kill.rs).
The native content-driven authority fixture is
[content_authority.rs](../../src/crates/vcp-lifecycle/tests/support/content_authority.rs).
Its MCP counterpart is
[mcp_content_coding.rs](../../src/crates/vcp-lifecycle/tests/support/mcp_content_coding.rs),
selector `mcp_content_coding::hostile_mcp_content_followups_cannot_escape_native_scope`.
The latter completes in 92.48 test seconds; the table includes compilation and
supervisor time. Ordinary patch permission comes from the owner before any
hostile content is observed. Resource/prompt roles remain external data and are
never promoted into system/developer instruction roles.

The affected MCP content baseline also passes: selector
`mcp_content_coding::coding_content_selectors_roles_and_cached_artifact_provenance_reach_real_provider_loop`,
receipt `artifacts/p80203-recovery/content_mcp_baseline-30f3c601-d5ff-4b1d-8814-c8082a2e3787/result.json`,
SHA-256 `8944b0e9a8b2720a7ca40f5e76cb09dd651a10994aae59917d7c2fc28a117654`.
It took 65.073 wall seconds (60.32 test seconds), with source content hash
`28e815fa435ddb87d0655e45cd22110e85fcbcda74b1c693d96afd3855257b0b`.
All referenced files match the receipt. This baseline is retained separately
from the passing hostile MCP content qualification above.

## Retained evidence mapped without rerunning

The existing P8 snapshot row already covers sixteen actual process kills:
Files/SQLite × `captured`, `prepared`, `encrypted`, `admitted`, `owned`,
`partial`, `copied`, `completed`. A job resumes across those positions, produces
one object and retains canonical watermark/task revision. It proves local vault
copy recovery, not interruption of an actual cloud provider transfer. Historical
per-barrier temporary roots were removed; retained outer logs must not be
described as retained independent supervisor receipts for every old kill.

Its `artifacts/p8-local-matrices/e21f5098-a79b-4ebb-8d6a-6d469c91ee7d/matrix/manifest.json`
has SHA-256 `9077462efcb84703d26b166884dde25d8e33f76af6d31d5f656275805e6fcaf0`.
The `p8-02-snapshot-job-recovery/stdout.log` row has SHA-256
`8c3857a2cdd845ddacc2fc9db2120df5d3dbe8fe5368090378ea7f8406f45d82`.
The command and both log hashes were checked against the manifest.

The `artifacts/p7-final-delegation/2bc1f5c9-b4f5-4fed-8b9a-7cc4565d4a03/manifest.json`
has SHA-256 `59ef747109ff6840a87676c16ed034a3c0255c1d1105216cf1b366c21be77b0d`.
All seven retained stage-log hashes match it. The finite trust-boundary map is:

| Existing stage | Applicable evidence | Limit |
|---|---|---|
| `repository-tools.log` | Scoped and absent-destination paths; Git metadata/device refusal; junction escape and metadata junction denial before external writes; untrusted Git filters cannot execute during observation; child cleanup checks exact root, parent and marker, including replacement/movement and partial-removal retry; process profiles reject model authority, environment credentials and implicit shells | Native path/ownership/tool boundary tests; not a claim about every model interpretation of hostile content |
| `connected.log` | Mixed child patches, sibling isolation, child-owner marker deletion refusal, current root budget, independent stop, write receipts and interrupted cleanup | Registered child and connected native fixtures; no bypass of missing filesystem sandbox permission |
| `canonical-contracts.log`, `verification-pause.log`, `terminal.log`, `native-cli.log`, `child-output-loss.log` | Canonical lifecycle, pause/check, CLI and output-consumer-loss regressions retained with their original selectors | Historical component/executable evidence; not final production install/upgrade qualification |

The first two log hashes are respectively
`b412048922b3b10c6d1a8e3ab552db4669686ede218482ece7d4e2f5b5d3d1b2`
and `96138fbc6235d1ea00541d33d6d10602766a2684118f255894d0717066921ee2`.
The manifest supplies the remaining five hashes. Existing MCP waiter-abort tests
remain useful cooperative interruption evidence; they are not counted as actual
owner kills. The new four-arm owner-kill receipt above addresses that distinction.

## Actual cloud hydration observation

A separate `artifacts/p802-cloud-hydration-revised/5e6c5312-0ae3-4b4b-b7dd-e706a3477478/result.json`,
SHA-256 `31d014b2782cbf71276c3457f3adab765ebb075e003db1f553f9e323167e31f8`,
records a 300-second deadline and completion in 2.232 seconds. An existing
771,824-byte encrypted OneDrive object was preserved independently, dehydrated,
then hydrated through a metadata-safe `WRITE_DAC` handle without changing its ACL.
The supervisor observed pending I/O (`0x800703e5`), incomplete overlapped
completion (error 996), and zero on-disk/validated bytes before killing the
helper. It remained at zero bytes immediately after that kill. Subsequent
rehydration returned the exact original length and SHA-256
`886e726f9df5f623d07fa560f5b45cb850a64a51cad197d9efd8675b5c780da4`.
The preserved copy, original pin state and original attributes were verified.

This is an observed unfinished hydration request followed by helper termination
and exact recovery. It does not claim interruption after a nonzero partial
transfer. No VCP process was active during this cloud helper run.

A subsequent same-ciphertext VCP restore now completes the sequential recovery
proof. The external supervisor receipt
`artifacts/p802-cloud-vcp-restore-supervision/30899d15-1e9b-4c9d-a828-200e5178ba96/result.json`
has SHA-256 `f840aa2067f5c38ac692c0b82890e13c323cbb4a6e25904abf7d76a39d1cb7bf`.
It enforced a 600-second process-tree deadline; the helper completed in 4.963
seconds. Its selected receipt
`artifacts/p802-cloud-vcp-restore/60110f7b-4ca4-4a75-b413-483ea071a622/result.json`
has SHA-256 `dd0b0371ea82c15add0c09a05375936cd6615f93d37149b50ca0401af09481e1`.
The supervisor retains exact helper source and hashed command logs. The helper
hash-links the preceding hydration receipt and verifies the same 771,824-byte
ciphertext hash before and after restore.

This run used the extracted qualification `vcp.exe`, SHA-256
`4343607c43def35e244c2f2d89f8b297bb7c1a06e387dcab33c5a00e5a541874`.
It independently enrolled the saved sequence-2 checkpoint and existing
disposable recovery key in fresh local roots, then restored the known sequence-3
archive into SQLite. The existing historical roots were not opened or changed.
Independent `age` decryption recovered the known manifest and validated all 92
payload object hashes. The oracle compared the complete 76-record historical replay state,
including 65 transaction receipts; 60 retained immutable live records; all 45
original event envelopes and 23 command receipts; every one of 97 spool parts
(11,719 chunk bytes); and four independently enumerated source files. These
comparisons passed after activation, after each of two CLI reopens, and after
exact operation retry. The task retained its stable identity and paused state,
the workspace was untrusted, and retry reconciled without repeating rebind.

This qualifies actual hydration-helper interruption, exact rehydration, then
packaged VCP restore/reopen of that same ciphertext. It does not establish VCP
activity during the interrupted request, a nonzero partial provider transfer,
or cloud publication interruption. The executable identity above remains the
scope of this receipt if a later package is rebuilt.

## Failed and superseded attempts retained

Failures are separate observations, not retrospectively converted to passes.
Each directory below retains `result.json`, its logs and source inventories under
`artifacts/p80203-recovery/` unless otherwise stated.

| Attempt directory | Disposition |
|---|---|
| `prune-f1567ad2-a2fe-4d38-8b63-8cae80bbcd29` | Failed visibility oracle: the fixture needed to distinguish pruned/purged history representation while asserting no body/evidence/applicability. |
| `prune-38c70632-9dec-4b6a-a432-5e929c7d817f` | Failed whole-state equality oracle: cleanup records a new attempt receipt. Corrected assertions preserve unrelated records, earlier receipts and event prefixes while allowing that specified receipt. |
| `prune-2da73e14-7bc4-4292-b563-b45b8d0e394d` | Test compilation failed on nonexistent `State.receipts`; fixture corrected to `State.transactions`. |
| `prune-b314059a-d39d-46fe-a052-f4aec0a8e76c` | Earlier eight-kill pass retained (result SHA-256 `073eac99d433affc75eeeb8fd46cd59f4f295e17e98967f63837eb4adf005281`). The selected `a90d9479` pass adds explicit saved acknowledged-transaction, foreign-workspace and nonzero reopen-placeholder assertions. |
| `combined-599aac3a-dc6b-4421-9aaa-262face7ae5c` | Child exited during fixture setup before `artifact_sealed`; no crash-boundary claim from this attempt. |
| `model_kill-1ca1505a-1b51-421e-82e2-049b26cb6ef1` | Compilation completed, then missing configured evidence-parent directory stopped the fixture. Runner created the parent; the selected rerun passed all twelve positions. |
| `model_kill-cd6b565b-272c-4012-860a-d047ca49fbe6` | Earlier twelve-kill pass retained, result SHA-256 `fac4e52be42d49fdf4bc4a198f99da1b88f947a73323feb94e2699932e969df8`, source `ddbe282d0ce88747b1aca61b30f0dcf528cdbd894a450a5f3916d248d1108792`, 19.322 wall seconds. The post-correction `78e79e94` pass is primary (13.51 test seconds); it adds no new distinct crash positions. |
| `mcp_kill-dc1b0448-9193-4e00-99e6-ce70bf8adf29` | Initially passed, but superseded because a request timeout could race the kill. The strengthened fixture passed as `acdfc730`, then was repeated after the startup-classifier correction as the selected `ec9ee337` receipt. |
| `mcp_kill-acdfc730-e10c-4171-a365-4965ba8b41ad` | Earlier strengthened four-arm pass retained, result SHA-256 `d6e4ff3f742d12eab7990e97204e3645b682722202bdd3c3d0335a5f89b8b093`, source `0e2514f24c7c292780286b1cb77bb382d32ece1f9fa3de13c92fc006808a9a6a`, 48.866 wall seconds. The post-correction `ec9ee337` pass is the primary regression receipt; it adds no new distinct crash positions. |
| `content_native-2650100a-c4c6-4b29-8b2c-a015028fce3c` | Native hostile-content fixture treated a scope-refresh response as a final policy denial. The retry oracle was corrected for the selected `d8f15e32` pass; this earlier failed result does not prove that acceptance condition. |
| `content_native-81a9c12a-dd74-4b47-9f53-dd76f5ff99d4` | Failed execution-ID oracle: successful native file operations also carry execution IDs. The retained state has two succeeded ordinary file effects and three cancelled control/setup effects without execution. A revised assertion distinguishes those exact effects; this failed attempt remains unselected. |
| `content_native-ea5a2ec3-e74d-4c97-97e8-22a2ef171260` | All semantic assertions passed, but fresh reopen failed because the fixture retained live test/host owner references. The fixture now drops those references before reopen; this failed attempt does not establish fresh-reopen qualification. |
| `content_mcp-bcf20ffc-faf9-4003-b674-0e78edea149c` | The positive ordinary-patch control lacked an owner Write grant. The fixture now grants Write before exposing untrusted content, retains intrinsic traversal/Git refusals, and selects precise native prepared-plan records because MCP also uses that schema. This failed attempt remains distinct from the selected `2da6d1df` pass. |

The `artifacts/p802-cloud-hydration/e54f9913-f1da-4231-844d-f25ec56dac1d/result.json`,
SHA-256 `35098895c678e16092090fd56638ff4f1af69617d50f75909006132f55dd6763`,
retains three attempts without an observed unfinished hydration interruption.
It is not selected as interruption evidence. The revised receipt above records
the distinct observed pending-I/O case. These fixture/setup/oracle failures do
not by themselves demonstrate a product defect.

The sequential restore helper also retains its initial rejected repository-local
data layout (`75f018af-f115-4822-b6d3-a5aad6fad2c9`), a successful activation
followed by a Python extended-path SQLite URI error
(`1ad18cc7-d47d-4a38-9fe2-483ef451bd4e`), and the first successful oracle run
(`eabdf606-6a48-4bcf-8863-f8a55e1849f2`) under
`artifacts/p802-cloud-vcp-restore/`. Only the externally supervised `60110f7b`
run is selected above. No cloud mutation was repeated for these restore runs.

## Remaining qualification boundaries

The prior increment's real SQLite `SQLITE_FULL` page ceiling remains valid;
Files, seal and staging `StorageFull` results are explicitly injected equivalents.
A physically full volume was not run. No claim is made that those mechanisms
qualify every filesystem or journal-tip publication failure. The cloud proof is
sequential interruption/recovery followed by VCP restore, with the limits above. The finite
selected crash schedules and trust tests do not claim every possible combined
sequence or content-driven authority case. Machine handoff stays skipped.

Production install, upgrade/rollback, interactive pause/resume and owner
acceptance remain separate P8-01/P8-04/P8-05 requirements. P8-03 credential/key
exclusion and packaged MCP history/compaction/reopen have their own
[follow-up evidence record](p8-history-security-followup-2026-09-22.md).
