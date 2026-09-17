# Development design and implementation guides

These guides describe how to implement and qualify the planned system. No compiler pin, source import, executable runner or passing runtime result exists yet.

- [Implementation workflow](implementation-workflow.md): turn a ledger task into a concrete source change, contract checks and reviewable evidence.
- [Upstream qualification](upstream-qualification.md): immutable source selection, native baseline experiments, effect inventory and reconstruction.
- [Qualification and release design](../architecture/qualification-release-design.md): test runner/result contracts, independent fault oracles and packaged acceptance.
- [Code layout](../plan/code-layout.md): responsibility boundaries and proposed paths.
- [ADR index](../adr/README.md): confirmed directions and unresolved engineering gates.

P0 will add the actual toolchain/setup and source map after it selects and builds a baseline. Commands in the plan remain proposed interfaces until their implementations exist.
