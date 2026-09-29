# Design direction and interface writing

Adapted from `skills/frontend-design/SKILL.md` in the pinned Anthropic source listed in [../UPSTREAM.md](../UPSTREAM.md). These are design choices to consider, not rules that override the user's brief.

## Choose a point of view

Open with the most characteristic thing in the subject's world, in the form that serves it: a headline, image, live demonstration, interactive moment or another treatment. A large number, small label and gradient accent should be a choice justified by the content.

For a new visual system, capture a small palette with semantic roles, typeface roles, a layout concept and alignment guidance. Compare a couple of sketches when the composition is uncertain. Ask whether the same plan would emerge for an unrelated product; revise any choices that are just habit. Reuse the established system when evolving an existing product.

Familiar treatments can work when they fit the brief. Warm cream and display serifs, dark surfaces with a bright accent, newspaper columns or rounded SaaS cards are not substitutes for a direction. Avoid repeating identical cards, shadows and radii where the content has different importance. A memorable element is stronger when every surrounding element does not compete with it.

## Make the typography do useful work

Set a coherent scale and a small number of weights. Treat headline typography as part of the composition. Choose line lengths and line heights for actual content and viewport size; serif body copy often benefits from more leading. Avoid decorative labels above every heading, isolated accented headline words and all-cap labels unless they solve a particular hierarchy or brand need.

Check real rendered text for clipping, awkward wraps and overflow. Long labels, localized strings, missing images and dense data should not break the composition. A narrow screen may need a different arrangement, not just smaller text.

## Keep structure and motion meaningful

Rules, borders, labels and dividers should clarify relationships. Number items when their order matters. Use white space to separate groups before adding more containers. Keep interactive and static surfaces visually distinguishable.

A single coordinated entrance can be more effective than fade-and-slide effects on every section. Interaction-driven motion is useful when it shows what opened, expanded or changed. Remove decorative motion that distracts from the task, and provide a stable reduced-motion experience.

## Write for someone using the product

Words help people understand and act. Name features by what people recognize rather than internal service or implementation names. Be specific before being clever. Match the voice to the audience, using plain verbs and sentence case.

Give each element one job. A heading identifies the content; supporting text adds needed context; a button names the action. Prefer "Save changes" over "Submit" when that is what happens. Keep the action vocabulary consistent through confirmation, progress and completion.

An error explains the problem and how to recover without losing work. An empty state explains how to create the first useful result. Do not invent endorsements, business statistics, organizational facts or live data to make a page look finished; clearly label illustrative content.

## Critique the delivered screen

Review the primary task, hierarchy, alignment, spacing, copy and complete states together. Inspect actual rendering when the environment supports it. Remove decoration that does not serve the brief. A project build proves compilation; keyboard interaction, layout measurements and visual review answer different questions. Use the checks available for this task and report relevant gaps plainly.
