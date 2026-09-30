# Targeted testing and verification

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Inspect first

- Existing test layout: unit tests beside code or in separate trees, integration and end-to-end directories, naming conventions.
- Runners and their configuration, found in manifests, task-runner targets, CI workflows and contributor docs, including filters for running one test or file.
- Fixtures, snapshots, golden files, test data builders and required services such as databases or emulators.
- Known flaky or quarantined tests, retries configured in CI, and platform-specific skips.
- Which tests, if any, already cover the boundary you are changing.

## Discover the check contract

Read project manifests, test configuration, CI commands, and neighboring tests. Identify the runner, working directory, version/features, required services, and inputs. Separate unit, integration, end-to-end, build, lint, and type checks: one passing category does not establish another.

Choose the smallest check that exercises the changed boundary. A regression should fail for the defect's observable consequence, not mirror implementation details. Reproduce a valid failure before changing its expectation. If a test conflicts with the task contract, resolve the evidence; do not skip it to obtain green output.

## Proceed and verify

1. Run the narrowest existing test for the boundary first, filtered by the runner's own selection syntax as the project uses it.
2. Add or adjust a test next to similar tests, reusing existing fixtures and helpers. Confirm a new regression fails without the fix when practical.
3. Broaden testing when shared contracts change, a focused failure exposes wider risk, or acceptance requires it. Do not repeatedly run an unchanged expensive suite without a reason. Treat skipped tests, timeouts, infrastructure failures, and missing services as distinct outcomes.
4. For a long foreground check, use its explicitly configured duration within the trusted process profile and remaining task deadline. A manifest or skill cannot raise those limits. Report the limiting configuration if the check cannot run; do not split or relaunch it merely to evade a ceiling. A timeout, output-limit stop or cancellation is not a passing check.
5. Run the project's format, lint and type checks that cover the changed files before reporting completion.

## Preserve verification identity

Record the exact command, working directory, exit status, result counts, and relevant source revision. Edits after a passing run invalidate checks that depend on those edits. Report not-run checks explicitly; a zero-test run is not coverage of the intended behavior.

Keep raw process evidence distinct from the decoded output preview. Report declared encoding, omitted bytes or decoding loss when relevant; readable output cannot change the exit status or pass rule. After removing temporary instrumentation, rerun checks that depended on the edited files.

## Pitfalls

- A filter that matches nothing and exits successfully; check the reported count.
- Updating snapshots or golden files to match a defect instead of fixing it.
- Tests depending on order, wall-clock time, randomness without a seed, network access or shared global state.
- Retrying a flaky test until it passes and reporting that as green; report the flake and the observed failures.
- Mocking the unit under test so the assertion proves nothing about the real boundary.
- Windows: path separators and drive-letter case in assertions, CRLF in fixtures and snapshots under `core.autocrlf`, files or ports still held by a previous run, long temporary paths, and per-test timeouts too short for slower process startup.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
