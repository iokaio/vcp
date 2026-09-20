# P7-01 native lazy-discovery experiment

`manifest.json` freezes two synthetic catalogs (4 and 128 packages), each package's
32 KiB body and 1 KiB resource, matching/nonmatching project cues, and the required
100% assertion rate before execution. Changed expectations require a new revision
with an explained rerun; failed cases remain in the report.

Run from the repository root with the provisioned native environment:

```powershell
pwsh scripts/evals/skill-discovery-qualification.ps1 -Jobs 4
```

The runner creates a unique ignored artifact directory, records source identity
before and after execution, and retains contract-test logs, hardware/toolchain
information and report hashes. It never overwrites existing evidence.

The example creates actual local files and invokes production discovery, matching,
activation and dependency revalidation. It reports descriptor/body/resource read
counts and bytes, selected-content revalidation reads, descriptor-listing JSON
size, and local elapsed time. Discovery must load no body/resource content;
activation must load only the explicitly selected package. Both matching and
nonmatching project cases stay in the denominator.

Listing bytes describe the declared short-description projection; they are not
model tokens or actual provider request sizes. Retained-host context tests cover
the latter boundary separately. The experiment does not contact a model, install
or execute a toolchain, or qualify skill usefulness. It makes zero model calls
and spends zero dollars. Single-host timing with uncontrolled cache effects is
reported as observation, not a production performance guarantee.
