# Governed memory and retention hygiene

Original VCP guidance.

## Inspect first

- VCP's governed surfaces, confirmed with `--help` on the current installation: `vcp memory search <text>` and `vcp memory inspect <claim>` for claims and their evidence, `vcp history list` and `vcp history search <text>` for raw history, and `vcp retention show` for the saved policy. In the terminal, `/history`, `/memory inspect`, `/prune` and `/retention` reach the same controls.
- Candidate stale entries: claims whose applicability no longer matches current source, superseded preferences, and recall that points at removed files.
- Candidate duplicates, checked by lineage and source evidence rather than similar wording.
- The exact scope requested: workspace, path, claim, date range or filter.
- Protection rules: active-task recovery, unsettled accounting, dependent derived records, and protected references a preview reports.
- Backup and retention state: which retained backups or external copies hold the selected content.

## Inspect scope and provenance

Use governed memory/history inspectors to identify claims, versions, source evidence, applicability and current access. Distinguish raw history from semantic recall, inferred claims from confirmed preferences, and stale applicability from incorrect historical evidence. Similar wording alone does not establish a safe duplicate.

Prefer correcting or superseding governed claims when history remains useful. Exclusion, presentation compaction, restore-to-recall and purge are different operations. A purge proposal needs an exact scoped/date/filter preview with protected accounting/recovery references and dependent artifacts visible.

## Proceed and verify

1. Establish the explicit scope from the user. Never delete, purge or exclude anything without an explicit scope; a general request to tidy up is a request for a proposal.
2. List stale or duplicate candidates with their source evidence and why each qualifies. Prefer correction or supersession over removal.
3. Prepare the least destructive operation that meets the request with `vcp memory prune --preview` (claims) or `vcp history prune --preview` (history), explicit filters such as `--since`, `--before`, `--task` or `--path`, and an explicit `--action exclude|restore-recall|compact|purge`; the default action is purge. Review it with `vcp prune show <preview-id>`: selected items, counts, protected references, dependents, recall impact and backup copies that will remain.
4. Apply only the developer-selected current preview with `vcp prune apply <preview-id>`; do not delete index files or mutate a store directly. Recheck stale preview failures rather than broadening the selection.
5. After applying, inspect again and confirm what is excluded now and what cleanup remains pending; `vcp prune cleanup <receipt-id>` completes pending cleanup for an applied receipt when that is in scope.

Evidence is the preview identity, the applied selection and counts, the reported cleanup state, and a post-change inspection. A proposal or preview is not a completed cleanup.

## Pitfalls

- Treating wording similarity as duplication and removing the version with better provenance.
- Deleting outdated but historically accurate evidence instead of marking it superseded or no longer applicable.
- A preview that silently grows because new records matched after it was made.
- Removing records that protect active recovery or unsettled accounting.
- Claiming erasure while backups, retained snapshots or external copies still hold the content. Removing current recall does not erase an old cloud backup or an independently retained copy.
- Editing memory or index files by hand, including on Windows where a locked file can leave a store partially modified.

## Report

State what is excluded now, what physical cleanup completed or remains pending, and what external copies remain outside local control. Preserve historical attribution and unsettled obligations. No universal erasure promise, automatic retention policy, or tool authority follows from this guidance.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
