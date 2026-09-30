# Test the web application

Adapted from Anthropic's Apache-2.0 webapp-testing skill and Playwright examples. See [UPSTREAM.md](UPSTREAM.md).

## Choose the shortest useful route

Use browser testing when the task depends on rendered controls or browser behavior. Keep ordinary unit tests in the existing test runner. Inspect the application's package scripts, current tests and framework before adding a separate harness.

For static HTML, reading the markup can reveal the controls and selectors. For a dynamic application, inspect the rendered state after a meaningful readiness condition. Prefer the project's installed Playwright or other supported browser tooling. A missing browser is a setup limitation, not proof that browser testing is impossible for VCP: continue useful source/unit checks and name the missing dependency. Do not silently install tooling or switch to a user's signed-in browser profile.

Identify the permitted target origin and whether the server is already user-owned. Use an existing authorized server when appropriate, or start an owned server through the project's or VCP host's supported process tools. Wait for a bounded application readiness signal. An occupied port is not permission to terminate or adopt another process. Stop only servers you started.

## Reconnaissance, action, assertion

Navigate to the permitted page, then wait for the control, heading or application state needed by the task. Long polling and open sockets are normal; global `networkidle` and fixed sleeps are poor readiness tests.

Inspect the rendered DOM, accessible roles/names and relevant input fields. A screenshot can help when image review is available. Select controls by role and accessible name, label or stable test ID before fragile layout selectors. Then perform the actual interaction and assert its user-visible result. A successful click with no result assertion is incomplete.

Cover the requested success path and relevant failure/recovery states. For forms, check labels, keyboard access, validation, retained input and duplicate submissions where applicable. For filtering, check the actual membership and empty/reset state. For layout changes, check representative narrow and wide viewports. Keep the test proportional to the changed behavior.

Read [references/browser-patterns.md](references/browser-patterns.md) when writing a new browser test. The optional [scripts/check-page.cjs](scripts/check-page.cjs) ports upstream reconnaissance and console-capture examples into a small Node/Playwright check for an already running, permitted loopback application. Run `--help` first. It uses the project's installed Playwright, opens an isolated context, blocks cross-origin HTTP requests, redirects and sockets, and closes its owned browser. It does not start servers or install dependencies. The helper is a hash-verified `file` resource and is not in your context. Copy it with `vcp_skill` (action `materialize`, resource `scripts/check-page.cjs`, destination a new file in an existing workspace directory, such as `check-page.cjs`). Run the copy with `vcp_exec` only through an authorized Node process profile; otherwise report the helper as not run. Remove the copy with `vcp_patch` afterwards unless the user wants to keep it.

## Capture enough to explain the result

Capture relevant console/page errors while exercising the page. Use synthetic data and avoid retaining secret-bearing logs or screenshots. The example captures console counts by default; text capture and a screenshot require explicit options. Save artifacts only to the requested output location. A screenshot file does not prove someone reviewed the pixels.

Return the interactions and assertions actually observed, the browser/target used and useful failure details. Separate source findings, browser checks and visual review. Normal browser testing does not require a paid model comparison, a receipt campaign or the historical CS-3 qualification system.

Browser automation remains within the existing tool, origin and process permissions. The example's browser routing is a test guard, not an OS network sandbox or proof of host cleanup after process death. Use host-owned process supervision for cancellation and owner loss; no skill can grant that authority.
