# Upstream attribution and modifications

This VCP 2.0.0 skill package contains material adapted from Anthropic, PBC's [anthropics/skills](https://github.com/anthropics/skills) repository. Upstream attribution and license notices are retained.

- Upstream revision: `8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4`.
- Source directory: [skills/claude-api](https://github.com/anthropics/skills/tree/8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4/skills/claude-api).
- Per-skill license examined: Apache License 2.0, copied byte-for-byte to [LICENSE.txt](LICENSE.txt).
- Source reviewed and VCP adaptation recorded: 2026-09-29.
- Adaptation author: VCP contributors. No Anthropic endorsement is implied.

## Original source inventory

SHA-256 values below identify the original Git commit bytes, before adaptation or checkout line-ending conversion. The VCP descriptor separately hashes every shipped body/resource.

| Original path | Original SHA-256 |
| --- | --- |
| `skills/claude-api/SKILL.md` | `9aac10d9ffb8778c5b7c0b9ec612b3915e83065feb92763c8231469e9fbc29c9` |
| `skills/claude-api/shared/tool-use-concepts.md` | `bc5364fed0fe2a196a9f81cddf943d210adde53e3ad5be9b84057093fc8b28f5` |
| `skills/claude-api/shared/agent-design.md` | `9960728faf6976a2ddfceed02ed72af122d3b2558af573a9a0ea207b3adec2bc` |
| `skills/claude-api/shared/prompt-caching.md` | `92c0904c031831594187840231628d5986c4fd9e0e3415e41ac06bf9220e5c60` |
| `skills/claude-api/LICENSE.txt` | `bc6b3af2f331cbc7fb0da1344efb2cbe5877a31498b4d70dbc7000f3405a1362` |

## VCP modifications

- The upstream YAML front matter is not retained; `skill.json` is the only package metadata, and its description drives discovery.
- SKILL.md and references/integration-patterns.md adapt SDK helper reuse, tool definitions and loops, complete response preservation, streaming argument validation, typed errors, context and cache-stability workflows.
- references/anthropic.md retains optional Anthropic wire-format guidance without changing the provider of unrelated projects.
- assets/tool-schema.json ports the weather-tool definition from shared/tool-use-concepts.md; VCP changes add a use condition, bounded location string, rejection of additional properties and explicit application-owned unit defaults.
- VCP changes remove mandatory Anthropic/model selection, changing model/price tables, fixed token defaults, automatic feature substitution, hidden installs and compulsory paid evaluation work. Provider adapters and existing authorization/budget controls remain authoritative.
- No provider SDK, credential, network runner or auto-executing tool is included.
- SU-11 (2026-09-29) adds VCP-authored material with no upstream source: assets/tool-schema-neutral.json (the ported weather tool as a provider-neutral definition with the same parameters schema), references/tool-schemas.md (per-provider wrapping), references/openai.md, references/gemini.md and references/hosting.md (provider and hosted-platform notes checked against official documentation and SDK types on that date), plus reasoning-content, batch, token-counting and embeddings/RAG sections in references/integration-patterns.md and a short VCP addition in references/anthropic.md.
- Package rule: no file in this package contains model identifiers, prices or default token counts. Applications supply those from their own configuration and current provider documentation; `src/tests/skills/test_llm_integration_assets.py` checks model-identifier and price patterns.

Only the listed permissively licensed skill material is used. No material from the restricted document skills or unlicensed doc-coauthoring skill is included. Runtime permissions, explicit activation and VCP package integrity remain unchanged.
