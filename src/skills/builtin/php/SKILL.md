# PHP and Composer projects

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Read the project

- `composer.json`: `require` PHP and extension constraints, `config.platform` overrides, `autoload`/`autoload-dev` namespace mappings, and `scripts` (the project's own commands).
- `composer.lock`: the exact installed versions; keep lockfile changes attributable to an explicit dependency task.
- `phpunit.xml(.dist)`: test suites, bootstrap and environment variables; `tests/Pest.php` means Pest.
- Framework markers: `artisan`, `app/`, `routes/` and `config/` for Laravel; `bin/console`, `config/packages/` and `src/Kernel.php` for Symfony.
- Static analysis and style configs: `phpstan.neon(.dist)`, `psalm.xml`, `.php-cs-fixer(.dist).php`, `phpcs.xml(.dist)`, `rector.php`.

Identify the relevant namespace and package boundary before editing. Composer scripts may run application code and are not automatically safe.

## Discover commands in this order

1. Project-declared entry points: `composer.json` scripts, Makefile/justfile, CI workflows, CONTRIBUTING, AGENTS.md.
2. Ecosystem defaults, as candidates only when evidence supports them and `vendor/` is already installed:
   - `composer test` or another named script
   - `vendor/bin/phpunit <tests/Path/FileTest.php>` or `--filter <name>`
   - `vendor/bin/pest <file>`, `php artisan test --filter <name>`, `bin/phpunit`
   - `vendor/bin/phpstan analyse <paths>`, `vendor/bin/psalm`, `vendor/bin/php-cs-fixer fix --dry-run --diff`

Use the configured PHP binary and installed project dependencies. Do not assume a global executable matches the locked version. Composer install/update and networked package hooks require separate authorization.

## Toolchain variants

- PHPUnit vs Pest; Laravel vs Symfony vs plain libraries or WordPress plugins.
- PHPStan vs Psalm, with baseline files that record accepted errors.
- Docker or Sail wrappers that run PHP in a container instead of the host.
- Different CLI and web SAPI `php.ini` files enabling different extensions.

## Coding rules

Preserve autoload and public API conventions, validation, escaping, exception handling, and database transaction ownership. For generation, follow existing controller/service/domain patterns and supported PHP syntax. Do not run migrations or use example connection strings to validate a change.

## Verification evidence

- Run the test file or filter covering the change, then the relevant suite, then the full suite only when shared code changed.
- Run the configured static analysis and style checks on changed paths; do not regenerate baselines to hide new errors.
- Record PHP version/extensions and exact selected checks. Missing extensions, vendor dependencies, or service fixtures mean those checks are not run.

## Pitfalls

- On Windows, extensions are DLLs enabled in `php.ini`; a missing `ext-*` makes Composer refuse to install or tests fail. PECL extensions needing compilation are usually unavailable; report them as not run.
- `vendor/bin/*` shims are shell scripts; on Windows use the `.bat` variants or `php vendor/bin/<tool>`.
- PSR-4 autoloading is case-sensitive on Linux but not on Windows; a class name/file name mismatch passes locally and fails in CI.
- CRLF endings can fail style checks and break heredoc output comparisons.
- Framework caches (`bootstrap/cache`, `var/cache`) can serve stale config; clear only with project tasks and authority.
- File locking on Windows can block cache or log cleanup while a server runs.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
