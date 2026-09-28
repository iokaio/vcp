# Decision 031: bounded retention batches

Status: accepted on 2026-08-20. Owner: Storage Council.

Retention batch admission uses all three limits in the contract: entry count,
per-note Unicode scalar count, and aggregate UTF-8 payload bytes. Values at a
stated maximum are accepted. A rejected batch creates no retention records and
does not change an existing record.

The council accepted validation before persistence. It did not approve partial
acceptance, truncation, character replacement, or a retry with altered content.
