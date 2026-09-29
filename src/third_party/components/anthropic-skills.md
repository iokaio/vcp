# Anthropic skills source

SP-01 adds [anthropics/skills](https://github.com/anthropics/skills) at revision
`8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4` as a selectively ported upstream.
See the [per-skill license audit](anthropic-skills.json) and
[delivery plan](../../../docs/research/skillsplan-new/.md).

Six Apache-2.0 sources match the selected VCP workflow goals. Each port retains
its license and a package-local source/modification record. The repository has
no root LICENSE that grants rights to every directory. In particular,
`doc-coauthoring` has no explicit skill license and is excluded;
`docx`, `pdf`, `pptx`, and `xlsx` have restrictive service terms and are
excluded. The upstream THIRD_PARTY_NOTICES also lists GPL components: it is not
permission to bundle them. No upstream fonts, artwork, FFmpeg, or GPL/LGPL
components are selected. Review any newly selected dependency separately.

Copies are adapted into native VCP skill packages, not a second skill loader
or a vendored Claude runtime. Source snapshots and license digests identify the
reviewed revision; attribution never grants execution authority.
