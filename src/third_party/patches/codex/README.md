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

The unmodified 1.98.0 failure is retained as qualification evidence. Any future
upstream update should check whether the attribute remains necessary and whether
the layout has changed. Removing this patch is an explicit source-maintenance
change with updated reconstruction hashes and native checks.

Use [the reconstruction procedure](../../../../docs/development/codex-source.md#explicit-reconstruction)
in a fresh disposable directory. Original and resulting per-file hashes remain
separate, and unchanged files retain their prior transformation labels.
