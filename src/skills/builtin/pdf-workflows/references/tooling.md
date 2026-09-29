# PDF helper setup and scope

VCP implementation: Apache-2.0. Dependencies are installed in the selected Python
environment, not bundled or automatically installed by activation:

| Dependency | Tested version | License | Purpose |
|---|---|---|---|
| [pypdf](https://pypi.org/project/pypdf/6.14.2/) | 6.14.2 | BSD-3-Clause | Page text extraction |
| [ReportLab](https://pypi.org/project/reportlab/5.0.1/) | 5.0.1 | BSD-3-Clause | Text PDF generation |

When dependency installation is authorized, use the project's environment and
`python -m pip install -r requirements.txt`. ReportLab also depends on Pillow
(MIT-CMU) and charset-normalizer (MIT). Review any changed dependency set before
redistribution. The helper was developed against these libraries' public APIs;
no code or prose from Anthropic's restrictively licensed `pdf` skill was copied.

ReportLab's wheel also contains decorative DarkGarden fonts under GPL, which this
skill neither uses nor redistributes. Do not copy those assets into a VCP package.
Unicode fixtures use its separately licensed Bitstream Vera font; production
Unicode output uses the caller's authorized TTF. No library or font is vendored.

Python 3.10 or newer is required. Inputs and outputs stay within the root supplied
by the host. Parents must already exist; output creation is exclusive. Input PDF
and output files are limited to 20 MiB, PDFs to 200 pages, extracted text to one
million characters, source text to 100,000 characters and TTF fonts to 8 MiB.
Malformed and encrypted PDFs fail explicitly. A no-text page may be scanned,
blank or use unsupported encoding; the helper does not diagnose which or run OCR.

The parser and font library still need the host's memory, time and filesystem
restrictions for hostile documents. A root argument is not an operating-system
sandbox. No shell command, URL, attachment or JavaScript from a document is run.

Default output uses ASCII Helvetica on US Letter pages with 54-point margins and
11-point text. Unicode generation requires an authorized TTF whose glyph map
covers the content. Complex-script shaping, bidirectional text, tables, embedded
images, tagged-PDF accessibility, archival conformance and visual fidelity to an
existing PDF are not advertised. The helper does not render or visually review.
