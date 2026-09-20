# PHP and Composer projects

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Inspect the PHP contract

Read composer.json/composer.lock, PHP and extension constraints, autoload mappings, framework bootstrap, and test/static-analysis configuration. Identify the relevant namespace and package boundary before editing. Composer scripts may run application code and are not automatically safe.

Use the configured PHP binary and installed project dependencies. Select the declared PHPUnit/Pest/static-analysis command or Composer script; do not assume a global executable matches the locked version. Preserve autoload and public API conventions, validation, escaping, exception handling, and database transaction ownership.

For generation, follow existing controller/service/domain patterns and supported PHP syntax. Do not run migrations or use example connection strings to validate a change. Composer install/update and networked package hooks require separate authorization.

## Results

Record PHP version/extensions and exact selected checks. Missing extensions, vendor dependencies, or service fixtures mean those checks are not run. Keep lockfile changes attributable to an explicit dependency task.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
