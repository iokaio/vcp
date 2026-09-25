# Scoped update to Orchard review guidance
Advance orchard-note-maintenance from 1.3.0 to 1.3.1. The project now tracks planned-change checkpoints by a local issue reference instead of a calendar date. Update references/planned-changes.md to require a named owner and a local issue link that exists inside the supplied project. Report a missing linked issue as unavailable; do not create the issue or replace it with a made-up date.

Keep the rule that delivered changes cite implementation evidence and separately identify pass, fail or not_run test evidence. Keep all other instructions and metadata unchanged. Preserve package/SKILL.md and references/delivered-changes.md byte-for-byte. The only descriptor value changes are version and the sha256 for references/planned-changes.md. Do not append a release entry or revise historical version records.

Only package/skill.json and package/references/planned-changes.md may change. Keep the resource below 6000 UTF-8 bytes. Report what changed and the verification actually available. Do not install, activate, publish or claim runtime qualification.
