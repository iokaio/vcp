# MCP exact numeric and schema profile

`mcp-schema/2` is the bounded admission profile selected by
[ADR-028](../adr/028-exact-mcp-schema-profile.md). It extends the initial integer
profile while preserving the existing stdio/HTTPS dispatch boundary. The
[qualification report](../evaluations/p7-03-numeric.md) distinguishes pure and
native evidence. This profile is a supported subset, not a full JSON Schema
implementation.

## Exact values

JSON number tokens are parsed as sign, coefficient digits and decimal exponent.
There is no binary-float conversion. Limits apply before validation and after
normalization:

| Bound | Profile limit |
| --- | ---: |
| Original numeric token | 256 bytes |
| Nonzero normalized coefficient | 128 digits |
| Explicit and normalized exponent magnitude | 128 |
| Schema/argument JSON defaults | 128 KiB, 16 levels, 4,096 values |
| Complete frame ceilings | 1 MiB, 32 levels, 65,536 values |
| Schema/argument numeric digit allowance | 16,384 |
| Schema/argument work allowance | 1,000,000 units |
| Frame numeric digit/work ceilings | 65,536 / 10,000,000 |

Each parse and its subsequent schema validation share the remaining work budget.
Numeric comparison, division remainder, recursive equality and branch traversal
consume it. Reaching a limit rejects the candidate; it never produces an
approximate value or an assumed branch result.

Normalization removes redundant coefficient zeroes and maps all zero forms to
`0`. Exact integers whose expanded spelling fits 128 digits use integer text;
other values use coefficient/exponent form. For example, `1.00` and `1e0` become
`1`, `0.3` becomes `3e-1`, and `1e128` stays exponent-form. Existing i64/u64 integer
spellings retain their bytes. Input and normalized-output byte ceilings both
apply. Numeric-looking resource/prompt text remains an unchanged string.

Numeric callback IDs, error codes and resource sizes still require exact integral
values within their existing i64/u64 bounds. Thus an admitted callback ID `1.0`
is echoed as `1`; fractional or oversized IDs fail. VCP request IDs remain strings.

## Schema subset

Input/output roots must explicitly declare `type: object`. Nested schemas may be
boolean or combine the supported predicates below. An explicit `$schema` must be
the exact `https://json-schema.org/draft/2020-12/schema` URI.

| Feature | Supported profile |
| --- | --- |
| Types | Object, array, string, number, integer, boolean, null; a scalar string/number/integer/boolean type plus null |
| Numbers | Exact minimum/maximum, exclusive bounds and positive multipleOf |
| Objects | Properties, required names, boolean or schema additionalProperties |
| Arrays | Uniform items, min/maxItems and exact recursive uniqueItems |
| Strings | min/maxLength in Unicode scalar values |
| Equality | Bounded enum and const; recursive numeric equality |
| Composition | Nonempty allOf/anyOf/oneOf arrays and unary not |
| References | Root $defs and acyclic local references to those definitions |
| Inert annotations | title, description, $comment, default and bounded examples |

Integer validation uses mathematical integrality, and numeric equality treats
equivalent decimal spellings equally. Type-specific predicates apply only to
their instance type. These follow the admitted
[validation semantics](https://json-schema.org/draft/2020-12/json-schema-validation).
Reference siblings are conjunctive. additionalProperties considers properties in
the same schema object; property sets are not merged across references or allOf.
See the [core specification](https://json-schema.org/draft/2020-12/json-schema-core).

The compiled graph is limited to 1,024 nodes, 256 references, 64 levels and 16
branches per composition. Definitions, properties, required names, enum values
and example arrays are bounded to 128 members. Definition names are at most 256
bytes. All definitions are checked, including unused ones; cycles and unresolved
references reject publication.

References have the form `#/$defs/<name>`, with only JSON Pointer `~0` and `~1`
decoding. Raw name characters are ASCII letters/digits and `-._~!$&'()*+,;=:@?`;
slash must use `~1`. Percent escapes, whitespace, control characters, non-ASCII,
backslash and additional `#` reject even if a matching definition key exists.
There is no URI resolution, percent decoding, anchor lookup or nested definition
scope.

Unknown keywords reject recursively, including in an unselected alternative.
Unsupported features include regex/pattern, format, patternProperties,
propertyNames, dependent or unevaluated predicates, prefixItems, contains,
conditionals, remote/dynamic references, anchors, `$id` and custom vocabularies.
Sampling, roots, elicitation and tasks remain disabled protocol capabilities.

## Identity and retained data

Connections pin `admission_profile: "mcp-schema/2"`; schema hashes also include
the profile prefix and a NUL separator before canonical schema bytes. New
normalization therefore cannot reuse a profile-1 schema identity. Existing
receipt bytes, approvals and cached observations are not rewritten or promoted.

MCP admission rejects decoded duplicate keys and literal serde-private Number/
RawValue marker keys. The persistence codec deliberately preserves those keys in
older arbitrary record/event data; it is a different boundary with a different
contract. The model decision reader also remains separate: its numbers retain
serde's representation for existing probability and exact monetary conversion,
without MCP normalization or schema predicates.

## Known-secret capture

For a credential or session value that is itself an admitted numeric token, the
capture filter also protects its exact normalized form. Numeric scalar checks
compare normalized forms, covering serde exponent spelling changes as well as
MCP normalization. Original literal matching still applies to strings and keys.
Both protected forms participate in prepared-operation screening, result capture
and residual checks; the derived form is held in zeroizing private storage.

A sensitive discovery identity is rejected. A sensitive observed result may be
omitted while retaining the fact that a reply was observed. Known-secret arguments
are rejected before canonical proposal capture. Conservative matches can reject
otherwise ordinary content when it overlaps a protected form.
