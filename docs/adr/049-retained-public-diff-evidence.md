# ADR-049: Retained public diff evidence

Date: 2026-09-23
Status: accepted for the diff inspection increment of P9-02.

## Decision

`diff/read` selects a canonical tool effect by its existing `ToolRunId`, in the
authenticated workspace/session/task scope. It returns bounded bytes from a
separately captured `vcp-public-diff/1` document. It never reconstructs a change
from the current filesystem or rewrites a private artifact under its original
identity and hash.

The registered file-tool producer captures the typed public document alongside
its existing private prepared evidence. The initial `Validated` effect fact links
both artifacts before approval or dispatch. Later effect transitions may replace
the current observed-change list, so lookup uses that exact retained initial fact,
with current scope, redaction and retention checks. Missing or ambiguous linkage
is unavailable, including historical proposals made before this producer existed.

The public document contains scope, effect identity, operation digest, source
artifact identity and proposed file changes. Paths are relative; before/after
content carries SHA-256 and base64 bytes. Its disposition is explicitly `proposed`.
It does not assert application, verification or success. Private process ownership,
host denials and native filesystem identities are not part of this document.

Lookup verifies the full hashes of both captures, reconstructs the operation
digest and compares every public file change with the original typed preparation.
It then uses the existing authorized artifact-range path, including its response
size bound and full-capture hash. Parsing is bounded by actual prepared-tool limits
and streams encoded spool data rather than collecting the encoded document whole.
No new permission, mutation or live filesystem read is introduced by inspection.

The profile permits at most 64 files, 1 MiB per before/after value and 4 MiB total
after-content, matching prepared file-tool limits. Public base64 captures are
bounded to 112 MiB; legacy private preparation uses decimal byte arrays and has
a 304 MiB encoded bound. A streaming guard rejects any JSON string token over
1,398,104 encoded bytes before the JSON decoder can accumulate it. A range read
still verifies whole captures; its work is bounded by these source limits, not
only by the small returned range. The existing workspace base64 crate supplies
the codec without a version upgrade.

## Consequences

Retained proposal bytes remain stable after workspace edits and later effect
outcomes. Clients discover public capture references from event evidence, read
their effect identity, and use that identity for subsequent `diff/read` ranges.
They must inspect canonical outcome state separately before claiming a change
was applied. Process and MCP proposals without typed file changes do not acquire
a fabricated diff.

Tests cover both stores, exact scope and source linkage, retention and corruption,
bounded ranges and the compiled file-tool producer. This increment does not close
the remaining P9-02 adapters or SDK qualification.
