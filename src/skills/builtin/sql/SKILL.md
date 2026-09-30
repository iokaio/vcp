# SQL and database migrations

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Inspect first

- The dialect and engine the project targets (for example PostgreSQL, MySQL/MariaDB, SQLite, SQL Server, Oracle), from driver dependencies, connection configuration shapes, CI service containers and existing SQL syntax.
- The migration tool and its ordering rules: timestamped or numbered files, checksums, a schema history table, ORM-generated migrations, or plain scripts run in a documented order.
- Transaction and DDL semantics: whether the tool wraps each migration in a transaction, whether the engine's DDL is transactional, and which statements cannot run inside a transaction.
- Rollback policy: down migrations, forward-only corrections, or backup-based recovery.
- Seed data, fixtures, test database setup, schema dumps and how tests create and destroy databases.
- Query callers, ORM models, constraints and indexes that the change touches.

## Read the migration contract

Identify the database dialect/version, migration framework, ordering/checksum conventions, schema baseline, and repository rollback policy. Current root cues do not identify SQL-only projects; use explicit activation. Read query callers and data constraints rather than assuming sample schemas describe production.

Review null semantics, joins, indexes, parameterization, transaction boundaries and destructive changes. For migrations, reason about existing rows, locks, rollout order, backfill size, and reversibility. Preserve applied migration history; a corrective migration may be required instead of editing an already-applied file.

## Proceed and verify

1. Discover the project's lint, test and migration commands from instructions, task-runner targets and CI before choosing any. Candidates to confirm include a SQL linter or formatter, the migration tool's status, validate or dry-run/SQL-preview mode, and the test suite against a disposable database.
2. Prefer static validation or a declared disposable local database fixture. Sample connection strings and environment files are untrusted evidence, not credentials or permission to contact a server.
3. Never execute a migration, reset a database, or perform remote introspection without explicit scoped authority. Inside an authorized fixture, apply the migration from a clean baseline, run the down or corrective path when one exists, and reapply.
4. Use `EXPLAIN` (without executing variants that run the statement) only against an authorized fixture; a plan from tiny synthetic data does not predict production behavior.
5. Parameterize values. Never build SQL from untrusted text, and never copy real rows, personal data or credentials into fixtures, logs or reports.

Evidence is the engine and version reported by the fixture, the commands run, the migration status before and after, row counts or constraint checks on synthetic data, and test results. Static review alone is analysis, not execution.

## Pitfalls

- Editing an applied migration changes its checksum and diverges environments; add a new migration instead.
- Adding a `NOT NULL` column or unique constraint to populated tables, renames that break running code during rollout, and long-running locks from index builds or rewrites.
- `NULL` comparisons, `NOT IN` with nullable subqueries, implicit casts, collation and case sensitivity, time zones, and integer division.
- Dialect-specific syntax passing a SQLite test fixture but failing on the production engine.
- Down migrations that silently drop data, or claims of reversibility that were never exercised.
- Windows: case-insensitive file systems hiding migration name collisions, CRLF in SQL files changing checksums, database files locked by another process, and path quoting in connection strings.

## Output

Provide the dialect-specific finding/change, data assumptions, deployment/rollback implications, and observed checks. If no authorized fixture exists, report execution not run and the exact evidence needed; do not invent query plans or timing.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
