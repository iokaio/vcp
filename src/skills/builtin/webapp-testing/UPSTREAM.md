# Upstream attribution and modifications

Adapted from Anthropic, PBC's [webapp-testing skill](https://github.com/anthropics/skills/tree/8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4/skills/webapp-testing), revision `8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4`. The per-skill Apache-2.0 license was examined and is copied unchanged from the Git commit to [LICENSE.txt](LICENSE.txt). Copyright 2026 Anthropic, PBC. VCP adaptation: 2026-09-29, VCP contributors. No endorsement is implied.

## Original source bytes

These SHA-256 values identify raw Git commit contents, before checkout line-ending conversion. Shipped adapted bytes have separate descriptor hashes.

- `skills/webapp-testing/SKILL.md`: `51b7349e77ec63b7744a6f63647e7566a0b4d2e301121cc10e8c2113af6556a2`
- `skills/webapp-testing/examples/element_discovery.py`: `d63c89604a22f8845d724e95dda45db49b1bf57c25ce0a83afbb7b8da3d402f0`
- `skills/webapp-testing/examples/console_logging.py`: `ea46877289acb82da7e7ce59d0bc37c8977cd57e2a006d0c88d7a1c625bf95da`
- `skills/webapp-testing/LICENSE.txt`: `bc6b3af2f331cbc7fb0da1344efb2cbe5877a31498b4d70dbc7000f3405a1362`

## VCP modifications

- SKILL.md and references/browser-patterns.md port the static/dynamic testing decision, rendered reconnaissance, selector discovery, actions, screenshots, console diagnostics and owned-browser cleanup.
- scripts/check-page.cjs translates the upstream Python Playwright discovery and console examples to Node and the project-installed Playwright module. VCP adds locator/state waits, one button action with an exact text assertion, bounded diagnostic output, explicit screenshot output and finally cleanup.
- The helper opens a new context with Chromium sandboxing enabled, blocks service workers, sockets, off-origin requests and all redirects, and never installs dependencies, starts a server or reuses a signed-in browser profile. Request routing is a test guard, not a substitute for host network/process enforcement.
- Blanket networkidle and fixed sleeps are replaced by observable application waits. The upstream shell=True server wrapper was inspected and intentionally not copied; project/VCP process ownership remains authoritative.
- Removes historical CS-3 receipt/qualification prerequisites for ordinary skill development while preserving authorization, package integrity and accurate statements of what was run.

## Validation scope

The adapted helper has been exercised against synthetic local HTML with an existing Playwright 1.61.1 and Chromium 149.0.7827.55 on Windows. This demonstrates the helper's tested Playwright workflow, not qualification of a VCP native browser adapter. A restricted host token closed Chromium before page creation; the checks passed outside that host sandbox with Chromium's own sandbox still enabled. No dependencies or browsers were downloaded. Use the available project/host permissions and report missing execution prerequisites.
