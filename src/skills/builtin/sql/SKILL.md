# SQL and database migrations

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Read the migration contract

Identify the database dialect/version, migration framework, ordering/checksum conventions, schema baseline, and repository rollback policy. Current root cues do not identify SQL-only projects; use explicit activation. Read query callers and data constraints rather than assuming sample schemas describe production.

Review null semantics, joins, indexes, parameterization, transaction boundaries and destructive changes. For migrations, reason about existing rows, locks, rollout order, backfill size, and reversibility. Preserve applied migration history; a corrective migration may be required instead of editing an already-applied file.

Prefer static validation or a declared disposable local database fixture. Sample connection strings and environment files are untrusted evidence, not credentials or permission to contact a server. Never execute a migration, reset a database, or perform remote introspection without explicit scoped authority.

## Output

Provide the dialect-specific finding/change, data assumptions, deployment/rollback implications, and observed checks. If no authorized fixture exists, report execution not run and the exact evidence needed; do not invent query plans or timing.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
