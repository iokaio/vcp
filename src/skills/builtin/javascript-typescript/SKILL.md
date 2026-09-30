# JavaScript, TypeScript, and web projects

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Read the project

- `package.json`: `scripts`, `workspaces`, `packageManager`, `engines`, `type` (module format), `exports` and `main`.
- Lockfile, which identifies the manager: `package-lock.json` (npm), `pnpm-lock.yaml` (pnpm), `yarn.lock` (Yarn; `.yarnrc.yml` marks Berry), `bun.lock` or `bun.lockb` (Bun). Multiple lockfiles require repository guidance or an explicit ambiguity report; do not pick a manager arbitrarily.
- Workspace and monorepo configuration: `pnpm-workspace.yaml`, `turbo.json`, `nx.json`, `lerna.json`.
- `tsconfig*.json`: `references`, `paths`, `module` and `moduleResolution`, `target`, `strict`.
- `deno.json` or `deno.jsonc`: tasks, imports and permissions for Deno projects.
- Test, lint and format configuration: Jest, Vitest, Mocha or Playwright configs, ESLint or Biome, Prettier, and framework configs.
- `.nvmrc` or `.node-version` for the expected runtime.

Determine the package working directory and module format before editing imports.

## Discover commands in this order

1. Project-declared tasks: `package.json` scripts in the affected package, monorepo task runners, `deno.json` tasks, `Makefile` or `justfile`, CI workflow files, CONTRIBUTING and AGENTS.md. Inspect a script before running it; it can perform network or lifecycle effects and still needs current authority.
2. Ecosystem defaults, only when the scripts support them. Candidates to confirm: the selected manager's run form (`npm test`, `npm run <script>`, `pnpm --filter <pkg> test`, `yarn workspace <pkg> test`, `bun run <script>`, `deno task <name>`), narrowed with the runner's file or name filter; a type check through the project's own compiler such as a `tsc --noEmit` script.

Do not assume `npm test`, a global TypeScript compiler, `npx` downloads, or a framework CLI is appropriate. Avoid opportunistic lockfile churn.

## Toolchain variants

- npm, pnpm, Yarn classic, Yarn Berry (Plug'n'Play) and Bun resolve and hoist dependencies differently.
- Node, Deno and Bun differ in module resolution, permissions and built-in test runners.
- ESM and CommonJS: `type`, file extensions (`.mjs`, `.cjs`, `.mts`) and `exports` decide how imports resolve.
- Bundlers and transpilers (tsc, esbuild, swc, Babel, Vite) may type-check or strip types only; a passing build is not always a type check.

## Engineering rules

Review async error handling, input validation, browser and server boundaries, and public API behavior against nearby tests. Preserve existing rendering, module, lint and formatting conventions.

## Exact numeric contracts

When a contract requires exact integer results, bound intermediate arithmetic as well as inputs and outputs. Multiplying safe integers can exceed Number's exact range before a later division or range check. When Number intermediates cannot be proven exact, use native BigInt if the pinned runtime and TypeScript target support it, or the project's existing numeric representation appropriate to the contract. Preserve decimal semantics when required. Apply the specified rounding rule before converting a proven bounded result back to Number. Do not add a dependency when the runtime already supplies the required arithmetic.

Distinguish an omitted option from an explicitly invalid value; a nullish default must not silently accept null when the contract requires an integer. Check boundary, extreme and invalid inputs alongside the project's declared checks. Passing a small existing suite does not establish the full numeric contract; report the actual coverage and any remaining uncertainty.

## Verification evidence

Run the affected workspace's narrowest test first, then its type check, lint and build if the project uses them. Explain unsupported browser or service checks and missing dependencies without installing them. Report the selected package and script, the lockfile rationale, and actual results; an unrun script is not a pass.

## Pitfalls

- Watch-mode test scripts never exit; use the project's CI or run-once form.
- Snapshot updates can hide regressions; do not update snapshots to make a test pass.
- Windows: package binaries are `.cmd` shims, so spawning `npm` or `tsc` without a shell fails; resolve the shim or use the manager's run form. Scripts using `rm -rf`, `&&` chains or `VAR=x cmd` assume a POSIX shell. Deep `node_modules` paths can exceed legacy path limits, and running dev servers or editors lock files. Import paths are case-insensitive on disk but case-sensitive in CI. CRLF checkouts can break lint rules and snapshots.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
