# Frontend design

Adapted from Anthropic's Apache-2.0 frontend-design skill; see [UPSTREAM.md](UPSTREAM.md) for the pinned source and modifications.

## Start with the subject and the existing product

Use this skill to build or reshape an interface. Identify the audience, the primary job and the real content from the request and nearby screens. The subject's industry, materials and vocabulary should inform the design: a children's learning activity and an analyst's dashboard need different visual choices. Ask only when missing information changes the result; a small UI fix does not require a new design brief.

Inspect the owning component, framework, styling conventions and existing design tokens. Keep the user's stack and established identity: extend the existing tokens, Tailwind theme and component primitives instead of adding a parallel system. Load [references/theming-and-performance.md](references/theming-and-performance.md) when the work touches themes or dark mode, right-to-left or localized content, fonts, media or loading performance. When the brief gives a visual direction, follow it, including familiar styles. When the brief leaves room, make choices for this subject rather than importing a signature palette or template. Use actual content and realistic long labels throughout.

For a new screen, sketch a compact direction before coding: color roles, typography, hierarchy, layout and the one element that should be memorable. A few named colors and an ASCII wireframe are enough when useful. Review the direction against the brief, then build. Load [references/design-and-copy.md](references/design-and-copy.md) for a substantial redesign or new visual identity.

## Build the design and the interaction together

Typography carries personality. Choose one family or two clearly distinct families from available, approved assets; give them intentional roles, sizes, weights and line spacing. Aim for readable body lines under roughly 80 characters. Use spacing and alignment to communicate priority. Borders, labels and numbering should encode information: numbered markers fit a sequence, not an arbitrary collection.

Spend visual boldness in one place and keep the surrounding interface disciplined. Motion should explain an action or draw attention deliberately. Respect reduced motion. Watch CSS specificity and competing spacing rules as the implementation grows.

Write from the user's perspective. A person manages notifications, not webhook configuration. An action should keep the same name through the flow: a button labelled "Publish" produces a "Published" result. Error text explains what happened and a useful next action; empty states help someone begin.

Cover the relevant loading, empty, populated, invalid, submitting and failure states. Preserve entered data after recoverable errors and prevent accidental duplicate effects. Use semantic controls, accessible names, keyboard operation and visible, unobscured focus. Meet WCAG 2.2 AA for the colors, targets, layouts and states you change: text contrast, non-text contrast, 24 by 24 CSS px targets, reflow at 320 CSS px and text spacing. Load [references/accessibility.md](references/accessibility.md) for the thresholds, exceptions and how to report which checks ran.

## Finish with a usable result

Run available project checks and exercise the changed interaction in a permitted browser or preview when available. Inspect the rendered result if image viewing is supported; otherwise use layout/DOM checks and identify the visual review still needed. A saved screenshot alone does not establish that you saw it.

Deliver the working interface and a concise account of the changed behavior, checks run and any material limitation. Ordinary design work does not require a comparative model campaign, a receipt bundle or a bespoke evaluation harness. Use existing execution permissions; this guidance does not install fonts, frameworks or browsers, register tools, or authorize publishing.
