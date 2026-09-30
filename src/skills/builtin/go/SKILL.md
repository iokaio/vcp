# Go modules and workspaces

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Read the project

- `go.mod`: module path, `go` directive (minimum language version), `toolchain` directive, `require`, `replace` and `exclude`. Nested `go.mod` files mark separate modules that root commands do not cover.
- `go.sum`: must stay consistent; preserve sums for source-only changes.
- `go.work` and `go.work.sum`: workspace membership and `use` or `replace` entries that change dependency resolution. Check whether `go.work` is committed or local only.
- Build constraints: `//go:build` lines and `_windows.go`, `_linux.go` or `_test.go` suffixes decide which files compile.
- Generated code: files carrying a `Code generated ... DO NOT EDIT.` notice and the `//go:generate` directives that produce them. Edit the source of generation, not the output.
- `vendor/` with `modules.txt`: vendored builds.
- Lint configuration such as `.golangci.yml` or `staticcheck.conf`, and nearby tests and `testdata/`.

Identify the affected module and package. A nested-only module may need explicit skill selection.

## Discover commands in this order

1. Project-declared tasks: `Makefile`, `magefile.go`, `justfile`, `Taskfile.yml`, scripts, CI workflow files, CONTRIBUTING and AGENTS.md. These record required build tags, environment variables and lint settings.
2. Ecosystem defaults, only when the module supports them. Candidates to confirm: `go test ./<pkg>/...` or `go test ./<pkg> -run <TestName>` from the module root, widened to `go test ./...` when shared code changed; `go vet ./<pkg>/...`; `gofmt -l <paths>` to list unformatted files; `go build ./...`; `golangci-lint run` or `staticcheck` when configured; `go test -race` where the host supports it.

Pass the same `-tags` the project uses so tagged files are compiled.

## Toolchain variants

- Single module, multi-module repositories, and `go.work` workspaces; `GOWORK=off` shows behavior without the workspace.
- `GOFLAGS=-mod=vendor` or `-mod=mod`, `GOPROXY`, `GOPRIVATE` and `GONOSUMDB` settings change resolution.
- The `toolchain` directive and `GOTOOLCHAIN` can trigger an automatic toolchain download.
- cgo: `CGO_ENABLED` and a C compiler decide whether cgo packages build; the race detector needs cgo on many platforms.
- Cross compilation with `GOOS` and `GOARCH` compiles other-platform files without running them.

## Engineering rules

Preserve context cancellation, error wrapping, ownership of goroutines and channels, and interfaces already used by callers. For concurrency fixes, establish the actual race or lifecycle failure. Do not add goroutines or broad retry loops solely to hide blocking behavior.

## Verification evidence

Run the narrowest package test first, then dependents and the module. Test caching can report `(cached)`; that result reflects an earlier run of unchanged inputs, and `-count=1` forces a fresh run when freshness matters. Run formatting and static checks the project uses. Record Go version, module or workspace, build tags, environment overrides and commands. Race tests require a supported host and compiler and may not represent every deployment target. If dependencies or native prerequisites are absent, distinguish unrun compilation, race checks and integration services.

## Pitfalls

- Module downloads, toolchain auto-downloads, `go get`, `go mod tidy`, `go generate` and generated-code changes are effects requiring explicit task relevance and authority; they can rewrite `go.mod` and `go.sum`.
- `./...` from the repository root skips nested modules.
- Packages with no test files report `no test files`; that is not coverage.
- Windows: a running test binary locks its `.exe`; cgo needs a GCC-compatible toolchain that is often absent. Files with `_windows.go` suffixes or `windows` build tags compile only there, so Unix-only verification misses them. Use `filepath` rather than `path` for OS paths, and expect `\` separators, drive letters and case-insensitive names. Batch files (`.cmd`, `.bat`) run through `cmd.exe`, whose quoting rules differ; pass arguments as a list and keep untrusted text out of batch arguments. CRLF checkouts break `gofmt` checks and golden files, and deep module cache paths can exceed legacy path limits.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
