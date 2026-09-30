# Upstream attribution and modifications

This VCP 2.0.0 skill package contains material adapted from Anthropic, PBC's [anthropics/skills](https://github.com/anthropics/skills) repository. Upstream attribution and license notices are retained.

- Upstream revision: `8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4`.
- Source directory: [skills/frontend-design](https://github.com/anthropics/skills/tree/8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4/skills/frontend-design).
- Per-skill license examined: Apache License 2.0, copied byte-for-byte to [LICENSE.txt](LICENSE.txt).
- Source reviewed and VCP adaptation recorded: 2026-09-29.
- Adaptation author: VCP contributors. No Anthropic endorsement is implied.

## Original source inventory

SHA-256 values below identify the original Git commit bytes, before adaptation or checkout line-ending conversion. The VCP descriptor separately hashes every shipped body/resource.

| Original path | Original SHA-256 |
| --- | --- |
| `skills/frontend-design/SKILL.md` | `d91970639e9f5c37682ac7ab60094d35f1c7c1f38d731bd56396563aee10c1d3` |
| `skills/frontend-design/LICENSE.txt` | `0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594` |

## VCP modifications

- The upstream YAML front matter is not retained; `skill.json` is the only package metadata, and its description drives discovery.
- SKILL.md and references/design-and-copy.md adapt the upstream subject-first design process, typography, layout, motion, restraint and interface-writing sections.
- VCP changes preserve the existing stack and user direction, make planning proportional to the UI task, add complete states and truthful preview checks, and remove compulsory client confirmation and claims about client preferences.
- No upstream fonts, branding, images or external tool installers are included.
- SU-13 (2026-09-29) re-compared the pinned upstream SKILL.md, which already includes the 2026-09-03 upstream change on avoiding generic design defaults (upstream commit `41bbe19d1a`). references/design-and-copy.md now also adapts the template-chrome calibration list (eyebrow labels, middle-dot meta strings, em-dash labels, tinted near-black, monospace data labels, appended arrows), deliberate typeface choice, a four-to-six color palette, stating what changed after plan review, avoiding hover transitions on every card, non-apologetic error text and removing one more element before finishing. The upstream brief-wins rule is retained. Upstream's reference to Anthropic's own interaction accent color is not carried over.
- SU-13 added two VCP-authored references that are not derived from the upstream skill: references/accessibility.md (WCAG 2.2 AA thresholds cited by success-criterion number from the W3C Recommendation of 12 December 2024, and truthful reporting of automated and manual checks) and references/theming-and-performance.md (project framework and component-library conventions, design-token theming and dark mode, right-to-left and internationalization, and loading and layout-shift performance). SKILL.md points to both.

Only the listed permissively licensed skill material is used. No material from the restricted document skills or unlicensed doc-coauthoring skill is included. Runtime permissions, explicit activation and VCP package integrity remain unchanged.
