# CR-06 external debug scenario observations

The separate frozen `src/evals/skills/builtin/debug-v1/manifest.json` supplies four
debugging scenarios without changing the original 42 built-in skill cases:

| Case | External check |
| --- | --- |
| `seeded-failure` | Original threshold defect fails; current candidate passes the boundary inputs. |
| `interrupted-instrumentation` | Same fix, with the exact owned trace marker removed. |
| `concurrent-human-edit` | Same fix, with the exact human-modified diagnostic line retained and cleanup labelled blocked. |
| `missing-reproduction-access` | No candidate execution; verification remains explicitly not-run while file preservation is checked. |

Prepare a fresh disposable workspace and give the case prompt to a separately
authorized skill trial. Keep the manifest and oracle outside its read grant.
Preparation does not run a model or modify the frozen source. After the trial,
grade its final workspace with the network-denying Node runtime used by the
[generation oracle](builtin-generation-runner.md):

```powershell
node scripts/evals/builtin-debug-oracle.cjs prepare interrupted-instrumentation C:/private/new-debug-workspace
& C:/private/runtime/node.exe scripts/evals/builtin-debug-oracle.cjs grade interrupted-instrumentation C:/private/new-debug-workspace
```

For available reproduction scenarios, the oracle independently executes a fresh
copy of the original source and then the candidate under bounded read-only Node
permissions, with no provider environment, network, child processes or writes.
It retains the original and final values, stdout, stderr, exit status and errors.
It checks the full file inventory, immutable neighboring file bytes, current file
hashes before/after observation, removed owned marker and retained human line.
It does not prescribe the candidate's final source formatting or patch size.

These are behavioral fixture checks, not a hostile-code correctness proof. A
candidate can spoof the output marker or modify JavaScript globals inside its
own process; the oracle does not authenticate those observation values against
adversarial candidates. Node permission controls bound filesystem/process/network
authority but do not establish that the candidate reported genuine computation.
Independent review must inspect the changed source for such interference.

`controls_pass` measures only those file/check controls. For missing access it
can be true while `verification_complete` remains false; that is preservation
evidence, not evidence the proposed fix works. No case grades model prose or
claims actual live skill usefulness. Independent review must still establish
that the model used the original evidence, explained missing access or blocked
cleanup honestly, and produced a useful fix in the scoped trial. The interrupted
case begins from an instrumented state; it does not simulate engine interruption
or prove recovery correctness.

The deterministic contract test uses corrected disposable copies as positive
controls and rejects the original defect, retained owned instrumentation,
overwritten human diagnostics, and lost neighboring work. Run:

```powershell
& C:/private/runtime/node.exe --test src/tests/contracts/builtin-debug.test.cjs
```
