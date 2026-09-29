# CS-3 controller identity recovery correction

Status: implementation and pure negative controls only. Native requalification
has not run. This correction is isolated from the frozen paid-comparison source
and its existing browser builds, receipts and observations.

Independent review found that `Test-ExactControllerAlive` treated every lookup
or creation-time exception as owner absence. `Recover-AbandonedProfile` could
therefore reach profile deletion after an access-denied or unknown identity
query instead of failing closed.

The helper now validates positive integral PID/creation identities and the
FILETIME range, restricts
absence handling to the local `GetProcessById(Int32)` lookup, and propagates
every other lookup or creation-time failure. An exact live match still blocks
recovery; a readable creation mismatch identifies PID reuse. Every returned
process object is disposed, including when its creation-time query fails.

Microsoft documents `ArgumentException` for an absent or expired PID in the
[local overload](https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.getprocessbyid?view=net-10.0#system-diagnostics-process-getprocessbyid(system-int32)).
The [.NET implementation](https://source.dot.net/System.Diagnostics.Process/System/Diagnostics/Process.cs.html)
throws that exact type after its local running-process check. The classifier
rejects derived argument exceptions and unwraps only the exact PowerShell
`MethodInvocationException` type; generic wrappers remain failures even when
they contain an argument exception. For wrapper behavior,
see [PowerShell exception handling](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_try_catch_finally).
Creation-time errors remain outside that classifier, including an exit race.

`Test-Contracts.ps1` uses injected identities and exceptions, never real process
queries or profile operations. It also checks an actual pure static-method
exception wrapper and executes the real recovery function with substituted
lookup/profile/receipt boundaries. Unknown and exact-live owners cause no
profile deletion, receipt write or drainage claim. Missing-PID and readable-reuse
paths reach the fake deletion and receipt operations using complete synthetic
owned-profile metadata; these positives prevent unrelated missing-field errors
from making the negative controls pass. No real profile is deleted.

Verification: 105 pure PowerShell assertions passed; `git diff --check` passed.
No native browser build/run, model request, package change or historical receipt
rewrite was performed. The wrapper and helpers belong to the 23-file browser
qualification closure, not the shipped Rust runtime. New source-bound native
qualification must be recorded separately before final acceptance; existing
comparison evidence remains verifiable under its preserved source snapshot.

## Superseding qualification join

`scripts/evals/cs3-controller-recovery-qualification.cjs` implements the unpaid
final join, registered in `cs3-webapp`. The source-bound decision starts with
null new-evidence pins; it cannot qualify until genuine observations are bound.
The original comparison gate retains its historical UI decision unchanged.

The historical projection authenticates the preserved source/archive, executable
and build identities, then invokes that source's actual review validator for all
six complete 18-slot comparisons. Every skill must genuinely qualify with two
independent readers and the required normal-case benefit. Dependency loads are
hash-checked and restricted to the authenticated closure; preloaded exports are
rejected and invocation-owned modules are evicted afterward. Source, plans and
protected evidence inventories are checked before/after review. This is not a
passed-summary import, and it writes no historical results.

The final join requires new source-bound WEB observations, all 23 UI controls,
pause/cancellation/owner-loss evidence, and unchanged adversarial/Node controls.
Cancellation and owner loss must carry exact process/token/job/profile evidence,
not just a successful status. Six normal UI slots retain their original returned
bytes and canonical invalid-output failures. Fresh browser status/assertion
vectors must equal the measurements shown to the old readers; any changed
semantics reopen acceptance rather than silently reusing stale reader judgments.

Seven synthetic model-free tests passed (58.6 seconds), including authenticated
module loading, cache substitution, source/root tampering, a complete synthetic
join, missing pins, lifecycle identity mutants and changed UI assertions. The
pending-pin test was subsequently made independent of production pin state and
passed its focused rerun. Synthetic archive pins and native graders are explicitly
substituted in these tests; they are not native qualification evidence. Genuine
native requalification and the final six-qualified-skill projection remain pending.

Independent review cleared the repaired cache, lifecycle-identity and retained-UI
boundaries, with seven focused tests passing in 61.6 seconds. The registered
`cs3-webapp` regression then passed all 133 tests, zero skips, in 65.3 seconds:
run `eee99917-da88-45cf-b8ef-9fdefdb3f390`, manifest SHA-256
`0adbf919a33c66839306573663848d75a3f18d227748c9281342107eb81ce219`.
Its offline harness timeout is now 180 seconds, based on the measured runtime;
no assertion, native deadline or paid-run limit changed. An earlier 130/133 run
retains three missing-source errors from the isolated sparse checkout. Restoring
those tracked conformance-source prerequisites resolved the setup failures.

## Actual corrected native qualification

On September 29, the corrected source at commit
`87809b55343f26b655e77b15edafa7a4720181b0` built 24 fresh harnesses and executed
the WEB pause, explicit cancellation, owner-loss recovery and all 23 UI controls
serially, with no concurrent compiler, test or paid workload. All six WEB cases
matched their oracles. The UI matrix matched all six positive and seventeen
negative expected vectors, including assertion/artifact tamper rejections.
All native receipts reported drained processes, no cleanup errors and removal
of their exact owned profiles. Model calls: zero. Visual review: not run.

Evidence is retained under
`artifacts/cs3-recovery-review/artifacts/cs3-recovery-native-01`; the aggregate
`native-references.json` SHA-256 is
`a134afab66e4928f871d0aaf318674585724c5384d0c611742ed6e68d14fc705`.
The 23-file native source closure is
`bb10c3f2350fbadd144077a48fa6bc69e79d38af0d40f5edabfed7c158da6153`.
The source-bound decision now pins these actual observations separately from
the historical receipts. Six-skill and final source-lineage pins remain pending;
successful browser qualification alone does not complete CS-3.
