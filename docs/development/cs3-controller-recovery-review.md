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
