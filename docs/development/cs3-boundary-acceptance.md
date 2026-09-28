# CS-3 native adversarial boundary controls

September 28, 2026: the zero-capability AppContainer denial matrix passes on
the current Windows host. This evidence joins, rather than replaces, the exact
browser descendant/token/job and lifecycle receipts in the
[native checkpoint](cs3-native-dom-checkpoint.md). No account, firewall default,
existing ACL or browser policy was changed.

## Reproduction and identity

Run `scripts/evals/cs3-boundary-qualification.ps1 -OutputDirectory <new directory>`
with a new directory beneath this repository's `artifacts/` directory, in the
native owner environment. The broker's redirected profile is not a supported
AppContainer profile environment and fails closed before creating a profile.
There are no model calls or package downloads.

The accepted local receipt is `artifacts/cs3-boundary-12/result.json`, SHA-256
`08d001bc81050e19fe12903b9eb84e72a57d9e9eb58550557c39aa60a7e5a98e`.
All inputs are hashed before and after qualification; copied executables and
probe source must match. Exact identities:

| Input | SHA-256 |
|---|---|
| Runner | `7a6b281ae724bd07be92e35897ad4f1f545a57da85d3a8009bc9d2516990b622` |
| JavaScript canary | `1b96b119a56735b9422d4bf97015a48e428f5341da6a44ba6cf912e8308a5804` |
| Existing AppContainer fixture | `650543c94cf1345921ff90fde09cd6def3feb3d9071acc622e07291e05509836` |
| Node executable | `3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5` |
| Breakaway source | `ec008efe568dc997e5ae10272fb916f040d715148310cb94532fc66161adb770` |

## Observations

An unrestricted positive control, restricted probe and second unrestricted
control run in fresh disposable profiles. The restricted token is independently
checked as AppContainer, matching the owned profile SID, with zero capabilities.
All three profiles are removed and both parent-owned listeners close.

| Operation | Positive controls | Restricted probe |
|---|---|---|
| Owned work-file read and writable open | Allowed | Allowed |
| Staged read-only resource read | Allowed | Allowed |
| Staged resource writable open | Allowed | `EPERM` |
| Private host-file read/writable open | Allowed | `EPERM` |
| Host files granting Everyone, Users or Authenticated Users | Allowed | `EPERM` for all reads/writable opens |
| Junction from owned directory to private host canary | Allowed | `EPERM` for read/writable open |
| IPv4 and IPv6 parent-owned loopback listeners | Connected | Native socket error; neither listener received a connection |
| External IPv4/IPv6 documentation addresses | Not contacted | `EACCES` |
| Suspended self-child with `CREATE_BREAKAWAY_FROM_JOB` | Access denied | Access denied |

Write probes open the existing synthetic file with read/write access without
mutating it. Parent checks independently confirm every canary's original bytes.
Only newly created synthetic directories receive ACL or integrity-label changes.
The breakaway helper verifies kill-on-close and absence of both breakaway flags
in the enclosing job. An unexpectedly created child would be terminated by its
exact handle while still suspended and fail qualification. This is a direct
no-breakaway property control, not a browser process-count stress test.

Windows reports the AppContainer loopback drop as native `ETIMEDOUT` before the
probe's 1,500ms watchdog. Acceptance requires that native error, both independent
listeners receiving nothing, and successful unrestricted connections before and
after. A watchdog timeout, refused connection or external timeout does not pass.
This is consistent with Microsoft's description of
[AppContainer loopback packet filtering](https://techcommunity.microsoft.com/blog/coreinfrastructureandsecurityblog/diagnosing-network-isolation-issues/2511562).
The external destinations are documentation IPs, with no DNS or payload, and
require explicit access denial; no claim of a reachable external-service control
is made.

## Scope and retained failures

These are synthetic token controls. They do not prove that Windows or installed
runtime resources are unreadable: those resources are required for execution.
The qualified browser uses the matching zero-capability boundary and separately
checks every retained process identity and its jobs. The owned server remains
outside the browser token, with the fixed, hash-acknowledged resource mediator;
the browser has no general loopback exemption.

Earlier attempts remain under `artifacts/cs3-boundary-01` through `-09`.
The initial controls exposed source-loading/integrity-label prerequisites;
another attempt rejected the native loopback error, and `-08` failed on the
broker profile mismatch. These are retained failures, not passing retries within
one receipt. `-09` passed; `-10` repeats the complete matrix after review added
rejection of redirected output-directory ancestors before any creation or ACL
changes. A separate negative path control confirms rejection before creating any
directory through the junction. `-11` and final `-12` repeat the matrix with
profile names and exact cleanup targets journaled before creation; the
post-creation journal write is protected by the disposal boundary. All three
profile records end in `removed`. This document does not close WEB execution,
original-UI regrading or six-skill comparisons.
