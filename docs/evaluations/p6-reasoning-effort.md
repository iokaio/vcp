# P6-05 selected reasoning effort

The selected optimizer policy accepts `minimal`, `low`, `medium`, and `high`, in
that order. These are the four efforts explicitly listed by the primary
[OpenRouter Responses reasoning documentation](https://openrouter.ai/docs/api_reference/responses/reasoning).
The [general reasoning guide](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens)
describes additional model-dependent values; those values are not accepted by
this Responses contract. No provider default effort is inferred.

Effort is an optional typed policy value. A requested effort is bounded by the
trusted configuration's effort ceiling. Without a trusted effort configuration,
the effective value is absent and preview exposes that result. An inherited
value uses the trusted effort when configured. Legacy policies and compatibility
records omit absent/empty fields, preserving their serialized digests.

An explicit effort requires an exact endpoint compatibility record qualifying
that effort, with `reasoning` in the required provider parameters and observed
endpoint parameters. Selection excludes unqualified candidates, including
strictly pinned candidates. Generic model names or support for a `reasoning`
parameter alone do not establish qualification. Existing live profiles have no
qualified efforts until an exact endpoint/effort probe supplies evidence.

The Responses codec writes `reasoning.effort` into the canonical request before
context sealing and reservation. Admission revalidates the sealed request using
the effective effort. The output token ceiling and cost reservation continue to
bound the complete output; reasoning does not create another budget or increase
the output ceiling. Provider policy still requires parameters and disables
fallback routing.

## Evidence

- `vcp-models` test
  `reasoning_effort_requires_exact_qualification_preserves_legacy_bytes_and_output_bound`:
  passed; checks qualification, provider parameters, legacy serialization and
  bytes, enum boundaries, exact body, and unchanged output ceiling.
- `vcp-models` test
  `explicit_reasoning_effort_filters_unqualified_endpoints_without_relaxing_a_pin`:
  passed; checks exact effort selection and strict pin failure.
- The synthetic canonical host test
  `routing_effort::effort_is_qualified_selected_sealed_and_bounded_without_changing_output_reservation`
  covers Files/SQLite preview and apply, trusted clamping, absent trusted effort,
  unsupported effort with no admission, actual request serialization, settlement,
  and policy mutation after send. It passed in the four-test native routing run
  recorded at `artifacts/p6-routing-native-stack16.log`, using the repository's
  standard `RUST_MIN_STACK=16777216` and `CODEX_TEST_ENVIRONMENT=local` settings.

These fixtures qualify implementation behavior, not any live model's reasoning
quality or endpoint support.
