# P7-02 built-in catalog foundation

This increment supplies 21 original versioned skill packages, executable-relative
default registration, embedded catalog identity, lazy body activation and exact
asset packaging. It does not complete P7-02 live usefulness or toolchain coverage.
Those gates remain explicitly unqualified in the shipped coverage matrix.

## Native contracts

Four new catalog tests and the ten existing extension tests passed. Three focused
retained-host tests passed on both Files and SQLite, including current read-policy
checks before corrupt metadata is read, metadata-only catalog verification,
subsequent invalidation, and the existing skill authority/reopen fences.

The CLI library passed 42 tests with one explicit external cloud-directory case
ignored. All fourteen actual executable workflows passed in 230.27 seconds.
New cases cover default discovery of 21 packages, relocation, missing versus
corrupt assets, workspace shadowing, explicit activation/disable, tampered bodies,
and preservation of 32 configured sources when no default assets are available.
Logs are `artifacts/p7-02-catalog-tests.log`,
`artifacts/p7-02-host-skills-tests.log` and
`artifacts/p7-02-cli-native-tests.log`.

## Frozen project experiment

All 42 project cases passed eight contract assertions each. Catalog integrity
read two metadata files totaling 41,848 bytes and 21 descriptors totaling 10,798
bytes. Normal discovery separately read the 21 descriptors (10,798 bytes), with
zero body/resource reads. Explicit activation loaded the selected body and
reported dependency revalidation reads separately. Project observation and
fixture-preservation reads are outside those catalog counters.

The run is
`artifacts/p7-builtin-skill-qualification/3ff6abd3-a06a-4cee-ad80-0fd7e1b919c2/manifest.json`.
It retained all outcomes and unchanged source identity before/after. Fixture
identity is `02e1bd9ae65a6c5ee6a0e922344d8c730b50762c069ab480e9662f20ca69b54e`;
catalog identity is `a51233aa6dffa90e04c903d43c10b893a7e188a77b26f84a9bfd91a41ed8e60d`.
No model or project toolchain was invoked. These results do not grade suggested
commands, seeded findings, generation quality or actual Git index preservation.

## Asset packaging

Four Node contract tests passed, including actual directory-link rejection and
preservation of an existing staging destination. Seven native archive cases
passed: the exact valid inventory was accepted, and traversal, duplicates,
missing/extra content, altered bytes and symlink metadata were rejected.

The helper staged the explicit native qualification executable plus notices and
44 catalog asset files, then verified all 48 ZIP entries and hashes. The package
record is
`artifacts/p7-builtin-packages/66709106-259c-41e9-98ac-9a16ef82a5a6/result.json`.
This is a fixture-enabled qualification executable and asset archive, not a
production installer or published release. Both extracted-archive native checks
passed: relocated catalog inspection and PTY activation/disable with shadowing
and tamper rejection. Each checked the executable digest and frozen catalog bytes
before exercising temporary copies. Evidence is
`artifacts/p7-02-cli-archive-tests.log`.

All nine repository delivery checks passed in
`artifacts/p7-02-fast-final/7751ed6d-868b-4aea-8b64-eb19e13a8cd3/manifest.json`.
Formatting checks passed for the changed Rust implementation (frozen input
projects retain their authored bytes). Clippy passed for all targets of
`vcp-extensions`, `vcp-lifecycle` and `vcp-cli`, with existing upstream warnings.
General CLI regression checks passed (69 passed, one explicit prerequisite case
ignored); the full log is
`artifacts/p7-02-cli-general-tests.log`.
The diff check passed with the deliberately conflicted Git input fixture excluded;
its conflict markers are frozen test data, not unresolved implementation changes.

## Remaining

Live U01–U03/U08 tasks, independently graded usefulness, actual ecosystem
toolchain execution and the full P8 distribution/install campaign remain not run.
Missing tools remain visible; packages never install them or contact services by
themselves. Only contract and packaging results are claimed here.
