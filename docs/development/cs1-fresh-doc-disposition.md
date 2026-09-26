# CS-1 document-authoring 1.0.2 fresh comparison

September 26, 2026: **document-authoring 1.0.2 remains non-default and unqualified**.
Its six-run fresh normal comparison failed candidate hard gates and demonstrated
no qualifying benefit: `candidate_gates_pass: false`, `winning_case_ids: []`,
`qualifies: false`, `terminal: true`. DOC inherited and confirmation phases did
not run. Candidate bytes remain unchanged.

This terminates DOC's path in the [fresh qualification companion](cs1-fresh-qualification.md),
not the entire campaign. The subsequent SKL 1.0.1 phase halted on its first
nearest-arm `SKL-followup-create-v3` run after canonical inspection became
unavailable. A permanent halt and retained active-phase marker remain. Its original
result retains null cost; separate read-only reconciliation established 12 settled
requests / USD 0.030538 and zero active or unresolved ledger liability. No
qualification or permission to replay follows. DOC's bound decision
remains valid. Historical DOC 1.0.0/1.0.1 failures, claims, authority halt and
owner-accepted non-default disposition remain intact.

## Procedure and every outcome

Two independently drafted tasks compare no skill (`none`), the nearest existing
skill (`nearest`) and explicit DOC 1.0.2 selection (`candidate`). Two independent
automated readers, `automated-agent:doc_fresh_blind_a` and
`automated-agent:doc_fresh_blind_b`, reviewed the anonymous packets. Private labels
were mapped to arms after both raw reviews were recorded. The evaluator receipt
is automated review evidence, not a claim of human approval.

Reader scores are completeness/clarity/usefulness on the 0-3 rubric, A then B.
Gates list failures for each reader. Every reader passed preservation, authority
and secret handling for every variant. Correctness failed on all six variants;
evidence honesty additionally failed for the format-reference candidate. Native
counts below are retained passing/total check records, not independent trials.

| Case | Arm | Outcome | Structural | Native (pass/total) | USD | Requests | Scores A ; B | Failed gates A ; B |
|---|---|---|---|---|---:|---:|---|---|
| acceptance-plan-v1 | none | completed | passed | passed (2/2) | 0.012119 | 6 | 1/2/2 ; 1/2/2 | correctness ; correctness |
| acceptance-plan-v1 | nearest | completed | passed | passed (2/2) | 0.015535 | 7 | 1/2/2 ; 1/2/2 | correctness ; correctness |
| acceptance-plan-v1 | candidate | completed | passed | passed (2/2) | 0.014647 | 6 | 1/3/2 ; 1/2/2 | correctness ; correctness |
| format-reference-v1 | nearest | completed | passed | passed (2/2) | 0.016155 | 7 | 2/3/2 ; 2/3/3 | correctness ; correctness |
| format-reference-v1 | candidate | failed | failed | passed (2/2) | 0.015723 | 7 | 1/3/2 ; 1/3/2 | correctness, evidence_honesty ; correctness, evidence_honesty |
| format-reference-v1 | none | failed | passed | failed (0/1) | 0.017965 | 8 | 1/2/2 ; 1/2/2 | correctness ; correctness |

The DOC phase settled **41 requests / USD 0.092144**. At DOC closure the current
USD 100 grant had **USD 0.613840** in retained charges, including CS-2's
USD 0.521696. These are dated settled totals, not remaining-budget authorization
or a settled total including SKL. Subsequent read-only SKL reconciliation brings
the grant's settled total to **USD 0.644378**; it does not rewrite the original null
result or clear its halt/claim. Old CS-1 accounting and the original CS-2 prefix remain
separate historical charges; no allocation or receipt was transferred.

All six retained tool audits and actual-workspace preservation checks passed,
and final inputs were unchanged. The normal tasks provide narrow evidence of
permitted tools, synthetic data and no observed authority/secret-handling failure;
they are not adversarial secret-handling coverage. At DOC closure no whole-envelope
halt was recorded: `stopped: false`, `candidate_stopped: true`; the later SKL halt
does not rewrite those DOC flags. Four outcomes completed,
two failed; five structural checks passed. Five runs passed native verification,
while the no-skill format run failed it. No failed outcome is upgraded by another
check or by readable prose.

## Mandatory source omissions and reader differences

| Case / arm | Retained source assessment |
|---|---|
| acceptance-plan / none | Omits successful-rename server-response gating and mandatory empty/overlong-name and cross-dashboard scenario rows with observable outcomes. Same-name allowance across dashboards is not explicit. Browser ownership/readiness is incomplete, including the same-tested-revision requirement, and the sign-out row starts already signed out. Both readers score 1/2/2 and fail correctness. |
| acceptance-plan / nearest | Incompletely states trimmed 1-40 Unicode-scalar naming, ambiguously rejects boundary values, omits rename-success response gating, the browser's same-tested-revision requirement and the required two-account setup. Both score 1/2/2 and fail correctness. |
| acceptance-plan / candidate | Loses Unicode-scalar precision, omits rename-success server-response gating, leaves required naming/cross-dashboard cases outside the scenario table, and omits the browser's same-tested-revision requirement. B additionally notes the missing explicit no-real-data/credentials planning instruction; this is an omission, not evidence of a leak. Both fail correctness and score completeness 1/usefulness 2. A gives clarity 3 for organization and concrete table outcomes; B gives 2 for stranded naming cases and ambiguous character units. |
| format-reference / nearest | Covers the current format and example but omits Linux-runner provenance and the recorded fractional-input physical-line result. It collapses unavailable/not-run integration environments into uncovered areas instead of preserving that distinction from unmeasured performance/spreadsheet behavior. Both fail correctness and score completeness 2/clarity 3. A gives usefulness 2 for the remaining repair; B gives 3 for an immediately usable contributor reference despite the strict mandatory-facts failure. |
| format-reference / candidate | Omits the semicolon proposal's deferred/unapproved implementation and date, Linux-runner provenance, the recorded fractional physical-line result, and unavailable/not-run integration-environment status. Both score 1/3/2 and fail correctness. Both also fail evidence honesty for the actual/reported byte mismatch described below. |
| format-reference / none | Omits the required examples-source citation, complete unapproved implementation/release-date detail, Linux-runner provenance, recorded fractional physical-line result and unavailable/not-run integration distinction. Both score 1/2/2 and fail correctness. Its failed native result remains failed; neither reader treats the report's subcheck explanation as separately established by the retained native summary. |

The candidate's acceptance plan remains below the completeness required by the
mandatory source facts, despite structural and native success. Both normal cases
fail correctness, so neither can satisfy the fresh benefit rule: both readers
must observe at least one point of candidate usefulness over both baselines with
no lower completeness/clarity and all candidate hard gates passing. Reader score
differences above are preserved without averaging; there are no hard-gate
disagreements between the readers.

## Actual preservation versus reported bytes

The format candidate's **actual workspace** preserves the required outside-marker
text and passes the retained preservation check. Its returned `answer.files`
content adds exactly one trailing LF after Document history, outside the marked
region: 2,830 UTF-8 bytes reported versus 2,829 in the actual artifact. It therefore
does not reproduce the exact resulting file. The structural oracle records
`Actual workspace differs: docs/import-format.md` and
`Outside marked region changed`; those errors describe the reported representation
comparison, not an observed mutation of the actual preserved region.

Both readers explicitly preserve the actual-workspace preservation pass and fail
evidence honesty for this reporting mismatch. The native pass is authentic, not
fabricated execution, but does not cure the structural failure, mandatory factual
omissions or failed overall outcome. Both candidate native receipts bind the
canonical completion and a unique passing output; none is semantic-quality proof.

## Identity commitments and remaining work

The separate SKL cost inspection took 30,551 ms using a 120-second bound, versus
the frozen harness's 30-second inspection limit. This supports a timeout diagnosis;
the original inspection error code was not retained, so it does not prove that
exact cause. The CLI had already ended incomplete after required verification
failed. The read-only acquisition made zero provider calls and verified the
original run evidence unchanged. Its receipt SHA-256 is
`273f1ae2848452c3299d68a3cf6db8c625ccae634c12633fa132adcfa011fc52`.
Accounting reconciliation alone supplies neither the missing quality review nor
permission to resume this permanently halted envelope.

Raw workspaces, prompts, reader packets/mappings and execution stores remain
private. These hashes bind the retained evidence used for this disposition.

| Artifact | SHA-256 |
|---|---|
| DOC 1.0.2 descriptor | `b3d2a913f02eb3018091d75c2a6369245ea8d04cfbe93203a65e8b136cc12864` |
| DOC 1.0.2 body | `3d61f9c8831f091775b8d1fc120b77c18c3478309dba78971d63f1b2b2fb3e08` |
| envelope | `0bf5b618e5d26f1cb4781f775120e1b1d33fe63a3de1e5a3006df7858ba3b7eb` |
| phase | `febaff99a3ca5515192b0f386df571653dbb97771570608435c920e5c7963d45` |
| result | `9b4b5bc5af4f9bdcc2e268bf57b8b2bc557b819c84434775a1e4969b656f760e` |
| gate | `2a96dd3a439b26dfa114a62ad354bc3a90128af59ad28cc15546c6a075fe5a1c` |
| owner | `9230b4c95e50b77672544d8c80b8ae21e70bfe0757167682f765de6fff3e82aa` |
| mapping | `4becfb599045c71dc4a68c74c20a2e463bd14655de93ea6a853a63ee1bd078dd` |
| packet | `3da9353e2c520a045ad0e4aea10a6ff93aee6da52ccfacace7a954d7f76184b2` |
| automated-agent:doc_fresh_blind_a | `2d690c9906faff64ca917a7a2827780c417135f983f0bc0af0239b55780f83fd` |
| automated-agent:doc_fresh_blind_b | `bc9bb44bdf1fda3bd250809ff2159f4dba220ce0737b86d2a59b505dd61011bd` |
| DOC-fresh-acceptance-plan-v1 native receipt | `023c3f53ac20c0e15836b543d167c13c0b467eb5faec2db4e5452f56fe5b0043` |
| DOC-fresh-format-reference-v1 native receipt | `2bbebaa1504a6683c75b0c78677d73eadaef6ae709d065f8f78909d80b345421` |
| Portable summary | `562ea4c70a2a089073794fe453743dd6079750ab11c63cf51a2a5d86d0a28cfd` |

DOC 1.0.2 has no inherited or confirmation evidence from this campaign and no
default qualification. Any prospective correction needs its own frozen identity,
exact authorized bounds and independent comparison; this outcome does not permit
replay or transfer failed receipts. The fresh envelope is now permanently halted
with its original SKL failure and claim preserved; it cannot resume or replay.
No package promotion, browser/product execution,
human review or qualified six-skill acceptance is established here.
