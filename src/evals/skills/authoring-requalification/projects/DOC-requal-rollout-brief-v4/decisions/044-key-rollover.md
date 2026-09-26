# Decision 044: staged signing-key rollover

Status: accepted on 2026-08-22. Decision owner: Identity Council.

Ring A is the pilot. Rings B and C follow separately after their own approval.
During a ring's change, readers accept the old and new signing keys while writers
use only the new key. The old key is revoked only after the required observation
window and owner approval. A later proposal does not supersede this decision.

Tenant operators do not rotate service keys themselves. They need notice of the
validation window and any client errors, but the Identity operator performs the
rollover.
