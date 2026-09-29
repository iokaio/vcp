# Upstream attribution and modifications

This VCP 2.0.0 skill package contains material adapted from Anthropic, PBC's [anthropics/skills](https://github.com/anthropics/skills) repository. Upstream attribution and license notices are retained.

- Upstream revision: `8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4`.
- Source directory: [skills/mcp-builder](https://github.com/anthropics/skills/tree/8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4/skills/mcp-builder).
- Per-skill license examined: Apache License 2.0, copied byte-for-byte to [LICENSE.txt](LICENSE.txt).
- Source reviewed and VCP adaptation recorded: 2026-09-29.
- Adaptation author: VCP contributors. No Anthropic endorsement is implied.

## Original source inventory

SHA-256 values below identify the original Git commit bytes, before adaptation or checkout line-ending conversion. The VCP descriptor separately hashes every shipped body/resource.

| Original path | Original SHA-256 |
| --- | --- |
| `skills/mcp-builder/SKILL.md` | `0f4592dcb53cf2b5d6b7febee6b4152018b565551a1c29e3c612f57b218ab295` |
| `skills/mcp-builder/reference/mcp_best_practices.md` | `80fb4369a349447cf18ecdd7494fe7938b6065377e9f08c077cec411093a3007` |
| `skills/mcp-builder/reference/node_mcp_server.md` | `c3ba35a4f599dd53be9c6555ae72c19a7bf412cd5426576c2c08d42755482c66` |
| `skills/mcp-builder/reference/python_mcp_server.md` | `2da52f77e675191014ca2e146a4b95aa04d0ca7dd7e2b100322df15ade685e80` |
| `skills/mcp-builder/LICENSE.txt` | `bc6b3af2f331cbc7fb0da1344efb2cbe5877a31498b4d70dbc7000f3405a1362` |

## VCP modifications

- SKILL.md and references/server-patterns.md port task-oriented tool design, naming, typed registration patterns, pagination, result formatting, transport, annotations and actionable errors.
- assets/paginated_result.py adapts the Pagination Implementation response construction from reference/python_mcp_server.md into a dependency-free function; VCP adds strict integer/range and page consistency checks, rejects nonprogressing empty pages, and returns a dictionary for SDK serialization.
- VCP changes preserve the project SDK/transport, limit exposed operations to the task, retain authorization and cancellation behavior, and replace mandatory broad API coverage, universal dual output formats and LLM evaluation campaigns with focused project checks.
- No SDK, inspector, evaluator runner, credential, installer or registration command is bundled.

Only the listed permissively licensed skill material is used. No material from the restricted document skills or unlicensed doc-coauthoring skill is included. Runtime permissions, explicit activation and VCP package integrity remain unchanged.
