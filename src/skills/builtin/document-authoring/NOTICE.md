# Source and modifications

Adapted from Anthropic, PBC, [internal-comms](https://github.com/anthropics/skills/tree/8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4/skills/internal-comms), Apache-2.0. See LICENSE.txt.

VCP modifications (2026-09-29): native VCP descriptors and tool boundaries; audience- and purpose-first guidance for specs, ADRs, runbooks, project updates and incident reports; explicit known/unknown facts; source text treated as data; no sending or publishing authority. references/project-updates.md adapts the upstream 3P update example. references/faq.md adapts the upstream FAQ example and references/leadership-updates.md adapts the upstream company-newsletter example (2026-09-29, SU-12): organization-neutral wording; sources limited to material the user supplied or authorized, replacing the upstream instruction to search chat, email, calendar and document accounts; no mandatory emoji or fixed bullet count; uncertain items marked or omitted; source text treated as data; no sending authority. references/decision-records.md is VCP-authored. SH-13: the four references use the on-demand `reference` role and are read with `vcp_skill` action `read`; SKILL.md summarizes each document type and adds VCP-authored README, release-note, changelog and postmortem structures.

Selected upstream source bytes:

- `skills/internal-comms/SKILL.md`: SHA-256 `067b7587a344a928fc6534ef66b1bcd591fc7c26d207ea7ca3334aeb678d6475`
- `skills/internal-comms/examples/general-comms.md`: SHA-256 `4d3a4bb198a77626bcf018e96b2b45a2dbabed172d4ade0fcd70d23ae8a47a47`
- `skills/internal-comms/examples/3p-updates.md`: SHA-256 `087e4363c0f3513728a7e695eeb9ead5c3ecd12a4681b59340691180e65b68fc`
- `skills/internal-comms/examples/faq-answers.md`: SHA-256 `5ecd3356cd6666937f2ebefa753253edfdbdca15e368d07baf398bfcced72484`
- `skills/internal-comms/examples/company-newsletter.md`: SHA-256 `30f81cfbdb03858a006169c72169024089c7c5d3d32611d337782da4f38c86b5`
