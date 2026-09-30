# Shell scripts and command boundaries

Original VCP guidance.

## Inspect first

- The shebang or invocation site: `#!/bin/sh`, Bash, zsh, `pwsh`/`powershell.exe`, or a script sourced by another script. The caller decides the real interpreter.
- Shell options: `set -e`, `-u`, `-o pipefail`, `IFS` changes, `$ErrorActionPreference`, `Set-StrictMode`, and any `trap`/`try`/`finally` cleanup.
- Lint and format configuration such as `.shellcheckrc`, inline ShellCheck directives, `.editorconfig`, PSScriptAnalyzer settings, and CI steps that run them.
- `.gitattributes` line-ending rules, executable bits, and how CI or packaging invokes the script.
- Repository instructions, Makefile/task-runner targets and existing script tests (for example Bats, Pester or shunit2 fixtures) that show the project's verification commands.

## Establish the interpreter

Inspect the script's declared interpreter, repository instructions, shell options, call sites, and test/lint configuration. Determine Bash, POSIX shell, PowerShell, or another actual contract; these languages and their error rules are not interchangeable. This skill is always listed by its description rather than detected from root files.

Trace each input into arguments, expansions, redirections and filesystem operations. Preserve spaces, Unicode and metacharacters as data. Check pipeline failure propagation, cleanup traps/finally blocks, temporary-path ownership and the distinction between an exit code and a partial side effect.

## Proceed and verify

1. Find the project's lint/test commands in instructions, task-runner targets and CI before choosing one. Candidates to confirm include `bash -n`, `sh -n`, `shellcheck`, a PowerShell parser or PSScriptAnalyzer check, and the declared script test runner.
2. Run a declared non-effectful syntax/lint check first when available and authorized.
3. Exercise behavior with a bounded fixture: a temporary directory the task owns, synthetic inputs including spaces, quotes, globs, leading dashes and empty values, and stubbed commands for anything effectful.
4. Where the script supports a dry-run, `--help`, `echo`-only or `-WhatIf` mode, prefer it before any real effect. Never run deletion, network, package-install, service or privileged commands against user data or remote systems without explicit authority.
5. Never test destructive commands against user data. Do not silently run a Bash script under a different shell or download a missing interpreter.
6. Keep secrets out of arguments, traces and logs; `set -x` and verbose output can print tokens and environment values.

Evidence is the exact interpreter and version reported by the host, the command, working directory, exit status, and the observed files or output of the fixture. A clean lint run does not prove runtime behavior, and a passing run under one shell does not cover another.

## Pitfalls

- Unquoted expansions, `$*` versus `"$@"`, word splitting on filenames, and globbing in `rm`/`mv` targets.
- `set -e` is suppressed inside conditions, `&&`/`||` lists and some subshells; `pipefail` is not POSIX. `local` and arrays are not POSIX either.
- `cd` without a failure check, relative paths that depend on the caller's directory, and `mktemp` output left behind on error.
- CRLF line endings break shebangs and produce `$'\r'` errors; check `.gitattributes` and `core.autocrlf` before blaming the script.
- PowerShell quoting differs from POSIX: single quotes are literal, the backtick escapes, and native-command argument passing varies by host. Execution policy can block scripts; do not change it without authorization.
- Windows paths: backslashes, drive letters, case-insensitive names, reserved device names, long paths, and files locked by another process. Git Bash, WSL and native PowerShell see different paths and environments.
- Exit codes from PowerShell cmdlets versus native executables (`$LASTEXITCODE`), and non-zero statuses swallowed by pipelines.

## Report

Explain the interpreter-specific defect or change, the exact synthetic fixture used, observed exit/effect results, and checks not run on this host.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
