# Shell scripts and command boundaries

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Establish the interpreter

Inspect the script's declared interpreter, repository instructions, shell options, call sites, and test/lint configuration. Determine Bash, POSIX shell, PowerShell, or another actual contract; these languages and their error rules are not interchangeable. Current automatic root cues do not identify shell-only projects, so select this skill explicitly.

Trace each input into arguments, expansions, redirections and filesystem operations. Preserve spaces, Unicode and metacharacters as data. Check pipeline failure propagation, cleanup traps/finally blocks, temporary-path ownership and the distinction between an exit code and a partial side effect.

Use a declared non-effectful syntax/lint check first when available and authorized, then a bounded fixture with controlled paths. Never test destructive commands against user data. Do not silently run a Bash script under a different shell or download a missing interpreter.

## Report

Explain the interpreter-specific defect or change, the exact synthetic fixture used, observed exit/effect results, and checks not run on this host.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
