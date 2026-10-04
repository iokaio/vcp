# P2-06 .NET verification

The October 3, 2026 increment adds qualified .NET project/solution verification
and required native checks to Scenario B. Its P2-04 prerequisite accepts three
explicit absolute public SDK directory paths, with no ambient inheritance.

The native fixture uses .NET SDK 10.0.204, projects targeting `net10.0`, the normal
VSTest console logger, and cached pinned Microsoft.NET.Test.Sdk 17.11.1, xunit
2.9.2 and xunit.runner.visualstudio 2.8.2 packages. Restore clears package feeds
and supplies the cache as an explicit argument. Restore and verification both
use the production public environment and disable build servers. Build output
exclusions are configured before the source baseline, matching Scenario B's
template rather than adding global directory exclusions.

On both SQLite and Files, a named real xUnit test observes an application function
returning 41 and fails its assertion for 42. Completion is refused. After the
application source is repaired, a fresh check passes with no outstanding issues
and completion succeeds. An independent filesystem marker proves test execution.
The earlier failed canonical record and its output artifact remain byte-identical.
No provider request is made.

## Verification

- `scripts/test-dotnet-verification.ps1`: passed the selected native regression
  across both stores, with source hashes unchanged throughout qualification.
  Manifest: `artifacts/dotnet-verification-native/a478bb9f-137c-4fd5-84f6-ff963339c12f/manifest.json`.
- `scripts/test-tools.ps1`: passed all 128 current contracts and 65 retained
  patch tests, with the exact selected-source inventory and stable input hashes.
  Manifest: `artifacts/dotnet-tools-final/01b812fe-d841-4ae8-af62-647915392c20/manifest.json`
  in the isolated delivery checkout.
- All 25 fast cases passed across the full run and the focused authoring rerun
  after source edits stopped. The first authoring result retained its rejection
  of a checker receipt whose source inputs changed during execution.
- All offline scenario regressions passed. Final Inventory profile checks
  include cumulative names, fresh/reused `.sln`/`.slnx` projects, bounded
  deadlines, repair/resume wiring and protected-file preservation.
- Native protocol provenance, vendor hashes, the authored lock patch, formatting
  and diff whitespace checks passed.

## Scope

The parser qualifies bounded English VSTest output, distinct fully qualified
test names, and observed C# projects in the supported solution forms. Unsupported
formats, missing projects, zero tests, skipped tests and incomplete summaries
cannot prove completion. Test output does not independently establish semantic
coverage; trusted acceptance names and the separate inventory gates remain
necessary. The fixture does not qualify a live provider, SQL Server migrations,
an installed CLI, or complete paid Scenario A/B runs. No installer was built.
