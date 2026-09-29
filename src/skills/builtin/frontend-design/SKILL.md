---
name: frontend-design
description: Design and implement interfaces with a deliberate visual direction, clear copy, responsive layout and complete interaction states in the user's existing stack.
---

# Frontend design

Adapted from Anthropic's Apache-2.0 frontend-design skill; see [UPSTREAM.md](UPSTREAM.md) for the pinned source and modifications.

## Start with the subject and the existing product

Use this skill to build or reshape an interface. Identify the audience, the primary job and the real content from the request and nearby screens. The subject's industry, materials and vocabulary should inform the design: a children's learning activity and an analyst's dashboard need different visual choices. Ask only when missing information changes the result; a small UI fix does not require a new design brief.

Inspect the owning component, framework, styling conventions and existing design tokens. Keep the user's stack and established identity. When the brief gives a visual direction, follow it, including familiar styles. When the brief leaves room, make choices for this subject rather than importing a signature palette or template. Use actual content and realistic long labels throughout.

For a new screen, sketch a compact direction before coding: color roles, typography, hierarchy, layout and the one element that should be memorable. A few named colors and an ASCII wireframe are enough when useful. Review the direction against the brief, then build. Load [references/design-and-copy.md](references/design-and-copy.md) for a substantial redesign or new visual identity.

## Build the design and the interaction together

Typography carries personality. Choose one family or two clearly distinct families from available, approved assets; give them intentional roles, sizes, weights and line spacing. Aim for readable body lines under roughly 80 characters. Use spacing and alignment to communicate priority. Borders, labels and numbering should encode information: numbered markers fit a sequence, not an arbitrary collection.

Spend visual boldness in one place and keep the surrounding interface disciplined. Motion should explain an action or draw attention deliberately. Respect reduced motion. Watch CSS specificity and competing spacing rules as the implementation grows.

Write from the user's perspective. A person manages notifications, not webhook configuration. An action should keep the same name through the flow: a button labelled "Publish" produces a "Published" result. Error text explains what happened and a useful next action; empty states help someone begin.

Cover the relevant loading, empty, populated, invalid, submitting and failure states. Preserve entered data after recoverable errors and prevent accidental duplicate effects. Use semantic controls, meaningful labels, keyboard navigation and visible focus. Check narrow and wide layouts and long content. Check contrast for the colors and states you change rather than relying on an attractive palette.

## Finish with a usable result

Run available project checks and exercise the changed interaction in a permitted browser or preview when available. Inspect the rendered result if image viewing is supported; otherwise use layout/DOM checks and identify the visual review still needed. A saved screenshot alone does not establish that you saw it.

Deliver the working interface and a concise account of the changed behavior, checks run and any material limitation. Ordinary design work does not require a comparative model campaign, a receipt bundle or a bespoke evaluation harness. Use existing execution permissions; this guidance does not install fonts, frameworks or browsers, register tools, or authorize publishing.
