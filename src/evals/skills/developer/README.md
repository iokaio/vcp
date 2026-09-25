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
dependencies. The MCP project uses a bounded in-memory handler contract; its
independent stdio adapter/client is future executable qualification. The injected
LLM transport is synthetic, records bounded calls and never accesses a network.
It is not an installed third-party SDK or proof of OpenRouter compatibility.
Primary references and actual selected SDK versions still require qualification.

`developer-oracle.cjs` checks exact output shape, safe paths, bounded edits,
preservation, synthetic-canary handling and local HTML asset paths. It never loads
or executes generated code. Functional vectors describe future independent tests;
matching prose or candidate-supplied success logs cannot prove behavior. Browser
interaction, accessibility, visual/layout review and lifecycle remain with CS-3.

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

`developer-semantic-contract.cjs` provides independent assertions for eight cases.
Trusted test doubles run in bounded disposable Node children; no model artifacts
are loaded by this library or the structural oracle. These tests do not qualify
an untrusted-code adapter, MCP stdio transport, actual protocol pagination or a
selected SDK. Those integration and process-lifecycle checks remain not run.
Explicitly loaded near-miss guidance tests restraint, not selection. Human quality
and executable semantic checks remain pending even when structural checks pass.
The default catalog and candidate packages are unchanged by these fixtures.
