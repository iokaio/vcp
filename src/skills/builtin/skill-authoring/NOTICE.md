# Source and modifications

Adapted from Anthropic, PBC, [skill-creator](https://github.com/anthropics/skills/tree/8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4/skills/skill-creator), Apache-2.0. See LICENSE.txt.

VCP modifications (2026-09-29): native VCP descriptors and tool boundaries; concise authoring workflow; no fixed interview, provider CLI, background benchmark or sending authority. Existing VCP package-format guidance is original Apache-2.0 material. scripts/validate.cjs adapts quick_validate.py to native descriptors: hashes, resource roles, duplicate matching values, portable paths and links, with warnings for directory/id mismatch, non-semantic versions, missing "Use when" guidance and undeclared files. SH-07 aligns it with the runtime: the 16 KiB descriptor bound, UTF-8 bodies, the reference role, materializable helpers, host-emittable cues, known VCP tool names and links to declared files.

Selected upstream source bytes:

- `skills/skill-creator/SKILL.md`: SHA-256 `dcd4803e61e913e6fc27294184cd3a71f09f5e924ff20c8a9a20173e7b3c2bcf`
- `skills/skill-creator/scripts/quick_validate.py`: SHA-256 `67cf5703402013936c8fb75ad6a1afecd8841d45cc5e606b634eb05825fde365`
