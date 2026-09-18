# Codex compatibility patch series

Base: `openai/codex@3d3ae4965ab370217e871b3a7f0d15589557ee4b`.
The [selection manifest](../../components/codex-selection.json) is the authority
for patch order and exact SHA-256 values. Files under `src/third_party/codex/`
already include these changes; normal builds never apply patches.

1. `0001-chatgpt-recursion-limit.patch` raises the `codex-chatgpt` crate recursion
   limit to 256 and adds a VCP modification notice. Rust 1.98.0 exceeded the
   default query-depth limit while computing the future layout for
   `connectors::list_connectors()`. The change follows that compiler diagnostic;
   it changes compiler capacity, not runtime provider, policy or credential
   behavior. The retained upstream Apache-2.0 terms and copyright are preserved.
2. `0002-munarium-workspace.patch` registers the three separately attributed
   Munarium libraries as members of this workspace and extends its generated
   lockfile. All 1,491 previously locked package identities/checksums remain;
   34 entries are added. The workspace manifest and lockfile carry modification
   notices. This adds a shared build graph, not a competing engine or an
   implemented VCP memory adapter.
3. `0003-local-embedding-workspace.patch` registers the original VCP CPU embedding
   adapter and its Candle/tokenizer dependencies in the same graph. It adds 40
   net lock entries and changes the existing `regex-automata` 0.4.13 pin to 0.4.14,
   the minimum required by Candle's `fancy-regex` 0.18 dependency. All other
   preexisting package identities/checksums remain. The compatible tokenizer
   macro version keeps the existing `pastey` pin. Modification notices identify
   these changes; this does not implement VCP memory/index integration.

4. `0004-local-corpus-workspace.patch` registers the original P0-02 corpus
   qualification executable in the same graph. It adds only its local package
   lock entry; all 1,565 prior dependency identities/checksums are retained.
   It calls the existing CPU embedding and Munarium datastore APIs without
   changing their implementation or the Codex runtime.

5. `0005-local-governance-workspace.patch` connects the qualification package to
   the already selected Munarium core, in-memory backend and Tokio. It changes
   only that local package's lock dependencies and the modification notice;
   all 1,566 package identities/checksums and other dependency records remain.
   No imported Rust implementation is changed.

6. `0006-continuation-admission.patch` adds a host continuation hook to the
   retained extension API and gates delegated child input, review delegates and
   mailbox wakeups. Five new native regression cases accompany four implementation
   files. The default preserves existing shutdown drain; no pause command,
   cancellation, persistence, dependency or lockfile change is included. See the
   [implementation guide](../../../../docs/development/continuation-admission.md).

7. `0007-scoped-lifecycle.patch` threads controller identity through admission,
   adds retained owned interruption and three integration cases, and registers
   the original `vcp-lifecycle` package with one local lock entry. All prior
   external package identities/checksums are retained. See the
   [scoped lifecycle guide](../../../../docs/development/scoped-lifecycle.md).

8. `0008-lifecycle-recovery.patch` adds private startup/model/tool admission,
   completion receipts and native Job Object membership observation. The original
   lifecycle host gains existing serialization, hashing and process dependencies;
   external dependency identities are unchanged. See the
   [recovery guide](../../../../docs/development/lifecycle-recovery.md).

9. `0009-storage-qualification.patch` registers the original P0 storage experiment
   in the shared workspace and lockfile. SQLite, age, Ed25519 and search package
   pins remain unchanged. See [the storage guide](../../../../docs/development/portable-storage-spike.md).

10. `0010-integrated-host.patch` preserves host controls when helpers isolate
    their extensions, carries provider usage into the host's atomic receipt,
    applies the independent tool-name ceiling and exposes retained tool-ceiling
    injection to qualification tests. Existing local parser, feature, login and
    Munarium dependencies are connected to the host; no external pin changes.
    See [the integration guide](../../../../docs/development/p0-integration.md).

11. `0011-p1-foundation-workspace.patch` registers four original VCP foundation
    packages (`vcp-domain`, `vcp-protocol`, `vcp-store`, `vcp-engine`) in the
    retained workspace and lockfile. External package identities and checksums
    are unchanged. No imported implementation is changed in this increment.
    See [the foundation guide](../../../../docs/development/p1-foundation.md).
12. `0012-p1-accounting-history-workspace.patch` registers the original budget and
    audit packages. It adds only local lock entries and preserves external pins.
    See [accounting and history](../../../../docs/development/p1-accounting-history.md).
13. `0013-p1-canonical-host-integration.patch` connects actual retained HTTP
    attempts to canonical host admission, captures observed response bodies
    before SSE parsing, and passes response identity with usage. Hosted requests
    use HTTP with one transport attempt; retries re-enter admission. The local
    lifecycle lock entry gains the existing foundation packages and Wiremock;
    no external dependency pin changes. See [the retained host guide](../../../../docs/development/p1-retained-host.md).
14. `0014-p2-context-workspace.patch` registers original repository/context
    packages and their local lock entries. Existing external pins are unchanged;
    native Git observation reuses the retained Job Object. See [the context guide](../../../../docs/development/p2-context.md).

The unmodified 1.98.0 failure is retained as qualification evidence. Any future
upstream update should check whether the attribute remains necessary and whether
the layout has changed. Removing this patch is an explicit source-maintenance
change with updated reconstruction hashes and native checks.

Use [the reconstruction procedure](../../../../docs/development/codex-source.md#explicit-reconstruction)
in a fresh disposable directory. Original and resulting per-file hashes remain
separate, and unchanged files retain their prior transformation labels.
