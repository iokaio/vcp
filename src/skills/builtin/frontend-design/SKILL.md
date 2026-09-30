# Frontend design

Adapted from Anthropic's Apache-2.0 frontend-design skill; see [UPSTREAM.md](UPSTREAM.md) for the pinned source and modifications.

## Start with the subject and the existing product

Use this skill to build or reshape an interface. Identify the audience, the primary job and the real content from the request and nearby screens. The subject's industry, materials and vocabulary should inform the design: a children's learning activity and an analyst's dashboard need different visual choices. Ask only when missing information changes the result; a small UI fix does not require a new design brief.

Inspect the owning component, framework, styling conventions and existing design tokens. Keep the user's stack and established identity: extend the existing tokens, Tailwind theme and component primitives instead of adding a parallel system. When the brief gives a visual direction, follow it, including familiar styles. When the brief leaves room, make choices for this subject rather than importing a signature palette or template. Use actual content and realistic long labels throughout.

For a new screen, sketch a compact direction before coding: color roles, typography, hierarchy, layout and the one element that should be memorable. A few named colors and an ASCII wireframe are enough when useful. Review the direction against the brief, then build.

## Build the design and the interaction together

Typography carries personality. Choose one family or two clearly distinct families from available, approved assets; give them intentional roles, sizes, weights and line spacing. Aim for readable body lines under roughly 80 characters. Use spacing and alignment to communicate priority. Borders, labels and numbering should encode information: numbered markers fit a sequence, not an arbitrary collection.

Spend visual boldness in one place and keep the surrounding interface disciplined. Motion should explain an action or draw attention deliberately. Watch CSS specificity and competing spacing rules as the implementation grows.

Write from the user's perspective. A person manages notifications, not webhook configuration. An action should keep the same name through the flow: a button labelled "Publish" produces a "Published" result. Error text explains what happened and a useful next action; empty states help someone begin.

Cover the relevant loading, empty, populated, invalid, submitting and failure states. Preserve entered data after recoverable errors and prevent accidental duplicate effects.

## Accessibility baseline

Meet WCAG 2.2 AA for every color, target, layout and state you change. These rules apply whether or not you read the accessibility reference; follow a stricter project or legal standard when one applies.

- **Contrast:** text at least 4.5:1 against its background. Large text (at least 24 CSS px, or about 18.7 CSS px bold) and the non-text parts needed to identify controls, their states, focus indicators and meaningful graphics at least 3:1. Measure the rendered pairs in every theme and state you change. Color is never the only signal.
- **Targets:** pointer targets at least 24 by 24 CSS px unless a WCAG exception applies; prefer larger targets on touch-first surfaces.
- **Keyboard and focus:** every action works from the keyboard without a trap, focus order preserves meaning, and focus is always visible and not hidden behind sticky headers or banners.
- **Semantics:** use native controls with accessible names that contain the visible label, programmatic form labels (a placeholder is not a label), text alternatives for meaningful images and announced status messages.
- **Layout:** content reflows at 320 CSS px without two-dimensional scrolling and survives 200% text and user text-spacing overrides without clipping.
- **Motion:** honor `prefers-reduced-motion: reduce` by replacing movement, parallax, zoom and large transitions with an instant change or a short opacity fade that keeps the information. Let people pause automatic motion that lasts more than five seconds, and never flash more than three times a second.

## Read references when the task needs them

These references are not in your context. Read one with `vcp_skill` (action `read`, skill `frontend-design`, resource the path shown), for example `{"action":"read","skill":"frontend-design","resource":"references/accessibility.md"}`. Read only what the task touches; a small fix usually needs none.

- [references/design-and-copy.md](references/design-and-copy.md): a new screen, substantial redesign or new visual identity. Covers point of view, palette, typography, template habits to avoid, motion choices, interface writing and a final critique.
- [references/accessibility.md](references/accessibility.md): success-criterion detail and exceptions behind the baseline, custom widget keyboard patterns, or reporting which accessibility checks ran.
- [references/theming-and-performance.md](references/theming-and-performance.md): themes or dark mode, design tokens and component libraries, right-to-left or localized content, fonts, images, layout shift and loading performance.

## Finish with a usable result

Run available project checks and exercise the changed interaction in a permitted browser or preview when available. Inspect the rendered result if image viewing is supported; otherwise use layout/DOM checks and identify the visual review still needed. A saved screenshot alone does not establish that you saw it. Automated accessibility tools find only a subset of failures: state which checks ran and which did not, and do not claim conformance that was not tested.

Deliver the working interface and a concise account of the changed behavior, checks run and any material limitation. Ordinary design work does not require a comparative model campaign, a receipt bundle or a bespoke evaluation harness. Use existing execution permissions; this guidance does not install fonts, frameworks or browsers, register tools, or authorize publishing.
