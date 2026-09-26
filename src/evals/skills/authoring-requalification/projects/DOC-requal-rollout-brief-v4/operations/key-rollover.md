# Signing-key rollover procedure

Identity operator: Nia. Resume or revocation approver: Identity lead Omar.

For one ring at a time: (1) run the read-only client compatibility check; (2)
enable dual-key reads; (3) switch writers to the new key; (4) observe two full
15-minute intervals; and (5) ask Omar to approve old-key revocation. Do not begin
the next ring until the current ring is closed.

Stop and restore writers to the old key if signature failures exceed 0.50% in
either interval or if any `KEY_UNKNOWN` event appears. Keep dual-key reads enabled
while stopped. Omar owns the resume decision after the cause is reviewed. This
writing task authorizes none of these operational actions or contacts.
