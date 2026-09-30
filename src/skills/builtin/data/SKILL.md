# Data files and transformation pipelines

Original VCP guidance.

## Inspect first

- Declared schemas and contracts: JSON Schema, Avro/Protobuf/Parquet metadata, dataframe or validation models, dbt models and tests, and column documentation.
- Lineage and provenance: where each input comes from, which step produces each output, and whether outputs are checked in, generated, or fetched.
- Notebooks versus pipelines: exploratory notebooks may hold stale outputs and hidden execution order; the pipeline, job or package code is usually the contract.
- The engine and orchestration used (for example pandas, Polars, Spark, DuckDB, dbt, Airflow or plain scripts) and its configured destinations.
- Sample sizes and fixtures available locally, and whether any are representative.
- Sensitive fields: personal data, identifiers, credentials, free text, and any documented handling or retention rules.

## Establish data semantics

Read the declared schema, format, encoding, units, provenance, transformation code and tests. Identify keys, null/missing values, ordering, time zones and expected row/count invariants. This skill is always listed by its description rather than detected from root files.

Use bounded authorized samples or synthetic fixtures. Do not load an entire large dataset just to infer a schema, upload rows to external services, or assume de-identification from column names. Separate schema checks from statistical claims that need representative data.

For generation, preserve source data and make transformations reproducible under the declared engine/version. Review joins, duplicate handling, type coercion, rounding and loss of provenance. Write outputs only to authorized locations; a pipeline configuration can contain effectful commands or remote destinations.

## Proceed and verify

1. Find the project's test, lint and validation commands from instructions, task-runner targets and CI. Candidates to confirm include the unit test runner, schema or data-quality checks, dbt checks, and notebook execution checks the project already uses. `dbt parse` works offline, but `dbt compile`, `test`, `run` and `build` connect to the warehouse through `profiles.yml` credentials, `run` and `build` write to it, and `dbt deps` downloads packages.
2. Start with a small synthetic or authorized sample, stated by size and source. Prefer parse, dry-run, plain `EXPLAIN` or limit-bounded modes before full runs; `EXPLAIN ANALYZE` executes the query.
3. Check invariants explicitly: row counts in and out, key uniqueness, null rates, value ranges, type and unit consistency, and join fan-out.
4. Write outputs to a task-owned temporary location unless a destination is authorized. Never overwrite source data, push to shared storage, or trigger scheduled jobs without explicit authority.
5. Never print, log or report sensitive rows or secrets; summarize with counts, schemas or masked examples.

Evidence is the input identity (path, version or hash), the command, the sample size, and the invariants actually observed. A notebook's saved output is not evidence of a current run.

## Pitfalls

- Many-to-many joins inflating rows, silent drops from inner joins, and duplicates from retries or overlapping partitions.
- Type inference from a head sample, float rounding for money, integer overflow, and date parsing that depends on locale.
- Time zones and daylight-saving transitions, naive versus aware timestamps, and ordering assumptions in unordered sources.
- Encoding and delimiters: BOMs, mixed encodings, embedded newlines and quoted separators in CSV.
- Non-deterministic outputs from unseeded sampling, hash ordering or parallel writes.
- Windows: CRLF changing hashes and CSV parsing, case-insensitive paths colliding, long paths in partitioned layouts, files locked by an open spreadsheet or notebook kernel, and default text encodings differing from UTF-8.

## Evidence

Report schema and input identities, invariants checked, counts/ranges actually observed, preserved source files, and limits of sampling. Missing engines or restricted data access produce explicit not-run outcomes rather than fabricated metrics.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
