# Project conventions, theming, internationalization and performance

VCP-authored reference; not part of the upstream Anthropic skill. Apply the sections that the task touches. The accessibility baseline, including contrast and reduced motion, is in the skill body; success-criterion detail is in [accessibility.md](accessibility.md).

## Work inside the project's system

Find the existing styling approach before writing styles: Tailwind configuration and theme tokens, CSS custom properties, Sass variables, CSS modules, CSS-in-JS themes, and the component library or local primitives (for example shadcn/ui, Radix, MUI, Chakra or an in-house `Button`). Extend those. Add a token to the Tailwind theme or the variables file rather than hard-coding a hex value or arbitrary pixel value in one component; compose the existing `Button`, `Dialog` or form field rather than writing a parallel one.

Do not introduce a new CSS framework, component library, icon set, font service or animation library for a scoped change. When the existing system cannot express the design, make the smallest extension in the system's own style and mention it. Follow the project's lint, formatting, class-ordering and file-layout conventions. A new visual direction for an established product changes tokens and primitives, not scattered overrides.

## Themes and dark mode

- Define color as semantic tokens (`--color-surface`, `--color-text-muted`, `--color-border`, `--color-accent`, `--color-danger`) and map raw palette values to them per theme. Components use the semantic tokens, never raw palette steps.
- Follow the project's theme mechanism. Without one, default to the system preference with `@media (prefers-color-scheme: dark)` and set `color-scheme: light dark` so form controls and scrollbars match. When the user can choose a theme, store the choice and apply it before first paint to avoid a flash of the wrong theme.
- A dark theme is a separate palette, not an inversion: use dark grey surfaces, express elevation with lighter surfaces rather than shadows alone, reduce saturation of large accent areas, and recheck images, logos, charts and illustrations against the dark surface.
- Recheck contrast in every theme and state, including any high-contrast mode the project supports. A pair that passes in light often fails in dark. Respect `forced-colors: active` by avoiding information carried only by background images or box shadows.

## Right-to-left and internationalization

- Use logical CSS properties and values: `margin-inline-start`, `padding-inline`, `inset-inline-end`, `border-start-start-radius`, `text-align: start`. Set `dir` and `lang` on the root or on mixed-direction regions; Flexbox and Grid follow the direction automatically.
- Mirror directional icons (back, forward, progress chevrons) in right-to-left layouts; do not mirror logos, media controls with fixed meaning, or checkmarks.
- Leave room for expansion: translated strings are commonly 30% or more longer than English, and short labels can double or triple. Avoid fixed widths on buttons, tabs and table headers; test with long strings and pseudo-localization when the project provides it.
- Format dates, times, numbers, currency, lists and plurals with the locale APIs (`Intl.DateTimeFormat`, `Intl.NumberFormat`, `Intl.PluralRules`, `Intl.ListFormat`) or the project's i18n library. Never concatenate translated fragments into a sentence.
- Do not put text in images or SVG paths; keep it as real text so it can be translated, resized and read by assistive technology. Check that chosen fonts cover the scripts the product ships in.

## Loading and performance

- **Fonts:** use the fonts the project already ships or has approved. Limit families and weights, prefer variable fonts or subsets for the scripts in use, self-host or preload the critical face, and set `font-display: swap` (or `optional` for non-critical faces). Pick a fallback with similar metrics, or use `size-adjust`, so swapping does not shift layout.
- **Layout shift:** reserve space for images, video, embeds, ads and asynchronously loaded content with `width`/`height` attributes or `aspect-ratio`. Use skeletons that match the final dimensions. Do not insert banners above existing content after load.
- **Images:** serve responsive sizes with `srcset` and `sizes` (or the framework's image component), modern formats such as AVIF or WebP where supported, `loading="lazy"` for below-the-fold images and eager loading for the main above-the-fold image.
- **Animation:** animate `transform` and `opacity` rather than layout properties; keep durations short; avoid large blurs, many simultaneous animations and scroll-linked effects on long pages.
- **Code and assets:** do not add a heavy dependency for a small visual effect. Load large, rarely used components on demand when the framework supports it.
- Measure with the tools available (Lighthouse, the browser performance panel, project budgets) and report what was measured and what was not; a local development build is not a production measurement.
