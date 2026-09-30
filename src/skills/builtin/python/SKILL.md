# Python projects and environments

Original VCP guidance.

## Read the project

Inspect these before choosing an interpreter or command:

- `pyproject.toml`: `[build-system]` backend (setuptools, hatchling, poetry-core, pdm-backend, flit, maturin), `[project]` `requires-python` and optional dependencies, and `[tool.*]` tables for pytest, ruff, black, isort, mypy, pyright, coverage, uv, poetry, hatch or pdm.
- Lock and requirement files: `uv.lock`, `poetry.lock`, `pdm.lock`, `Pipfile` and `Pipfile.lock`, `requirements*.txt` and `*.in` (pip-tools), `environment.yml` (conda). The lockfile identifies the environment manager.
- Legacy packaging: `setup.py` and `setup.cfg`.
- Runners and configuration: `tox.ini`, `noxfile.py`, `pytest.ini`, `conftest.py`, `.pre-commit-config.yaml`, `mypy.ini`.
- `.python-version` and `requires-python`: the supported interpreter range and syntax.
- Package layout: `src/` layout versus flat, namespace packages, and compiled extensions.

A `.venv` directory is evidence to inspect, not proof that its interpreter is valid on this host.

## Discover commands in this order

1. Project-declared tasks: `Makefile`, `justfile`, `Taskfile.yml`, tox or nox sessions, hatch or pdm scripts, pre-commit hooks, CI workflow files, CONTRIBUTING and AGENTS.md.
2. Ecosystem defaults, only when the configuration supports them. Candidates to confirm: `python -m pytest <path>::<test>` or `-k <expr>` with the project interpreter; the existing environment's interpreter directly (`.venv/bin/python -m pytest`, or `.venv\Scripts\python.exe -m pytest` on Windows); the manager's non-syncing run form when that manager owns the environment, such as `uv run --frozen --no-sync pytest` or `uv run --offline pytest`; `ruff check`, `ruff format --check` or `black --check`; `mypy` or `pyright` when configured.

Use the existing configured interpreter and environment. Prefer module invocation (`python -m ...`) when that is the repository convention; do not assume bare `pip` or `pytest` resolves to the intended environment.

## Toolchain variants

- uv, Poetry, PDM, Hatch, pip-tools, Pipenv and plain pip with virtual environments are not interchangeable. Each has its own lock format and run form.
- Conda or mamba environments are selected by name or prefix, not by `.venv`.
- tox and nox create their own environments per session and may download packages.
- Compiled extensions (Cython, maturin, C extensions) need a rebuild before tests reflect source changes.
- The Windows `py` launcher selects interpreters differently from `python` on PATH.

A sync or install step changes the environment and may download packages, so do not run it as an automatic prerequisite. Run forms hide such steps: plain `uv run` may lock, sync and download before running the command, and `hatch run`, `pdm run` and `poetry run` may create or sync an environment on first use. Default to the frozen, non-syncing or offline form, or to the existing interpreter, and treat any lock, sync or environment creation as a separate effect needing authority.

## Engineering rules

Preserve import and typing conventions, resource cleanup, exception boundaries, and supported interpreter syntax. Narrow regression selection to the changed module while including affected integration contracts.

## Verification evidence

Run the narrowest affected test first, then the module's suite, then broader suites when shared code changed. Run the configured linter, formatter check and type checker when the project uses them. Record interpreter and environment identity, working directory, and the exact selected checks with their results. If Python or dependencies are absent, provide analysis and a specific not-run explanation. Never report generated code as tested merely because it is syntactically plausible.

## Pitfalls

- An editable install can shadow the working tree or be stale; check which module path was imported.
- Tests that pass alone can fail in the full suite through shared fixtures, import order or global state.
- `conftest.py` and pytest plugins change collection; a zero-test run is not a pass.
- Windows: virtual environments use `Scripts\python.exe`, not `bin/python`. Activation scripts may be blocked by PowerShell execution policy; calling the environment's interpreter directly avoids activation. The Microsoft Store `python` alias can shadow a real interpreter. Default text encoding may not be UTF-8, so open files with an explicit encoding. Build paths and CRLF line endings differ, and deep `site-packages` paths can exceed legacy path limits. Open files and running processes lock files, so deletes and rewrites can fail.
- `subprocess` with `shell=True` behaves differently under `cmd.exe`; pass argument lists.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
