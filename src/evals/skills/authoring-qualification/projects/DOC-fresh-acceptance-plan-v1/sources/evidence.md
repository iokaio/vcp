# Evidence index

Snapshot E-8 from 2026-09-14, revision sv-34.

API fixture results: create/load round-trip of columns, filters and sort passed; duplicate-name rejection after trimming and case normalization passed; sixth-view rejection without changing the original five passed; cross-user list and load denial passed. These ran with synthetic accounts in the fixture harness.

Still unverified: cross-user rename/delete denial; deleting one view without changing siblings; empty and overlong names; same name on two dashboards; unknown/inaccessible view default fallback; browser sign-out cache clearing; save/rename network-failure behavior; keyboard navigation; default dashboard usability without saved views. Browser staging is unavailable, so browser checks have not run.

Review order: Mira closes remaining API scenarios first. Leon then validates browser scenarios against that revision when staging is available. Priya reviews both evidence sets before a release decision. Unavailable staging is a blocker to completing acceptance, not a waiver. No release decision has been made.
