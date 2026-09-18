# Public synthetic Windows handoff

This fixture contains encrypted synthetic records and one synthetic artifact,
produced by the P0 storage executable on native Windows. `fixture.json` records
the producer binary hash and every ciphertext byte hash. It contains no user
workspace, credential, recovery secret or private history.

The decrypting identity is the **public age 0.11.2 unit-test vector**; the
qualification executable supplies it separately through `handoff-recovery`.
It is deliberately compromised and must never be used for actual backup data.
The synthetic Ed25519 writer also uses a public fixed seed in the fixture code.

Reproduction and the cross-machine check are described in
[the storage guide](../../../../docs/development/portable-storage-spike.md).
Random identities used in other tests stay local and are not part of this fixture.
