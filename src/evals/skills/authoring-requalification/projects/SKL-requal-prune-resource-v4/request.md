# Remove obsolete API review guidance

Advance `api-change-review` from 2.2.0 to 2.2.1. Remove the obsolete
`references/legacy-checklist.md` resource and its link and instructions from the
body. `references/compatibility.md` supersedes it and remains unchanged.

Only `package/skill.json` and `package/SKILL.md` may be modified, and only
`package/references/legacy-checklist.md` may be deleted. Update the body hash,
remove exactly the legacy resource entry, and keep every other descriptor value
and remaining resource hash unchanged. Preserve the request, caller inventory,
history, and compatibility resource. Do not append a history entry.

This maintenance does not authorize installation, activation, publication, or a
claim of runtime qualification. Report only checks actually performed.
