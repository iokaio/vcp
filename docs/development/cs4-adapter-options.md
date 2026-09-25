# CS-4 adapter options and local availability

Status: investigation only, September 24, 2026. Owner:
[CS-4](../plan/24-skills-follow-on.md#cs-4--document-and-data-adapters).
This records candidates for the bounded PDF/XLSX scope; it neither selects a
default dependency nor qualifies a converter, renderer or recalculation engine.
CS-4 still follows the six developer skills. No dependency was installed, no
document was converted, and no live model call ran for this investigation.

## Observed baseline

Targeted searches of VCP crate manifests, its Codex workspace lockfile and skill
tooling found no existing PDF/XLSX adapter dependency. Candidate skill prose is
guidance, not executable conversion support. Host probes used command discovery,
Python distribution metadata, Node module resolution and two standard installation
paths; they did not inspect credentials, private configuration or user documents.

| Component | Observed availability on this Windows host |
|---|---|
| Python | Active interpreter 3.13.7 |
| pypdf | Installed 6.14.2; metadata declares BSD-3-Clause and Python >=3.9. No mandatory dependency applies to this interpreter without extras |
| Other Python candidates | reportlab, openpyxl, xlsxwriter, defusedxml, pdfplumber, pymupdf and pypdfium2 not installed in the active interpreter |
| Node | v24.21.0; pdf-lib, pdfjs-dist, exceljs and xlsx not resolvable from repository working directory |
| Native tools | soffice, pdftotext, pdfinfo and mutool not found on PATH; soffice.exe absent in both standard Program Files LibreOffice locations |

These probes do not establish machine-wide absence: other virtual environments,
portable tools and nonstandard locations were not searched. They establish that
no probed recalculation engine or independent PDF renderer is ready to qualify.

## Candidate comparison

| Candidate | Fit and preservation limits | License/version/distribution decision |
|---|---|---|
| pypdf | First extraction experiment: page-indexed text from bounded, unencrypted text PDFs. No OCR; visual reading order and table structure need separate evidence. Parsing decompressed content can consume substantial memory, so compressed input size alone cannot bound it. Not a layout/render oracle. [Extraction documentation](https://pypdf.readthedocs.io/en/stable/user/extract-text.html) | Existing 6.14.2 is the local experiment baseline, not an approved distributable pin. [BSD-3-Clause license](https://github.com/py-pdf/pypdf/blob/main/LICENSE). Pin exact wheel/source/license hashes before packaging; do not infer installed files match a reviewed wheel |
| ReportLab open-source toolkit | Candidate for new PDF generation from a small structured input schema. Keep arbitrary template code, rich markup, remote resources and original-document edits outside the adapter. Fonts and glyph coverage need explicit fixtures and independently licensed font assets. [Toolkit introduction](https://docs.reportlab.com/reportlab/userguide/ch1_intro/) | Not installed. Maintainer metadata advertises BSD licensing, Python >=3.9 and release 5.0.1; this is a research candidate, not a selected version. Review the exact archive license and dependency/font inventory independently of commercial RML products. [Maintainer package metadata](https://pypi.org/project/reportlab/) |
| pdf-lib | Alternative new-PDF generator in Node. It cannot extract general page text or edit arbitrary existing page text; therefore it cannot replace the extraction adapter. [Project limitations](https://github.com/Hopding/pdf-lib#limitations) | MIT; not locally resolved and no version selected. Would add another dependency graph and font compatibility matrix. Prefer it only if a measured Node distribution advantage outweighs the separate extraction dependency |
| openpyxl plus XML hardening | Candidate for reading, creating and editing a strictly declared XLSX subset. It does not evaluate formulas. Cached-value reads do not prove freshness, and unsupported shapes may disappear during save. Default parser protection needs attention; upstream recommends defusedxml. [Overview/security](https://openpyxl.readthedocs.io/en/stable/), [round-trip caveats](https://openpyxl.readthedocs.io/en/stable/tutorial.html), [formula limits](https://openpyxl.readthedocs.io/en/3.1/simple_formulae.html) | MIT/Expat; neither library installed. No wheel/version selected. Record exact Python/wheel and transitive XML dependency identities before tests. Reject unsupported package features before opening for mutation; a successful save is not preservation evidence |
| XlsxWriter | Alternative for new workbooks only. It cannot read or modify existing workbooks and does not calculate formulas; stored results or recalculate-on-open flags are not a fresh calculation receipt. [FAQ](https://xlsxwriter.readthedocs.io/faq.html) | BSD-2-Clause; not installed, no version selected. Adds a second writer if openpyxl is already required for edits; defer unless generation fixtures show a concrete benefit. [License](https://xlsxwriter.readthedocs.io/license.html) |
| LibreOffice Calc | Candidate recalculation engine, separately qualified from workbook I/O. Headless conversion and a distinct user profile are available, but conversion success does not establish a complete recalculation or Excel-compatible results. Explicitly drive and verify the chosen calculation behavior. [Startup parameters](https://help.libreoffice.org/latest/en-US/text/shared/guide/start_parameters.html), [calculation semantics](https://help.libreoffice.org/latest/en-US/text/scalc/01/06080000.html) | Not located; no version selected. MPL-2.0 application with additional component licenses requiring an exact distribution inventory. Prefer an explicitly pinned external qualification installation before considering default bundling. [Distribution licenses](https://www.libreoffice.org/licenses/) |

PyMuPDF is a possible extraction/render alternative, but it is not installed and
its AGPL/commercial distribution choice needs a separate recorded decision. It
is not a casual replacement dependency. No commercial budget or license is
assumed. [Upstream licensing](https://pymupdf.readthedocs.io/en/latest/about.html#license-and-copyright)

## Proposed first experiment and gates

Start with the existing pypdf version for extraction measurements. Independently
evaluate one new-PDF generator, openpyxl with XML hardening for a minimal workbook
subset, and a pinned Calc installation for recalculation. This is a research
ordering recommendation. It authorizes no automatic installation or default
distribution, and no untested candidate version becomes a support claim.

Before implementation, freeze numerical ceilings in the adapter fixture contract.
A proposed first envelope is 16 MiB input, 100 PDF pages, 10 workbook sheets,
100,000 populated cells, 1,000 ZIP members, 64 MiB total expanded XLSX data,
8 MiB extracted text, 64 MiB generated output, 256 MiB owned temporary storage,
512 MiB worker memory and a 30-second wall deadline. These are unmeasured
proposals, not enforced limits. Bound decompression while it happens; checking
expanded sizes after allocation is insufficient. Lower limits if actual Windows
fixtures cannot satisfy them reliably.

Keep parsers/converters behind existing process and path authority. Use immutable
input copies and new owned output directories; validate originals before and
after, and publish only after successful verification. Independent observations
must cover spaces, Unicode, long paths, locked files, pause/kill, owner loss,
temporary-file bounds and cleanup without touching unrelated processes. A Calc
profile must be isolated from the user's running office session. Deny network,
macros and external-link refresh through actual configuration/isolation evidence;
headless mode by itself is not that evidence.

PDF fixtures need page provenance, Unicode/fonts, known text and structural
checks plus independent rendered-page inspection for generation. Distinguish
scanned/no-text, encrypted, malformed and oversized inputs. Defer OCR, signatures,
forms, arbitrary original edits and secure redaction.

XLSX fixtures need types, units, 1900/1904 date systems, cell/range provenance,
formula text and reference preservation, stale-cache diagnostics, independent
expected values after recalculation, untouched-sheet semantic comparisons and
original-file hashes. Whole ZIP byte equality is not a useful substitute for
checking untouched workbook content after a serializer rewrite. Reject macros,
external links, pivots, connections, embedded objects and unqualified charts;
check formula injection at CSV export. A fresh value label requires an exact
engine run receipt and its verified output, not a cached value or a library flag.

Before any default dependency, record exact version/artifact hashes, original
licenses/notices and transitive assets, runtime/architecture support, reproducible
installation and rollback, and responsibility for reviewing upstream security
changes. Re-run the affected hostile and preservation corpus on upgrades. Actual
Windows conversion, rendering, recalculation and lifecycle qualification all
remain **not_run**.
