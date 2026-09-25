# CS-2 developer fixture preparation

Original Apache-2.0 VCP fixtures: eighteen cases, six each for frontend-design,
MCP-development and LLM-integration. Each candidate has two normal tasks and one
boundary, hostile, missing-prerequisite and near-miss task. The separate authoring
samples are not held-out inputs. Do not tune packages against held-out solutions.

The manifest hashes source files, independent per-case oracles, shared rubric and
three-arm assignments. `author-fixtures.cjs` records original maintenance inputs;
ordinary evaluation reads frozen files and never regenerates them. Changes after
execution require a new revision and cannot erase failed runs.

Only the selected project, prompt and context enter the isolated task workspace.
Keep oracles, rubric, comparison labels and other projects out of model context.
Model output uses `{files:[{path,content}],report:string,not_run:[string]}` and must
match actual authorized edits. Report-only cases authorize no writes. Every case
preserves dependencies and all unlisted paths. No fixture authorizes process,
network, SDK installation, registration or external credentials.

These original Node/JavaScript and JSDoc skeletons deliberately require no added
dependencies. The MCP projects use a bounded in-memory handler contract. The injected
LLM transport is synthetic, records bounded calls and never accesses a network.
It is not an installed third-party SDK or proof of OpenRouter compatibility.
Primary references and actual selected SDK versions still require qualification.

`developer-oracle.cjs` checks exact output shape, safe paths, bounded edits,
preservation, synthetic-canary handling and local HTML asset paths. It never loads
or executes generated code. Matching prose or candidate-supplied success logs cannot
prove behavior. Browser interaction, accessibility, visual/layout review and
lifecycle remain with CS-3.

The 54 three-arm runs are planned, not executed or authorized by these files.
Revision v2 corrects MCP's nearest baseline to architecture together with
javascript-typescript, as required by the formal plan. Other cases use
javascript-typescript alone. Each arm activates exactly its assigned skill array;
the candidate arm still activates only its candidate. Original task facts,
oracles and the fifty-four planned runs remain unchanged.
Revision v3 then replaces only the underspecified `MCP-boundary-pages-v1` case
with `MCP-boundary-pages-v2`, declaring exact synthetic pagination, delayed queue,
cancellation, flush and reply-size behavior before execution. The earlier v2
metadata is retained under `history/cs-2-developer-fixtures-v2`; its referenced
original case files remain unchanged. The cohort still has eighteen cases and
fifty-four runs. `fixture/*` methods are local synthetic APIs, not MCP standards.

Revision v4 preserves the exact v3 metadata under
`history/cs-2-developer-fixtures-v3` and all original project/oracle bytes. Five
new v2 cases clarify tool schemas and unknown-record errors, typed transport
responses, deterministic streaming cancellation and limits, and REST name
validation. The thirteen other cases, including `MCP-boundary-pages-v2`, retain
their exact metadata and bytes. The cohort remains eighteen cases and fifty-four
planned runs; no model execution is claimed or authorized.

Revision v5 preserves the exact v4 metadata under
`history/cs-2-developer-fixtures-v4`, the v1 `rubric.json` and every earlier
project/oracle byte. It freezes the cohort for execution under the current
runtime. All eighteen cases have new IDs, because every prompt and tool allowlist
changed:

- **Tools.** Every case may use read-only `vcp_search` and `vcp_verify`. Write
  cases may also use `vcp_patch`, for exactly their editable paths. These
  allowlists are bound through the canonical tool ceiling.
- **Checker hook.** Write cases carry the in-run checker hook in their own
  `package.json`. Their prompts permit only that configured read-only checker.
- **UI seams.** The UI results and state-machine cases add pure,
  browser-loadable `filterItems` and `transition` seams.
- **MCP initialize.** MCP `initialize` returns `capabilities` and `serverInfo`,
  and `handle` never writes to stdout or stderr.
- **Rubric.** `rubric-v2.json` adds evidence honesty, halt classes, functional
  grading, a human layout rubric recorded `not_run` and the predeclared benefit
  rule.

Each oracle declares `functional_grading` as `none`, `single_shot` or
`interactive`.

[`developer-grader.cjs`](../../../../scripts/evals/developer-grader.cjs) grades
eleven cases functionally. It runs the final-workspace artifact beside a
grader-authored wrapper in the qualified Windows Node fixture adapter. The parent
holds every expected value:

- **Single-shot batches** cover the pure functions, the scripted boundary MCP
  session (observed through replies and `flush()` output, not internal state) and
  stream byte partitions.
- **Interactive sessions** make the parent the MCP stdio client, the LLM
  transport or the stream iterator.

The other seven cases have mode `none`: structural checks and blind readers.
Trusted doubles exercise the probe logic portably with an unqualified local
executor, which can never produce campaign evidence.

Explicitly loaded near-miss guidance tests restraint, not selection. Human quality
remains not run. The default catalog and candidate packages are unchanged by these
fixtures.
