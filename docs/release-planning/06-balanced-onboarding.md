# BETA-03 follow-up: optional-project account setup

Owner direction: October 2, 2026. Implement the reviewed setup plan with balanced
quality and cost as the initial preference. This increment follows BETA-03 and
supersedes the reverted October 2 guided setup design; it does not authorize a
release, deployment, or a paid evaluation campaign.

## BETA-03C — reusable provider compatibility

Dependencies: BETA-03, P2 provider accounting, P6 conservative routing.

Separate the adapter compatibility contract from fresh endpoint metadata and
the user's connection test. Reusable evidence must identify its scope and
limitations. Fresh metadata must still validate exact model/endpoint identity,
capabilities, availability, prices and bounded expiry. Do not infer model
quality, tokenizer guarantees or an entire set's live qualification from a
single response. Retain canonical budget reservation and unknown liabilities.

## BETA-03D — balanced account setup and model sets

Dependency: BETA-03C. Contracts: ADR-041 conservative monetary admission and
the existing provider, workspace trust and immutable task routing boundaries.

Acceptance:

- A first interactive bare launch offers a short interview once per Windows
  user. Interrupted or failed setup remains incomplete. Explicit help requires
  neither account state nor a project. Later bare launches equal `vcp --help`.
- Configure an OpenRouter key using hidden input and opt-in per-user credential
  storage or a designated environment variable, model preferences and a default
  task budget. Project selection is
  optional; configuring it requires explicit workspace trust.
- Present balanced quality/cost by default. Allow choosing sets by maker or
  coding-project type, inspecting role assignments and customizing selections.
- Run one small, explicitly budgeted connection prompt. Display its response,
  model and reported cost; identify exactly what it verifies. Mark setup
  complete only following success. Do not silently retry paid inference.
- `vcp setup` revisits configuration. Attended `vcp models` reuses the chooser;
  explicit model subcommands support scripts. `vcp models` inspects/changes account
  defaults and project overrides. Explicit profile selection has precedence;
  existing tasks retain their captured selections.
- Actual task dispatch follows selected roles and only eligible alternatives
  inside the selected set and task budget. Outside-set alternatives require
  explicit permission; no gateway fallback can silently expand the selection.

Verification: focused native/synthetic tests for first-run detection,
interruption, help equivalence, credentials, precedence, dispatched selections,
fallback restrictions and budget enforcement; format/static checks, diff review
and normal delivery checks. Live paid and final installed-product evidence must
be reported separately when unrun.

Status: implementation in progress.
