# Architecture and module organization

Original VCP guidance, version 1.1.0. This package supplies instructions, not a tool executor or authority.

## Establish the existing design

Read the task contract, scoped instructions, directly relevant ADRs, module entry points, and nearby tests. Draw a small dependency map from actual imports or project references. Identify which layer owns I/O, orchestration, domain rules, configuration, and errors; do not assume the repository follows a named architecture.

Search within relevant paths before reading large files. Use literal search by default, explicit regex when useful, then read the relevant line ranges and nearby contract. Retain source-version evidence, follow an incomplete result when needed, and reread changed source before making a finding. A bounded scan or range does not establish that the whole repository was examined.

For analysis, cite the files that establish each boundary and distinguish documented intent from observed coupling. For review, trace one concrete call path across the proposed change. Demonstrate a dependency-direction violation or incompatible error contract before calling it a defect.

For generation, implement the smallest change at the existing owning boundary. Reuse established configuration and error conventions. Introduce an interface only when a present caller needs it; avoid reorganizing unrelated folders. Preserve public compatibility unless the task authorizes a change.

## Evidence to return

Return the affected boundary, source references, one concrete before/after flow, the trade-off of any new dependency, and relevant checks actually observed. A diagram can explain the dependency map but cannot substitute for source inspection.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
