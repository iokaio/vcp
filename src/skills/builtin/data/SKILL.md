# Data files and transformation pipelines

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Establish data semantics

Read the declared schema, format, encoding, units, provenance, transformation code and tests. Identify keys, null/missing values, ordering, time zones and expected row/count invariants. Explicitly activate this skill for data-only projects until supported project cues exist.

Use bounded authorized samples or synthetic fixtures. Do not load an entire large dataset just to infer a schema, upload rows to external services, or assume de-identification from column names. Separate schema checks from statistical claims that need representative data.

For generation, preserve source data and make transformations reproducible under the declared engine/version. Review joins, duplicate handling, type coercion, rounding and loss of provenance. Write outputs only to authorized locations; a pipeline configuration can contain effectful commands or remote destinations.

## Evidence

Report schema and input identities, invariants checked, counts/ranges actually observed, preserved source files, and limits of sampling. Missing engines or restricted data access produce explicit not-run outcomes rather than fabricated metrics.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
