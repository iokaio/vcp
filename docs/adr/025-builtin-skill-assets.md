# ADR-025 — Built-in skill asset identity

Status: accepted for P7-02; native asset contracts qualified. Live usefulness remains unqualified.

## Decision

Ship the original built-in skill packages beside the executable at
`skills/builtin`. Embed only the catalog inventory in the binary. Before exposing
the reserved built-in source, compare installed catalog bytes and the referenced
coverage/descriptor hashes with that inventory through the existing authorized
native read boundary. Bodies and resources remain external and load only on
activation, using the P7-01 identity and authority checks.

The default source uses the same registry, precedence and disable semantics as
configured skills. Its reserved source/root IDs cannot be supplied as a user
source. The executable directory is the only default location; no working-tree,
home or environment fallback discovers another package collection. Missing assets
produce a visible diagnostic while preserving bare-binary operation. Present
corrupt assets fail closed. A package upgrade cannot silently reinterpret an
old active body: its captured dependencies must still validate.

## Packaging and evidence

Build-time staging reads the exact catalog allowlist, verifies all content hashes,
and rejects links, extra files and path aliases. Archive validation checks every
entry, bounded expanded length and content digest before accepting the archive.
The package record binds executable and catalog hashes. This is asset packaging
qualification, not the full P8 installer, migration, signing or release gate.

Coverage separates available guidance from observed toolchain execution and live
usefulness. Frozen project fixtures can prove discovery, activation, attribution
and unchanged fixture bytes; they cannot establish model command selection or
quality. Record additional integrity metadata reads separately from normal
descriptor discovery so lazy-loading measurements do not hide verification cost.

## Alternatives

Embedding bodies would avoid a sidecar directory but obscure the on-demand asset
boundary and distribution inventory. Resolving assets from a repository or home
directory would make behavior depend on ambient state. Executable-relative,
hash-bound assets preserve relocation and explicit identity without importing a
foreign skill loader or introducing another tool executor.
