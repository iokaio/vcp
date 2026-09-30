# Spreadsheet helper contract

VCP helper code is Apache-2.0. It uses [openpyxl 3.1.5](https://pypi.org/project/openpyxl/3.1.5/)
(MIT) and [defusedxml 0.7.1](https://pypi.org/project/defusedxml/0.7.1/) (PSF-2.0).
[Formualizer 0.10.0](https://pypi.org/project/formualizer/0.10.0/) (MIT, with
Apache-2.0 also offered in its wheel) performs explicit formula recalculation.
openpyxl's dependency et-xmlfile is MIT. None is bundled by the skill.
`requirements.txt` is a `file` resource: when setup is authorized, materialize it
with `vcp_skill`, then run `python -m pip install -r` on the copy in the project's
Python environment. It pins exact versions without hashes; use `--require-hashes`
only with a reviewed copy that adds them. Python 3.10 or newer is required.
No code or prose from Anthropic's restrictively licensed `xlsx` skill was copied.

## Commands

Every command writes a new file and prints a JSON summary. Paths are relative to
`--root` (`--root .` for the process's working directory). The output is checked
before inputs are read: it must not exist, and its parent directory must ("Output
parent directory does not exist"). Symbolic links are refused; Windows junctions
are refused on Python 3.12 or newer, and on older versions only when they resolve
outside the root. The summaries of `sheets`, `create`, `edit` and `csv-import`
list sheet names, which come from the workbook or specification and are data.
`--range` takes an unqualified uppercase A1 cell or range (`B2`, `A1:D20`); the
sheet is chosen with `--sheet`. `sheets` writes each sheet's index, name,
visibility, used range, maximum row/column and formula count. `inspect` writes
typed cells for one bounded range. `create`, `edit` and `csv-import` write
workbooks; `recalculate` writes fresh caches; `csv-export` writes UTF-8 CSV with
CRLF line endings and no BOM.

Both creation and edit specifications use this format:

```json
{
  "sheets": [{
    "name": "Data",
    "cells": {
      "A1": {"type": "string", "value": "Revenue"},
      "B2": {"type": "number", "value": 12.5, "number_format": "0.00"},
      "B3": {"type": "number", "value": 7.5},
      "B4": {"type": "formula", "value": "=SUM(B2:B3)"},
      "C2": {"type": "date", "value": "2026-09-29"},
      "D2": {"type": "boolean", "value": true},
      "E2": {"type": "blank", "value": null}
    }
  }]
}
```

Types are `string`, `number`, `boolean`, `date`, `datetime`, `formula` and
`blank`. Dates/datetimes use ISO strings; normalize timezone-aware times
explicitly before writing. Sheet names have 1..31 characters, no `\ / * ? : [ ]`
or control characters, no leading or trailing apostrophe, are not `History`
(reserved by Excel, in any case) and are unique ignoring case. Every writer
(`create`, `csv-import` and `edit`) refuses text containing `_xHHHH_` sequences
such as `_x0041_`: Excel decodes them when reading, while openpyxl neither escapes
nor decodes them, so escaping them would make the helper's own reopen checks and
Excel disagree. More than 15-digit integer identifiers must be strings. A creation
spec defines the complete sheet list; an edit spec may name only existing sheets
and leaves unspecified cells in place. Errors name the file with JSON line/column
or the `Sheet!A1` cell.

`edit` changes the workbook package directly instead of re-saving it with openpyxl.
It rewrites only edited worksheets, worksheets whose formula caches it invalidates,
`styles.xml` when a new number format is needed and `workbook.xml` calcPr
(`fullCalcOnLoad`). If an edit replaces a formula, `calcChain.xml` and its
references are removed, because Excel reports a chain listing a non-formula cell
as corruption. The summary lists changed and removed parts; every other part is
copied byte-for-byte. Text is written as inline strings; new cells take the default
style unless a number format is given; dates in cells without a date format get
`yyyy-mm-dd` (`yyyy-mm-dd h:mm:ss` for datetimes). Edits are
refused for table header/totals rows, non-anchor merged cells, shared or array
formula anchors, and package XML that is prefixed or not UTF-8.

## CSV

`csv-import` reads UTF-8 (optional BOM) with Python's `csv` module, at most 10,000
records and 256 fields per record. `--types` lists column types by position
(`string`, `number`, `boolean`, `date`, `datetime`, `formula`); empty or missing
entries are untyped text. `--header` imports the first record as explicit text,
so headers such as `+/-` or `-delta` are kept. Empty fields
become blank cells; CSV cannot express an empty string. Untyped text starting with
`=`, `+`, `-`, `@`, tab or carriage return is refused; type the column `string` to
keep it as text or `formula` to store a validated formula. Numbers must be plain
decimals (`-3`, `12.50`, `1e3`); separators, currency and percent signs are refused.
Booleans are `true`/`false` in any case. Values are then validated like JSON cells.

`csv-export` writes one sheet, from A1 to its maximum row and column or a `--range` of at
most 100,000 cells. Formula cells export their cached values, reported as
`cached_unverified`; a formula without a cache is refused, so recalculate first.
Numbers use exact round-trip text (`0.1`, `0.30000000000000004`, `10`), booleans
`TRUE`/`FALSE` and date-time values ISO 8601 (date only at midnight); number formats
are not applied. Text beginning with a formula character is prefixed with `'`, and
those cells are listed in the summary.

## Formulas and recalculation

Supported formulas use local A1 references, arithmetic, comparison, `&` and SUM,
AVERAGE, MIN, MAX, COUNT, COUNTA, IF, ROUND, ABS, SUMIF, SUMIFS, COUNTIF, COUNTIFS,
AVERAGEIF, AVERAGEIFS, IFERROR, AND, OR, NOT, INDEX, MATCH, VLOOKUP, XLOOKUP,
ROUNDUP and ROUNDDOWN. Tests compare each added function's Formualizer 0.10.0
results with known Excel values, including wildcards, case-insensitive matching,
approximate lookup and negative rounding. XLOOKUP is stored as `_xlfn.XLOOKUP`,
as Excel requires, and needs Excel 2021 or Microsoft 365. Named ranges, structured
references, external references and other functions need another tool.

Excluded after testing: DATE (the engine returns 61 for Excel's serial-60
1900-02-29 and a serial instead of #NUM! for year 10000), LEN and CONCAT (numbers
become shortest round-trip text, so `CONCAT(0.1+0.2)` gives `0.30000000000000004`
instead of Excel's `0.3`). The `&` operator has the same number-to-text deviation;
use it with text or integers only.

`recalculate` uses Formualizer's cache-only XLSX API and requires an error-free
result. It withholds only Excel's `x15:workbookPr` chart-tracking extension, which
0.10.0 refuses, from the engine and restores the original `workbook.xml`. It
reopens output to check formula text and caches and verifies that all
non-worksheet parts are unchanged. Workbooks with table parts are refused before
the engine runs; formulas that spill are refused without output. Creation/editing
invalidate formula caches; `fullCalcOnLoad` alone is not evidence of
recalculation. An arbitrary input inspected later always has unknown cache
freshness; retain the successful recalculation result for the exact output when
making a freshness claim. The engine evaluates ordinary spreadsheet numbers, not
exact financial decimals.

Known Formualizer 0.9.3 and 0.10.0 limitation: loading literal empty-string cells
loses the distinction from blank cells (COUNTA undercounts; ISBLANK is TRUE).
Recalculation rejects any workbook containing such cells before invoking the engine.
Create/edit/inspect preserve literal empty strings. Formula-produced empty-string
results retain their string caches and are supported, including COUNTA of them.

## Limits and rejected content

Inputs/outputs are bounded to 20 MiB. XLSX archives allow at most 256 parts, 32
MiB expanded content and 8 MiB per part. Parts are decompressed with bounded
reads, must match their declared sizes and, above 1 MiB, compress at most 250:1;
openpyxl and Formualizer receive only a repacked copy. Accepted parts are the
workbook, worksheets, shared strings, styles, themes, core/app/custom document
properties, `calcChain.xml`, tables, printer settings and `docProps/thumbnail`
images (opaque, never parsed), custom XML items and their relationships, and
comments: legacy `commentsN.xml`, threaded comments, `persons/person.xml` and the
comment VML drawing. VML is often not well-formed XML, so it is kept opaque and
accepted only when every shape is a comment note; VML with form controls,
formulas, images or OLE objects is refused. These parts are copied byte-for-byte
by `edit` and must be unchanged after `recalculate`. `xl/metadata.xml` is refused:
Excel writes it for dynamic-array formulas and linked data types, whose array
formulas and rich-value parts are outside this subset, so admitting the part alone
would not make those workbooks editable. `extLst` is accepted only in styles and
workbook parts. Defined names are accepted only as `_xlnm.Print_Area`,
`_xlnm.Print_Titles` and `_xlnm._FilterDatabase` whose values are local sheet, row
or column references. Conditional-format, data-validation and table formulas must
pass the cell formula subset. Error messages give cell references or part names,
never document text. Macros, macro-enabled content types, external links or
relationships, OLE objects, controls, drawings other than comment VML, charts,
pivots, connections and any other part are rejected. XML entity expansion is
rejected. Workbooks allow 32 sheets, 10,000 rows, 256 columns and 100,000
rectangular cells summed across sheets. Inspection is limited to 10,000 cells per
range. These limits bound ordinary work; retain the host's memory/time limits for
hostile input. The root argument does not replace an operating-system sandbox.

Excel compatibility is tested with a synthetic workbook carrying the parts and
markup Excel writes (calcChain, custom properties, printer settings, custom XML, a
table, style and workbook extensions, reserved names, `mc:Ignorable` attributes,
comments with VML, threaded comments and a thumbnail).
No file saved by Microsoft Excel has been tested. This subset does not promise
exact round-trip fidelity for arbitrary Excel features, active content, complex
layout, all Excel formula semantics, financial correctness or visual inspection.
