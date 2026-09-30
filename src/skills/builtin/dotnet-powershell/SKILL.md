# .NET and PowerShell on Windows

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Read the project

- `*.sln` or `*.slnx`: which projects belong to the build and their configurations.
- `*.csproj`, `*.fsproj`, `*.vbproj`: `TargetFramework(s)`, `Sdk` attribute (SDK-style) or legacy format, `IsPackable`, test framework references (xUnit, NUnit, MSTest), `InternalsVisibleTo`.
- `Directory.Build.props` and `Directory.Build.targets`: shared properties such as `TreatWarningsAsErrors`, `Nullable`, analyzers and `LangVersion`.
- `Directory.Packages.props`: central package version management.
- `global.json`: SDK pin and roll-forward policy. It does not prove that every project supports the installed SDK.
- `NuGet.config`: package sources; private feeds may need credentials you do not have.
- `.editorconfig`: formatting and analyzer severity.
- PowerShell: module manifests (`*.psd1`), `#Requires` statements, parameter validation, module dependencies, `PSScriptAnalyzerSettings.psd1`, and Pester tests (`*.Tests.ps1`) and configuration.

PowerShell-only projects can be selected explicitly.

## Discover commands in this order

1. Project-declared tasks: build scripts (`build.ps1`, `build.cmd`, Cake, Nuke, Invoke-Build or psake), `Makefile`, CI workflow files, CONTRIBUTING and AGENTS.md.
2. Ecosystem defaults, only when the project supports them. Candidates to confirm: `dotnet build <project-or-solution> -c <config>`; `dotnet test <test-project> --filter <expression>`; `--no-restore` only when dependencies are already restored; `dotnet format --verify-no-changes` when the project uses it; `Invoke-Pester -Path <tests>` with the repository's declared configuration; `Invoke-ScriptAnalyzer` when configured.

Restore and module installation require separate authority.

## Toolchain variants

- SDK-style projects build with `dotnet`; legacy (non-SDK) projects and some .NET Framework targets need MSBuild from Visual Studio and may not build on other hosts.
- Multi-targeted projects build and test once per framework; narrow with `-f <tfm>` when appropriate.
- Windows PowerShell 5.1 (`powershell.exe`, .NET Framework) and PowerShell 7 (`pwsh`, .NET) differ in cmdlets, default encodings, operators and module compatibility. Honor `#Requires -Version` and `PSEdition`.
- Pester major versions differ in configuration and assertion syntax; follow the version the tests are written for.

## Engineering rules

Preserve target frameworks and solution structure. Treat paths as data: use `-LiteralPath` for filesystem operations and correct argument passing; do not construct command text from repository strings. Review native exit codes (`$LASTEXITCODE`) separately from PowerShell exceptions, pipeline behavior, and terminating versus nonterminating errors.

## Verification evidence

Run the narrowest affected test project or filter first, then the solution when shared code changed. Include analyzer warnings the project treats as errors. Record the exact SDK or shell edition, project, configuration, target framework and observed results. Missing SDKs, workloads, modules or Windows components mean those checks were not run.

## Pitfalls

- A test run that discovers zero tests is not a pass; check the adapter and filter.
- Running test hosts, `dotnet build-server` processes, IDEs and antivirus lock `bin` and `obj` outputs; stop the process rather than deleting outputs.
- Stale `obj` state after switching configuration or framework can cause misleading errors.
- Execution policy can block scripts; do not change it without authority.
- Quoting: single quotes are literal, double quotes expand `$` and backtick escapes; native command argument passing differs between editions. `--%` and splatting change parsing.
- `Write-Host` output is not pipeline output; functions return everything they emit.
- Case-insensitive paths, `\` separators, CRLF line endings, and legacy path length limits. Windows PowerShell 5.1 reads BOM-less scripts in the ANSI code page, so non-ASCII scripts may need a UTF-8 BOM.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
