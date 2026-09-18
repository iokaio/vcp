# Codex selection record

Origin: OpenAI Codex, exact revision
`3d3ae4965ab370217e871b3a7f0d15589557ee4b`, acquired 2026-09-17.
Owner: P0-07 (selection/build) and P0-08 (integration/effect replacement).
State: imported with bounded P0 integration; not a production VCP runtime.

[Selection](codex-selection.json), [result inventory](codex-files.json),
[manifest](../upstreams.toml), [build/reconstruction procedure](../../../docs/development/codex-source.md)
and [original native baseline](../../../docs/evaluations/p0-07-native-candidates.md)
form the source record. The ordered [patch series](../patches/codex/README.md)
records the `codex-chatgpt` recursion-limit change for the Rust 1.98.0 compiler
experiment, the Munarium/embedding workspace and lockfile integrations, and
the P0-03 continuation-admission hook with retained-controller regression tests.
The scoped-lifecycle patch extends that hook with controller-owned thread IDs,
owned retained interruption and the original lifecycle host workspace member.
It adds one local lock entry while retaining all existing external dependency
identities/checksums. The subsequent lifecycle-recovery patch adds private
startup/model/tool admission, completion receipts, native job observation and
existing host dependencies. The host now has a private durable checkpoint and
synthetic owner-process CLI; it is not a qualified production VCP runtime.
Patch 0009 registers the original P0 storage comparison with existing age,
Ed25519, SQLite and retained search dependencies; external pins remain unchanged.
Patch 0010 preserves host gates in isolated helpers, carries usage into atomic
receipts, checks tool ceilings and reuses existing parser/governance dependencies.
The [handoff map](../../../docs/development/p0-handoff.md) records retained/replaced
responsibilities and the representative fix-import experiment.
The separate license transformation materializes bubblewrap's symlink from its
selected `COPYING` bytes. Original bytes/hashes and current results are retained.

The full `codex-rs` workspace and Cargo lockfile retain internal dependency
closure. Registry/Git dependency versions and checksums remain in that lockfile;
root Bazel files, third-party support, scripts and patch inputs preserve related
upstream maintenance structure. Native voice components have separate source
archives/hashes in `third_party/voice/sources.json`; no compiled voice library or
Microsoft redistributable is imported by this selection. This record does not
qualify optional/platform-specific targets or promise a fully offline build.

Patch 0011 also registers the four original [P1 foundation packages](../../../docs/development/p1-foundation.md)
without changing external dependency pins or imported implementation.
Patch 0012 registers the original budget and audit packages under the same rule;
see [accounting and history](../../../docs/development/p1-accounting-history.md).
Patch 0013 connects the [canonical host](../../../docs/development/p1-retained-host.md)
to retained per-attempt HTTP admission and observed-body capture. It keeps the
same controller and external dependency identities; the P0 journal remains a
separate regression fixture rather than a second canonical store.

Patch 0014 registers original repository/context packages using existing pinned
dependencies and retained native process containment. It changes only workspace
and local lock entries; see the [P2 context guide](../../../docs/development/p2-context.md).
Patch 0015 registers the original provider codec and propagates the host's total
response deadline through retained headers/body. It retains the existing HTTP
client and scheduler; see [the provider guide](../../../docs/development/p2-provider.md).

Applicable source notices are retained unchanged:

- Root `LICENSE` (Apache-2.0) and `NOTICE` (OpenAI and Ratatui attribution).
- `codex-rs/vendor/bubblewrap/COPYING` (Library GPL v2); bubblewrap source headers
  identify LGPL-2.0-or-later and preserve individual authors' copyrights.
- `third_party/wezterm/LICENSE` (MIT, Wez Furlong).
- `third_party/voice/NOTICE.md` and every file in its `licenses/` directory:
  LGPL-2.1, Opus/BSD, PCRE2/BSD, libffi/MIT, proxy-libintl, sljit/BSD and zlib terms.
  These accompany retained build/provenance material; they are not a claim that
  VCP distributes a native voice runtime.
- Bundled skill license files in `codex-rs/skills/src/assets/samples/` (Apache-2.0),
  source-specific copyright headers, and upstream test/asset provenance.

VCP adds modification comments to changed files, without replacing original
copyright or SPDX terms. Its root
license does not supersede bundled terms. Publication of a packaged binary will
require the actual enabled dependency closure, complete notices and source offer
or corresponding-source obligations where applicable under P8-04/P8-06.

The selection intentionally retains upstream credential/provider/telemetry and
scheduler code for controlled qualification. It is not exposed as a VCP session.
The [effect inventory](codex-boundaries.json), [source guide](../../../docs/development/codex-boundaries.md) and P0-03/P0-08
own replacement/injection before product use; no hidden effect is accepted merely
because this source builds.

Patch 0016 registers the original pure policy package and canonical engine/store
dependencies. It changes no retained implementation or external dependency pin;
[the authority guide](../../../docs/development/p2-policy.md) distinguishes this
core increment from subsequent native broker qualification.
