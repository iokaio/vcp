# Architecture and module organization

Original VCP guidance.

## Inspect first

- Module boundaries: packages, crates, projects, namespaces or directories, and what each exports.
- Dependency direction: manifest and project references, import graphs, and any layering or dependency rules enforced by lint or build configuration.
- Decision records: ADRs, design documents, the task contract, and scoped agent instructions that constrain the area.
- Public interfaces: exported APIs, wire and file formats, CLI flags, configuration keys, and error types that callers depend on.
- Where I/O, orchestration, domain rules, configuration and error mapping currently live.
- Nearby tests that pin the boundary's behavior.

## Establish the existing design

Read the task contract, scoped instructions, directly relevant ADRs, module entry points, and nearby tests. Draw a small dependency map from actual imports or project references. Identify which layer owns I/O, orchestration, domain rules, configuration, and errors; do not assume the repository follows a named architecture.

Search within relevant paths before reading large files. Use literal search by default, explicit regex when useful, then read the relevant line ranges and nearby contract. Retain source-version evidence, follow an incomplete result when needed, and reread changed source before making a finding. A bounded scan or range does not establish that the whole repository was examined.

## Proceed and verify

1. State the owning boundary for the change and cite the files that establish it.
2. For analysis, cite the files that establish each boundary and distinguish documented intent from observed coupling. For review, trace one concrete call path across the proposed change. Demonstrate a dependency-direction violation or incompatible error contract before calling it a defect.
3. For generation, implement the smallest change at the existing owning boundary. Reuse established configuration and error conventions. Introduce an interface only when a present caller needs it; avoid reorganizing unrelated folders. Preserve public compatibility unless the task authorizes a change.
4. If the change conflicts with a recorded decision, choose the smallest compatible resolution and record the deviation near the code or in a new decision record; do not rewrite historical ADRs.
5. Verify with the project's own build and test commands, found in instructions, manifests and CI. A build that compiles every affected module, plus tests at the changed boundary, is stronger evidence than a diagram. Any configured dependency or layering check is part of that evidence.

Evidence is the before/after dependency edges, the cited sources, and the checks actually run. Reasoning about a design is analysis; a claim that callers still work needs a build or test that exercises them, or it is reported as not run.

## Pitfalls

- Moving code across boundaries or renaming public items to tidy structure outside the task.
- Adding a shared "utils" or core module that inverts dependency direction.
- Speculative interfaces, plugin points or configuration options with no current caller.
- Leaking provider-, platform- or framework-specific types through a neutral interface.
- Treating a directory name as a layer contract without checking imports.
- Windows: path case and separators in module paths or include rules, platform-specific modules only compiled on one OS, and generated files that differ by line ending.

## Evidence to return

Return the affected boundary, source references, one concrete before/after flow, the trade-off of any new dependency, and relevant checks actually observed. A diagram can explain the dependency map but cannot substitute for source inspection.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
