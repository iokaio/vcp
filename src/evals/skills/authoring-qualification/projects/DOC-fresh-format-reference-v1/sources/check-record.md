# Recorded parser checks

Record R-27, 2026-09-10, parser revision 7, Linux fixture runner.

- Passed: both example records in examples.md parsed with expected fields.
- Passed: duplicate IDs within a file rejected the file with zero records written.
- Passed: fractional quantity rejected with physical line number 2.
- Passed: CRLF and initial UTF-8 BOM fixtures parsed.
- Not run: Windows filesystem integration and production inventory integration; those environments were unavailable.
- Not covered by this run: throughput, memory consumption, and downstream spreadsheet behavior.

This is historical fixture evidence supplied to the document author. It is not a receipt for checks performed while editing the document and does not establish end-to-end production readiness.
