# Local embedding qualification inputs

Owner: P0-07 runtime/asset provenance and P0-02 full local-memory qualification.
The original `src/crates/vcp-embedding/` helper is a member of the retained Codex
Cargo workspace. It has no Hub downloader or provider API. Its BERT implementation
comes from installed Candle crates, not a second coding engine.

| Input | Immutable identity and retained evidence |
|---|---|
| Candle core/nn/transformers | 0.11.0, published source `31f35b147389700ed2a178ee66a91c3cc25cc80d`; registry checksums in [the dependency record](embedding-dependencies.json) |
| Tokenizers | 0.22.2, source `6573f2c56172bac56f211e77934be3215adef2c2`, `onig` enabled without default/download features |
| Model | all-MiniLM-L6-v2 at `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`; [ten asset identities](minilm-assets.json), 91,578,299 bytes |
| Inference contract | F32 CPU, pinned tokenizer, 256 wordpieces, masked mean and L2 normalization, 384 dimensions |
| Independent reference | PyTorch 2.8.0+cpu and Transformers 4.57.1; [synthetic fixtures and reproduction](../../tests/fixtures/local-embeddings/README.md) |

Candle's newer repository head was first inspected at
`ddf1b879dc3a1760cbcb3f3c4a7c6467850cec4a`. That is the cited example reference,
not the published crate's source identity. The actual package `.cargo_vcs_info.json`
and BERT loader were inspected before qualification. The native graph requires
141 packages and rejects the identified HTTP/Hub and GPU backends; that check
does not prove OS network denial.

The safetensors weights are 90,868,376 bytes, SHA-256
`53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db`.
Software and asset license declarations are recorded separately in
[third-party notices](../../../THIRD_PARTY_NOTICES.md). No model asset is
committed. Explicit acquisition creates a fresh destination outside the checkout
and retains failed partial downloads; normal inference never acquires assets.

See [the development guide](../../../docs/development/local-embeddings.md)
for commands, effect boundaries and remaining integration work.
