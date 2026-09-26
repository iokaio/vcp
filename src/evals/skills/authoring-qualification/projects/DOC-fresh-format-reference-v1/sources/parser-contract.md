# Reed CSV parser contract, revision 7

Status: current implementation contract, accepted 2026-09-09.

Files use UTF-8, comma delimiters, and a header row. LF and CRLF are accepted. A UTF-8 byte-order mark is permitted only at the start of the file. Headers must be exactly `item_id,label,quantity,location` in that order; additional or missing columns reject the file.

`item_id` is case-sensitive and must match `[A-Z][A-Z0-9-]{2,15}`. It must be unique within this file; the parser does not check existing inventory. `label` is required after trimming outer ASCII spaces, is at most 80 Unicode scalar values, and may contain a comma when CSV-quoted. Double quotes inside quoted fields are escaped by doubling them. `quantity` is a decimal integer from 0 through 9999, inclusive; negative numbers and fractional values are rejected. `location` is either blank or one of `north`, `south`, `reserve`, in lowercase. Blank location means unassigned; it does not mean north.

Blank physical lines outside a quoted field are ignored. Newlines inside quoted fields are not supported. No field is evaluated as a formula or command by this parser; downstream spreadsheet handling is outside this contract.

Validation is all-or-nothing. Any invalid row rejects the entire file and writes no inventory records. Diagnostics identify the one-based physical line containing the start of the invalid record. Successful parsing returns proposed records for review; it does not commit them to production.

Deferred proposal: accepting semicolon-delimited files. No implementation or release date has been approved.
