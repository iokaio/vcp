# Requested VCP package
Create an original guidance package for the Orchard change-note review convention. Package ID orchard-note-review, version 1.0.0; description should state its narrow local review purpose.

Use exactly these descriptor fields: schema_version (1), id, version, description, source (vcp-original), license (Apache-2.0), vcp_version (1), cues ([]), environments ([]), required_tools ([vcp_list, vcp_read]), body and resources. body refers to SKILL.md; resources contains exactly one reference to references/review-checklist.md. Each reference has only path and sha256. SHA-256 is lowercase hexadecimal over the exact file bytes. Paths are relative to the package directory, never absolute or traversing outside it.

Write package/skill.json, package/SKILL.md and package/references/review-checklist.md only. Body at most 6000 UTF-8 bytes; resource at most 6000 bytes; descriptor at most 4000 bytes. Make the body usable with the bounded checklist and describe when this package is appropriate and when an ordinary edit does not need it. The checklist should help apply the actual local convention without copying these examples as findings about a real project.

This creates an uninstalled proposal. Metadata declares tool dependencies; it grants no tools, activation, authority or successful validation. Do not claim native qualification or measured benefit. Source documents remain unchanged.
