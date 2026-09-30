# Accessibility checks for interface work

VCP-authored reference; not part of the upstream Anthropic skill. Thresholds are from the W3C Recommendation [Web Content Accessibility Guidelines (WCAG) 2.2](https://www.w3.org/TR/WCAG22/) (12 December 2024), cited by success-criterion (SC) number and level. The SC text is authoritative; this page summarizes it for implementation. Follow a stricter project or legal standard when one applies.

## Contrast and color

- **Text, SC 1.4.3 (AA):** at least 4.5:1 against its background. Large-scale text needs at least 3:1. WCAG defines large scale as at least 18 point, or 14 point bold; with CSS's 1pt = 4/3px that is 24px, or about 18.7px bold. Very thin or unusual faces need more margin than the ratio alone suggests. Text in inactive components, pure decoration and logotypes is exempt.
- **Non-text, SC 1.4.11 (AA):** at least 3:1 against adjacent colors for the visual information needed to identify UI components and their states (input borders, checkbox marks, focus indicators, selected tabs) and for the parts of graphics needed to understand content (chart lines, icon-only controls). Inactive components and unmodified browser-default controls are exempt.
- **Use of color, SC 1.4.1 (A):** color is never the only signal. Pair error red, selected state or chart series color with text, an icon, a pattern or a shape.
- Measure the actual rendered foreground and background pairs, including hover, focus, disabled-looking-but-active, selected, error and placeholder text, text over images or gradients, and every theme. Check the lowest-contrast point of a gradient or image, not its average.

## Size, spacing and reflow

- **Target size, SC 2.5.8 (AA):** pointer targets are at least 24 by 24 CSS px. A smaller target passes only under an exception: *spacing* (a 24 CSS px diameter circle centered on each undersized target does not intersect another target or another undersized target's circle), *equivalent* (another control on the page does the same thing and meets the size), *inline* (a link in a sentence), *user agent control* (unmodified browser control) or *essential*. Prefer comfortable sizes (for example 44px on touch-first surfaces) when the layout allows; 24px is the floor.
- **Reflow, SC 1.4.10 (AA):** content works at a width equivalent to 320 CSS px (1280px at 400% zoom) without two-dimensional scrolling. Maps, diagrams, video, data tables and similar content that needs a two-dimensional layout may scroll in both directions within that region only.
- **Resize text, SC 1.4.4 (AA):** text can be resized to 200% without loss of content or function. Use relative units for type and avoid fixed-height containers around text.
- **Text spacing, SC 1.4.12 (AA):** nothing is clipped or overlaps when a user sets line height to 1.5 times the font size, paragraph spacing to 2 times, letter spacing to 0.12 times and word spacing to 0.16 times. Let text containers grow instead of fixing their height or width.

## Keyboard and focus

- **Keyboard, SC 2.1.1 (A), and no keyboard trap, SC 2.1.2 (A):** every action works from the keyboard without timed keystrokes (except path-dependent input such as freehand drawing), and focus can always leave a component. Custom widgets (menus, tabs, comboboxes, dialogs) need the expected keys; follow the [WAI-ARIA Authoring Practices](https://www.w3.org/WAI/ARIA/apg/) patterns or, better, use native elements.
- **Focus order, SC 2.4.3 (A):** focus moves in an order that preserves meaning. Move focus into an opened dialog and return it to the trigger on close.
- **Focus visible, SC 2.4.7 (AA):** a visible keyboard focus indicator exists. Do not remove outlines without a replacement; `:focus-visible` styles with at least 3:1 contrast against adjacent colors (SC 1.4.11) are a practical default.
- **Focus not obscured, SC 2.4.11 (AA):** a focused component is not entirely hidden by author content such as sticky headers, cookie banners or chat launchers. `scroll-padding` on the scroll container helps keep focused items in view.

## Names, roles and states

- **Name, role, value, SC 4.1.2 (A):** every control exposes an accessible name, a role and its current state. Native `button`, `a href`, `input`, `select` and `details` provide most of this; a clickable `div` does not.
- **Label in name, SC 2.5.3 (A):** the accessible name contains the visible label text, so voice-control users can say what they see. Icon-only buttons need an accessible name (`aria-label` or visually hidden text).
- **Info and relationships, SC 1.3.1 (A), and labels or instructions, SC 3.3.2 (A):** use headings, lists, landmarks, table headers and programmatically associated form labels. Placeholder text is not a label.
- **Non-text content, SC 1.1.1 (A):** meaningful images have text alternatives; decorative images use empty `alt=""`.
- **Status messages, SC 4.1.3 (AA):** announce results that do not move focus (saved, error count, search results) through a live region or `role="status"`/`role="alert"`.

## Motion

- **Pause, stop, hide, SC 2.2.2 (A):** motion that starts automatically, lasts more than five seconds and runs alongside other content needs a way to pause, stop or hide it.
- **Three flashes or below threshold, SC 2.3.1 (A):** nothing flashes more than three times in any one-second period unless the flash is below the general and red flash thresholds. Avoid flashing content entirely.
- Honor `@media (prefers-reduced-motion: reduce)`: replace movement, parallax, zoom and large transitions with an instant change or a short opacity fade. Keep the information that the motion conveyed. WCAG treats disabling interaction-triggered motion as AAA (SC 2.3.3), but the media query is inexpensive and expected in VCP work.

## Report what was checked

Automated tools such as axe, Lighthouse or eslint-plugin-jsx-a11y find a subset of failures. A clean automated result does not prove WCAG conformance: it cannot judge focus order, meaningful names, reading order, whether color alone carries meaning, or most visual problems. State which checks ran, for example "axe: 0 violations on the changed page", "keyboard walkthrough of the form", "contrast measured for text and borders in light and dark themes", and list the checks not run, such as screen-reader testing or visual review at 320px and 200% text size. Do not claim conformance or screen-reader compatibility that was not tested.
