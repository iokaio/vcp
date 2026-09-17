# Codex selection record

Origin: OpenAI Codex, exact revision
`3d3ae4965ab370217e871b3a7f0d15589557ee4b`, acquired 2026-09-17.
Owner: P0-07 (selection/build) and P0-08 (integration/effect replacement).
State: imported, unqualified as a VCP runtime.

[Selection](codex-selection.json), [result inventory](codex-files.json),
[manifest](../upstreams.toml), [build/reconstruction procedure](../../../docs/development/codex-source.md)
and [original native baseline](../../../docs/evaluations/p0-07-native-candidates.md)
form the source record. The ordered patch array is empty. The only transformation
materializes bubblewrap's license symlink from its selected `COPYING` bytes.

The full `codex-rs` workspace and Cargo lockfile retain internal dependency
closure. Registry/Git dependency versions and checksums remain in that lockfile;
root Bazel files, third-party support, scripts and patch inputs preserve related
upstream maintenance structure. Native voice components have separate source
archives/hashes in `third_party/voice/sources.json`; no compiled voice library or
Microsoft redistributable is imported by this selection. This record does not
qualify optional/platform-specific targets or promise a fully offline build.

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

VCP adds no copyright or SPDX replacement inside this unchanged source. Its root
license does not supersede bundled terms. Publication of a packaged binary will
require the actual enabled dependency closure, complete notices and source offer
or corresponding-source obligations where applicable under P8-04/P8-06.

The selection intentionally retains upstream credential/provider/telemetry and
scheduler code for controlled qualification. It is not exposed as a VCP session.
The [effect inventory](codex-boundaries.json), [source guide](../../../docs/development/codex-boundaries.md) and P0-03/P0-08
own replacement/injection before product use; no hidden effect is accepted merely
because this source builds.
