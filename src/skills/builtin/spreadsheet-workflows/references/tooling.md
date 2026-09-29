# Spreadsheet helper contract

VCP helper code is Apache-2.0. It uses [openpyxl 3.1.5](https://pypi.org/project/openpyxl/3.1.5/)
(MIT) and [defusedxml 0.7.1](https://pypi.org/project/defusedxml/0.7.1/) (PSF-2.0).
[Formualizer 0.9.3](https://pypi.org/project/formualizer/0.9.3/) (MIT, with
Apache-2.0 also offered in its wheel) performs explicit formula recalculation.
openpyxl's dependency et-xmlfile is MIT. None is bundled by the skill. When setup
is authorized, use the project's Python environment and
`python -m pip install -r requirements.txt`. Python 3.10 or newer is required.
No code or prose from Anthropic's restrictively licensed `xlsx` skill was copied.

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

Types are `string`, `number`, `boolean`, `date`, `datetime`, `formula` and `blank`.
Dates/datetimes use ISO strings; normalize timezone-aware times explicitly before
writing. More than 15-digit integer identifiers must be strings. A creation spec
defines the complete sheet list; an edit spec may name only existing sheets and
leaves unspecified cells in place. Both outputs must be new files.

Supported formulas use local A1 references, arithmetic and SUM, AVERAGE, MIN, MAX,
COUNT, COUNTA, IF, ROUND or ABS. The `recalculate` command evaluates this subset
using Formualizer's cache-only XLSX API and requires an error-free result. It
reopens output to check formula text and caches, and verifies that non-worksheet
ZIP members are unchanged. Named ranges, external
references and additional functions need another tool. Creation/editing invalidate
formula caches; `fullCalcOnLoad` alone is not evidence of recalculation. An arbitrary
input inspected later always has unknown cache freshness; retain the successful
recalculation result for the exact generated output when making a freshness claim.
The engine evaluates ordinary spreadsheet numbers, not exact financial decimals.

Known Formualizer 0.9.3 limitation: loading literal empty-string cells loses the
distinction from blank cells, causing COUNTA to undercount. Recalculation rejects
any workbook containing such cells before invoking the engine or writing output.
Create/edit/inspect preserve literal empty strings. Formula-produced empty-string
results retain their string caches and are supported, including COUNTA of them.

Inputs/outputs are bounded to 20 MiB. XLSX archives allow at most 256 known basic
parts, 32 MiB expanded content and 8 MiB per XML part. Workbooks allow 32 sheets,
10,000 rows, 256 columns and 100,000 rectangular cells summed across sheets.
Inspection is limited to 10,000 cells per range. Unsupported parts/extensions,
defined names, macros, external relationships, comments, drawings, charts, pivots,
connections and embedded objects are rejected. XML entity expansion is rejected.
These limits bound ordinary work; retain the host's memory/time limits for hostile
input. The root argument does not replace an operating-system sandbox.

Basic values, dates, number formats, styles and supported formulas are retained by
openpyxl. The original is never overwritten. This subset does not promise exact
round-trip fidelity for arbitrary Excel features, active content, complex layout,
all Excel formula semantics, financial correctness or visual inspection.
