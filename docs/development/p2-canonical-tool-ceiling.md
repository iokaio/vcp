# P2-05 canonical model tool ceiling

The owner execution profile may set `canonical_tools` to a subset of the seven
canonical model tools. Omission preserves the existing seven-tool default. An
empty array advertises no model tools; duplicate or unknown names fail profile
validation. For example:

```json
{"canonical_tools": ["vcp_read", "vcp_list", "vcp_search"]}
```

This is a narrowing ceiling, not an effect grant. Workspace trust, autonomy,
resource policy, prepared-call admission, child authority and effect checks still
apply independently. Allowing `vcp_exec` does not authorize a process invocation.

The host captures the selected set as `canonical-tool-ceiling/1` for the root
task. Retained root and child startup, coding configuration, outgoing tool
schemas and dispatch admission derive from that same set. A child cannot widen
it. Reopening the task requires the original selection; choosing another set
requires a new task. Startup also seals the legacy default before registering
the retained thread. Direct host coding setup seals its validated default before
becoming active, preserving subsequent child startup for existing callers.

Independent `canonical-coding-configuration/1` history across the root task
family detects a missing or changed ceiling record. Restricted history fails
closed. Legacy history without the field retains all seven tools and cannot be
retrofitted with a narrower set. Task-family membership is resolved from the
canonical task record within the same workspace and session.

Operating guidance and skill tool availability reflect the selected tools,
including process aliases. Excluding `vcp_verify` removes the model-callable tool
and the instruction to run it. Host-owned unchanged-analysis integrity checks
remain available. This does not make an editing task report-only or waive its
verification requirements.

Local regression coverage includes profile validation, durable identity and
missing-marker recovery on SQLite and file stores, retained provider schemas,
excluded-call admission, unchanged-analysis completion, executable CLI startup
and actual retained child provider requests. The fixture providers are local;
this prerequisite does not qualify or rerun a paid skill campaign.
