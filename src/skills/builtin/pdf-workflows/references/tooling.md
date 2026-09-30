# PDF helper setup and scope

VCP implementation: Apache-2.0. Dependencies are installed in the selected Python
environment, not bundled or automatically installed by activation:

| Dependency | Tested version | License | Purpose |
|---|---|---|---|
| [pypdf](https://pypi.org/project/pypdf/6.19.0/) | 6.19.0 | BSD-3-Clause | Text extraction, inspection, merge, split and rotation |
| [ReportLab](https://pypi.org/project/reportlab/5.0.1/) | 5.0.1 | BSD-3-Clause | Text PDF generation |

`requirements.txt` is a `file` resource: when dependency installation is
authorized, materialize it with `vcp_skill` first, then run `python -m pip install
-r` on the copy in the project's environment. It pins exact versions without
hashes; use `--require-hashes` only with a reviewed copy that adds them. ReportLab
also depends on Pillow (MIT-CMU) and charset-normalizer (MIT). Review any changed
dependency set before redistribution. No code or prose from Anthropic's
restrictively licensed `pdf` skill was copied.

ReportLab's wheel also contains decorative DarkGarden fonts under GPL, which this
skill neither uses nor redistributes. Do not copy those assets into a VCP package.
Unicode fixtures use its separately licensed Bitstream Vera font; production
Unicode output uses the caller's authorized TTF. No library or font is vendored.

Python 3.10 or newer is required. Inputs and outputs stay within the root supplied
by the host (`--root .` for the process's working directory), and paths are
relative to it. Symbolic links in a path are refused. `Path.is_junction` exists
from Python 3.12, so junctions are refused there; before 3.12 a junction is
followed, and only the check that the resolved path stays inside the root applies.
Output parents must already exist ("Output parent directory does not exist"); the
output path is checked before inputs are read, and creation is exclusive. Input
PDF and output files are limited to 20 MiB (merge inputs to 20 MiB combined and
2..20 files; exceeding the total names the combined limit), PDFs to 200 pages
including merged output, extracted text to one million characters, source text to
100,000 characters and TTF fonts to 8 MiB. pypdf's decoders are configured to stop
any single stream at 20 MiB while decoding, and the external `jbig2dec` program is
disabled.

PDFs are read non-strictly so common defects, such as a wrong cross-reference
offset, are repaired. `parser_warnings` reports only the number of recovery
warnings and their pypdf module or warning class, never warning text, which can
quote document-controlled names. Unrecoverable files fail with a generic message
that does not echo document content. The page tree is walked after decryption;
if it disagrees with the declared `/Count`, the file is refused, and page limits
apply to the walked count.

An encrypted PDF is opened only when its user password is empty. Extraction then
requires the document's extract permission. Merge, split and rotate write
unencrypted output, so they require every standard permission; otherwise they
would drop the owner's restrictions. An empty owner password grants all
permissions. Documents needing a password, and AES-encrypted documents when
pypdf's optional cryptography provider is absent, are refused.

`info` reports the header version, page sizes and rotation, standard metadata
strings (control characters removed, 1,000 characters each) and permissions. It
prints at most 8,192 bytes of JSON on stdout; a larger result fails without
output and needs `--output`. `split` keeps the requested page order and refuses
repeated pages; `extract` and `rotate` sort and de-duplicate their selections. Merge
and split copy page objects only; outlines, form fields, named destinations and
document metadata are dropped. Rotate clones the document and changes `/Rotate`.

A no-text page may be scanned, blank or use unsupported encoding; the helper does
not diagnose which or run OCR. The parser and font library still need the host's
memory, time and filesystem restrictions for hostile documents. A root argument
is not an operating-system sandbox. No shell command, URL, attachment or
JavaScript from a document is run.

`create` reads UTF-8 text, with or without a BOM. It defaults to ASCII Helvetica
on US Letter with 54-point margins, 11-point text and the title "Text document";
`--page-size a4`, `--margin` 18..144, `--font-size` 6..36 and `--title` (1..200
printable characters) adjust this. Unicode generation requires an authorized TTF
whose glyph map covers the content. Complex-script shaping, bidirectional text,
tables, embedded images, tagged-PDF accessibility, archival conformance and
visual fidelity are not advertised. The helper does not render or visually review.
