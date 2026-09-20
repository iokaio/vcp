# .NET and PowerShell on Windows

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Identify solution and shell

Read global.json, the declared .sln/.slnx or project, target frameworks, Directory.Build/Packages configuration, and test projects. For PowerShell, read script requirements, parameter validation, module dependencies, and any Pester configuration. A global.json cue does not prove that every project supports the installed SDK; PowerShell-only projects can be selected explicitly.

Use the intended SDK/MSBuild and shell edition/version. Select the documented solution or project and configuration for dotnet build/test, including no-restore only when dependencies are already available. For Pester, select the repository's declared invocation and scope. Restore/module installation requires separate authority.

Treat paths as data. Use literal-path filesystem operations and correct argument passing; do not construct command text from repository strings. Review native exit codes separately from PowerShell exceptions, pipeline behavior, and terminating versus nonterminating errors.

## Report

Preserve target frameworks and solution structure. Record the exact SDK/shell, project, configuration, and observed checks. Missing SDKs, modules, or Windows components mean those checks were not run.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
