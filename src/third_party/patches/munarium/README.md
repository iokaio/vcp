# Munarium source patch series

Base: `iokaio/munarium@8da666067000ca1ee9c131bc67e70b978862faa3`.
The [selection](../../components/munarium-selection.json) owns patch order and
digests. Committed source already includes these changes; normal builds do not
fetch source or apply patches.

`0001-shared-workspace.patch` changes only the three selected library manifests.
Each names the existing Codex Cargo workspace explicitly and materializes the
original Munarium package version, edition, license and inherited dependency
requirements/features. It preserves the libraries' 1.2.1 version and source
behavior. Each changed manifest carries a VCP modification notice. Original
Apache-2.0 headers, LICENSE and NOTICE remain intact.

The retained `server/Cargo.toml`, `Cargo.lock` and toolchain file describe the
upstream baseline. They are provenance inputs, not a second VCP build entry
point. The shared Codex lockfile selects actual dependency versions; its ordered
patch is recorded separately in [the Codex series](../codex/README.md).

Reconstruct with the [committed library procedure](../../../../docs/development/munarium-source.md).
If workspace dependency requirements change, re-materialize them from the exact
selected upstream manifest, review feature differences and regenerate both the
patch and resulting file inventory. Do not silently inherit Codex package
metadata or dependencies under Munarium's original names.
