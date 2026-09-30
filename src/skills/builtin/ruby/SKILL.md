# Ruby and Bundler projects

Original VCP guidance.

## Read the project

- `Gemfile` and `Gemfile.lock`: dependencies, groups, the locked Bundler version and platforms.
- `.ruby-version`, `.tool-versions` or a `ruby` line in the Gemfile: the interpreter the project expects.
- `*.gemspec`: a library gem, its public files and runtime dependencies.
- `Rakefile` and `lib/tasks/`: declared tasks, including the default test task.
- Rails structure (`config/application.rb`, `app/`, `bin/rails`, `db/schema.rb`): only this confirms Rails. Do not infer Rails merely from a Gemfile.
- `spec/` with `.rspec` and `spec_helper.rb` means RSpec; `test/` with `test_helper.rb` means Minitest.
- `.rubocop.yml`, `.standard.yml`: the lint and format policy.

## Discover commands in this order

1. Project-declared entry points: `bin/` scripts, Makefile/justfile, CI workflows, CONTRIBUTING, AGENTS.md, Rake tasks.
2. Ecosystem defaults, as candidates only when evidence supports them and the bundle is already installed:
   - `bundle exec rspec <spec/file_spec.rb>`, narrowed with `:<line>`
   - `bundle exec ruby -Itest <test/file_test.rb>` or `bin/rails test <file>`
   - `bundle exec rake test` or `bundle exec rake spec`
   - `bundle exec rubocop <files>` or `bundle exec standardrb <files>`

Use the pinned Ruby and project bundle. A `bundle exec` invocation is appropriate only when the bundle is already available and the selected command is configured. Rake and framework initialization can contact services or mutate state; inspect the selected task and use current authority. Do not run `bundle install` automatically.

## Toolchain variants

- Version managers: rbenv, rvm, chruby, asdf/mise; RubyInstaller with MSYS2 DevKit on Windows.
- Rails apps vs plain gems vs Sinatra/Hanami services.
- RSpec vs Minitest; system tests using Capybara and a browser driver.
- Binstubs in `bin/` (including Spring) that wrap commands.

## Coding rules

Preserve public method behavior, exception handling, encoding, loading/autoloading, and database boundaries. Generate changes consistent with the framework and nearby tests rather than adding a different service pattern. Respect Zeitwerk file-name-to-constant conventions in Rails.

## Verification evidence

- Run the spec or test file covering the change, then the related directory, then the full suite only when shared code changed.
- Run the configured RuboCop or Standard check on changed files.
- Return the relevant source/test references, interpreter and runner identity, and observed outcomes.
- Tests needing a database, Redis or other services are not run unless those fixtures already exist and are authorized; do not run migrations against real databases.

## Pitfalls

- On Windows, native gems (for example database adapters or parsers with C extensions) need the DevKit toolchain and may be unavailable; report those specific checks not run while continuing source analysis.
- `Gemfile.lock` platform entries differ between Windows and Linux; do not commit incidental platform changes.
- CRLF endings can break shebang scripts in `bin/` and heredoc-sensitive tests.
- Running without `bundle exec` loads global gems of different versions.
- Spring or bootsnap caches can serve stale code; note when they were active.
- File locking on Windows can block removal of `tmp/` or log files held by a running server.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
