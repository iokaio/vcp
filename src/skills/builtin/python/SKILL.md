# Python projects and environments

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Identify the environment

Read pyproject.toml and any requirements, lock, tox, nox, pytest, lint, or type-check configuration relevant to the task. Identify the declared Python version, package layout, and environment manager. A .venv directory is evidence to inspect, not proof that its interpreter is valid on this host.

Use the existing configured interpreter and environment. Prefer its module invocation when that is the repository convention; do not assume bare pip or pytest resolves to the intended environment. uv, Poetry, pip, and Conda workflows are not interchangeable. A sync/install step changes the environment and may download packages, so do not run it as an automatic prerequisite.

For changes, preserve import and typing conventions, resource cleanup, exception boundaries, and supported interpreter syntax. Narrow regression selection to the changed module while including affected integration contracts.

## Results

Record interpreter/environment identity, working directory, and exact selected checks. If Python or dependencies are absent, provide analysis and a specific not-run explanation. Never report generated code as tested merely because it is syntactically plausible.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
