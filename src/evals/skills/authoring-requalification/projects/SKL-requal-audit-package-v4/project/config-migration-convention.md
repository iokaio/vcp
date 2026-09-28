# Local configuration migration review convention

Use this review only for a proposed migration between versioned configuration
schemas. Identify affected and unaffected users, map every removed or renamed
field, identify defaults that change meaning, and require a rollback path before
calling a plan ready.

Delivered behavior requires local implementation evidence. Validation results
must be reported separately as pass, fail, or not_run with their environment.
An example is never evidence for the current project. A missing source remains
unavailable; do not fabricate a replacement or call it a pass.

Return review findings. Do not modify configuration, run a migration, contact an
owner, or publish without separate authority. An ordinary one-field config edit
that does not change schema is outside this reusable workflow.
