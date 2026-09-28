I propose keeping the [five-phase plan](/D:/code/Github/vcp/docs/development/cs3-implementation-phases.md), with explicit exit gates. The next milestone should be **reliable browser execution**, not another isolated successful diagnostic.

### 1. Finish browser readiness and boundary qualification

First, investigate one concrete alternative to a timing delay: whether input must start on a later UI-message-loop turn, after the navigation callback returns. Microsoft documents this scheduling distinction, but it is a hypothesis—not an established explanation for our failure. [WebView2 threading model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/threading-model)

Then:

- Establish observable input readiness. A harmless keyboard sentinel is worth testing in our controlled fixture; it must not be assumed harmless in arbitrary applications.
- Send the actual form action once. Never retry Enter blindly or substitute a synthetic DOM submission.
- Freeze the implementation and repeat fresh-profile, timeout, cancellation and owner-loss tests, retaining every failure.
- Resolve the development-only runtime flag, runtime integrity, internal browser-sandbox evidence and storage limits.

I would investigate a private, pinned runtime for reproducibility, with an explicit security-update policy. Microsoft supports Fixed Version distribution, but maintaining its updates becomes our responsibility. [Runtime distribution options](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version)

**Exit:** repeatable interactions and demonstrated containment. If the bounded investigation fails, evaluate another isolated browser boundary—not weaker permissions or progressively longer sleeps.

### 2. Join the browser to the owned server

My preferred design is the narrowly mediated route already considered in the boundary review: keep the browser without network capability and relay only approved requests to one VCP-owned server.

Before implementation, record whether that design satisfies the intended web-testing scope; it must not silently narrow the requirement.

Test unauthorized services, redirects, malformed requests, occupied ports, long polling, pause/cancellation and owner loss. Preserve unrelated listeners.

**Exit:** the combined browser/server lifecycle and access restrictions pass native Windows tests.

### 3. Complete executable WEB and UI checks

Connect the prepared WEB fixtures to independent browser assertions covering forms, keyboard/focus, accessibility, filters, loading/error/retry, viewport overflow and reduced motion.

Regrade retained CS-2 UI artifacts without modifying their historical bytes or outcomes. Update `webapp-testing` to describe the actual supported interface and limitations.

**Exit:** executable positive and negative controls pass. Screenshots and DOM checks must not be presented as human visual review.

### 4. Qualify all six skills

Candidate corrections and offline preparation can run alongside browser work. Prioritize the documented document-authoring and MCP defects; avoid gratuitous rewrites elsewhere.

Then freeze fresh tasks, candidate versions, baselines and independent grading. The current contract requires **108 model task runs**, plus separately specified review allowances. This needs a new, explicit dollar/request budget. [Comparison requirements](/D:/code/Github/vcp/docs/development/cs3-six-skill-readiness.md:52)

**Exit:** all six satisfy correctness and comparative-benefit requirements. Ties or failures cannot be relabeled complete.

### 5. Package and deliver for review

Promote only qualified versions. Verify exact-package installation, upgrade/rollback, offline discovery, activation/revocation and integrity; run final checks and update the PR. Stop before merge.

My recommendation is to begin with the bounded readiness investigation while preparing candidate corrections in parallel. No RDP change is needed for that work. Additional provisioning would require an exact reviewed proposal; paid evaluations require a fresh budget. **CS-3 completion remains contingent on the evidence passing—not merely finishing the implementation.**