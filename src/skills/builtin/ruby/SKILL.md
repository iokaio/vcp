# Ruby and Bundler projects

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Read Ruby project evidence

Inspect Gemfile/Gemfile.lock, Ruby version declarations, gemspec when present, Rakefile and test configuration. Determine whether the project uses RSpec, Minitest, Rails tasks, or another declared runner. Do not infer Rails merely from a Gemfile.

Use the pinned Ruby and project bundle. A bundle exec invocation is appropriate only when the bundle is already available and the selected command is configured. Rake and framework initialization can contact services or mutate state; inspect the selected task and use current authority. Do not run bundle install automatically.

Preserve public method behavior, exception handling, encoding, loading/autoloading, and database boundaries. Generate changes consistent with the framework and nearby tests rather than adding a different service pattern.

## Evidence

Return the relevant source/test references, interpreter and runner identity, and observed outcomes. On Windows, native gems or framework dependencies may be unavailable; report those specific checks not run while continuing source analysis.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
