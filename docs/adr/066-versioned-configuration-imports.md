# ADR-066 — Versioned configuration imports under native ceilings

Status: selected for P10-02; qualification evidence is tracked separately.

## Decision

Import only explicit, pinned Codex and Gemini configuration subsets as data.
The first subset narrows tool allowlists and deadlines for existing, exactly
matched MCP registrations. It does not create executable registrations or change
provider metadata, credentials, workspace trust, hooks or skill discovery.
[ADR-014](014-foreign-compatibility.md) remains the compatibility authority.

Neither upstream format requires a schema-version field. The caller therefore
selects a named compatibility profile bound to an immutable upstream revision.
Unknown profiles are rejected. A bounded parser reports unsupported fields and
identity conflicts; it never invokes upstream loaders, expands environment
variables, follows includes, resolves remote references or runs commands.

The native profile remains unchanged and supplies current authority ceilings.
An adjacent `.vcp-imports` directory contains bounded immutable preference
revisions. Each revision binds the preceding digest, base-profile digest, source
format/version/hash/root, selected fields and normalized restrictions. Raw source
and profile bytes are not copied into the journal. Preview is read-only and
contains only safe mappings, diagnostics and identity metadata.

Apply recomputes the preview while the source and native profile are pinned,
checks the selected IDs and expected revision, then publishes one synced complete
revision with a native create-only rename under an exclusive owner lock. Staging
files do not select configuration. A reader validates the contiguous digest chain
and selects its last complete revision. An interrupted first directory creation
with no owner or entries leaves the original configuration selected.

The profile loader applies allowlist intersection and minimum deadlines, so a
preference cannot widen its native registration. A changed base profile requires
a fresh preview before imported preferences can be used. Rollback appends a new
revision, interpreting the requested old preferences under the current native
profile. It cannot restore a removed server, tool grant, endpoint or credential.

## Alternatives and limits

Replacing the user's profile was rejected because arbitrary editors do not
participate in VCP's lock and the file may contain sensitive native settings.
Immutable preference revisions preserve that file and avoid copying its secrets.
Reusing the repository mutation adapter was rejected because its held-file copy
path does not promise atomic replacement on interruption.

Direct model-name mapping was rejected: VCP requires qualified endpoint,
capability and pricing evidence. Codex individual skill selectors do not identify
VCP discovery roots. Both receive explicit unsupported diagnostics. MCP includes
cannot widen current tool grants; commands and credentials remain separate setup.
These exclusions are visible partial compatibility, not drop-in import support.

The initial persistence implementation is native Windows. Process-kill tests
qualify application interruption, not hardware power loss or additional hosts.
The journal is bounded; automatic history pruning is outside this increment.

## Evidence

The [import guide](../development/configuration-imports.md) records exact fields,
commands and exclusions. [P10-02](../plan/19-deferred-extensions-and-platforms.md#p10-02--configuration-import)
requires synthetic golden, hostile input, stale revision, authority and native
publication/rollback tests before completion.
