# SPDX-License-Identifier: Apache-2.0
"""Local, bounded PDF text extraction, text-PDF generation and page assembly. No OCR."""
import argparse
import contextlib
import hashlib
import io
import json
import logging
from pathlib import Path
import re
import sys
import warnings

MAX_INPUT = 20 * 1024 * 1024
MAX_PAGES = 200
MAX_TEXT = 100_000
MAX_MERGE_INPUTS = 20
# pypdf decoders stop once a single stream would exceed this, instead of decoding it fully first.
DECODE_LIMITS = ("maximum_declared_stream_length", "array_based_stream_maximum_output_length",
                 "jbig2_maximum_output_length", "lzw_maximum_output_length", "run_length_maximum_output_length",
                 "zlib_maximum_output_length", "image_maximum_buffer_size")
PAGE_SIZES = {"letter": (612.0, 792.0), "a4": (595.2756, 841.8898)}


class WorkflowError(ValueError):
    pass


class PasswordRequired(WorkflowError):
    pass


class PermissionRestricted(WorkflowError):
    pass


def local_path(root, name, existing=True):
    relative = Path(name)
    if relative.is_absolute() or not relative.parts or ".." in relative.parts:
        raise WorkflowError("Use a relative path within --root")
    current = root
    for part in relative.parts:
        if any(ord(char) < 32 or char in '<>:"|?*' for char in part) or part.endswith((".", " ")) or re.match(r"^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)", part, re.I):
            raise WorkflowError("Use portable ordinary file names")
        current = current / part
        if current.is_symlink() or (hasattr(current, "is_junction") and current.is_junction()):
            raise WorkflowError("Linked paths are outside the supported file boundary")
    resolved = current.resolve(strict=existing)
    if not resolved.is_relative_to(root) or not resolved.parent.is_dir():
        raise WorkflowError("Path must stay inside the existing workspace")
    if existing and not resolved.is_file():
        raise WorkflowError("Input must be an ordinary file")
    return resolved


def read_input(root, name, maximum=MAX_INPUT):
    path = local_path(root, name)
    with path.open("rb") as stream:
        data = stream.read(maximum + 1)
    if len(data) > maximum:
        raise WorkflowError("Input exceeds the supported byte limit")
    return data


def write_new(root, name, data):
    path = local_path(root, name, existing=False)
    if len(data) > MAX_INPUT:
        raise WorkflowError("Output exceeds the supported byte limit")
    # Exclusive creation preserves originals, including an output created meanwhile.
    with path.open("xb") as stream:
        stream.write(data)


def write_json(root, name, value):
    write_new(root, name, (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))


def selected_pages(value, count):
    if value is None:
        return list(range(1, count + 1))
    pages = set()
    for part in value.split(","):
        ends = part.split("-")
        if len(ends) not in (1, 2) or not all(end.isascii() and end.isdecimal() for end in ends):
            raise WorkflowError("Pages must be one-based numbers or inclusive ranges")
        first, last = int(ends[0]), int(ends[-1])
        if not 1 <= first <= last <= count:
            raise WorkflowError("Requested page is outside the document")
        pages.update(range(first, last + 1))
    return sorted(pages)


def printable(value, limit):
    # Document-derived strings are data; drop control characters and bound their size.
    return "".join(char for char in str(value) if char.isprintable())[:limit]


class Diagnostics(logging.Handler):
    """Counts pypdf's recovery warnings from non-strict parsing for the JSON result.

    Warning texts can quote document-controlled names, so only the count and the
    code-derived source (pypdf module or warning class) are reported.
    """

    def __init__(self):
        super().__init__(logging.WARNING)
        self.summary = {"count": 0, "sources": []}

    def emit(self, record):
        self.add(record.name)

    def add(self, source):
        self.summary["count"] += 1
        source = source if re.fullmatch(r"[A-Za-z_][\w.]{0,63}", source) else "other"
        if source not in self.summary["sources"]:
            self.summary["sources"].append(source)


@contextlib.contextmanager
def pdf_parsing():
    import pypdf
    from pypdf.errors import DependencyError, LimitReachedError

    diagnostics = Diagnostics()
    logger = logging.getLogger("pypdf")
    logger.addHandler(diagnostics)
    propagate, logger.propagate = logger.propagate, False
    try:
        with warnings.catch_warnings(), pypdf.apply_configuration(jbig2dec_binary=None, **dict.fromkeys(DECODE_LIMITS, MAX_INPUT)):
            warnings.simplefilter("always")
            warnings.showwarning = lambda _message, category, *_args, **_kwargs: diagnostics.add(category.__name__)
            yield diagnostics
    except LimitReachedError:
        raise WorkflowError("A decoded PDF stream exceeds the supported byte limit") from None
    except DependencyError:
        raise WorkflowError("This PDF needs an optional pypdf dependency, such as an AES provider, that is not installed") from None
    finally:
        logger.removeHandler(diagnostics)
        logger.propagate = propagate


def open_pdf(root, name, purpose=None, maximum=MAX_INPUT):
    from pypdf import PasswordType, PdfReader
    from pypdf.constants import UserAccessPermissions

    source = read_input(root, name, maximum)
    # Non-strict reading recovers common malformations; recoveries are reported as parser warnings.
    reader = PdfReader(io.BytesIO(source), strict=False)
    encryption = "none"
    if reader.is_encrypted:
        matched = reader.decrypt("")
        if matched == PasswordType.NOT_DECRYPTED:
            raise PasswordRequired("PDF requires a password; use an authorized decrypted copy")
        encryption = "empty_user_password"
        allowed = reader.user_access_permissions
        if reader.are_permissions_valid is False:
            allowed = UserAccessPermissions(0)
        every = UserAccessPermissions(0)
        for flag in ("PRINT", "MODIFY", "EXTRACT", "ADD_OR_MODIFY", "FILL_FORM_FIELDS",
                     "EXTRACT_TEXT_AND_GRAPHICS", "ASSEMBLE_DOC", "PRINT_TO_REPRESENTATION"):
            every |= UserAccessPermissions[flag]
        required = {"extract": UserAccessPermissions.EXTRACT, "assemble": every}.get(purpose)
        # New PDFs are written unencrypted, so assembly needs every permission; otherwise the
        # output would drop restrictions the owner set. An empty owner password grants all.
        if required and matched != PasswordType.OWNER_PASSWORD and allowed & required != required:
            raise PermissionRestricted("The PDF's access permissions do not allow this operation")
    # pypdf trusts /Count for encrypted files; count the page tree itself and fail closed on disagreement.
    try:
        reader.get_page(0)
    except IndexError:
        pass
    actual = len(reader.flattened_pages or [])
    declared = reader.root_object["/Pages"].get("/Count")
    if not isinstance(declared, int) or declared != actual or len(reader.pages) != actual:
        raise WorkflowError("PDF page tree does not match its declared page count")
    if not 1 <= actual <= MAX_PAGES:
        raise WorkflowError("PDF must contain 1..200 pages")
    return reader, source, encryption


def pdf_bytes(writer):
    output = io.BytesIO()
    writer.write(output)
    return output.getvalue()


def extract(root, args, diagnostics):
    reader, source, encryption = open_pdf(root, args.input, "extract")
    pages = []
    total = 0
    for number in selected_pages(args.pages, len(reader.pages)):
        page = reader.pages[number - 1]
        # The host still owns process memory/time limits for parsing untrusted files.
        contents = page.get_contents()
        if contents is not None and len(contents.get_data()) > MAX_INPUT:
            raise WorkflowError("A decoded page exceeds the supported byte limit")
        content = page.extract_text() or ""
        total += len(content)
        if total > 1_000_000:
            raise WorkflowError("Extracted text exceeds one million characters")
        pages.append({"page": number, "status": "text" if content.strip() else "no_extractable_text", "text": content})
    empty = sum(page["status"] != "text" for page in pages)
    status = "ok" if not empty else "no_extractable_text" if empty == len(pages) else "partial"
    result = {"status": status, "source": args.input, "source_sha256": hashlib.sha256(source).hexdigest(),
              "encryption": encryption, "page_count": len(reader.pages), "pages": pages, "ocr": "not_run",
              "parser_warnings": diagnostics.summary}
    write_json(root, args.output, result)
    return {"status": status, "output": args.output, "pages": len(pages), "ocr": "not_run",
            "parser_warnings": diagnostics.summary}


def info(root, args, diagnostics):
    from pypdf.constants import UserAccessPermissions

    reader, source, encryption = open_pdf(root, args.input)
    metadata = {}
    for key, value in list((reader.metadata or {}).items())[:50]:
        value = value.get_object() if hasattr(value, "get_object") else value
        if isinstance(key, str) and isinstance(value, str):
            metadata[printable(key, 64)] = printable(value, 1000)
    permissions = None
    if encryption != "none":
        allowed = reader.user_access_permissions or UserAccessPermissions(0)
        permissions = [flag.name.lower() for flag in UserAccessPermissions if not flag.name.startswith("R") and allowed & flag]
    pages = []
    for number, page in enumerate(reader.pages, 1):
        box = page.mediabox
        pages.append({"page": number, "width": round(float(box.width), 2), "height": round(float(box.height), 2),
                      "rotation": int(page.rotation) % 360})
    result = {"status": "ok", "source": args.input, "source_sha256": hashlib.sha256(source).hexdigest(),
              "pdf_header": printable(reader.pdf_header, 16), "page_count": len(reader.pages),
              "encryption": encryption, "permissions": permissions, "metadata": metadata, "pages": pages,
              "parser_warnings": diagnostics.summary}
    if args.output is None:
        return result
    write_json(root, args.output, result)
    return {"status": "ok", "output": args.output, "page_count": len(reader.pages), "encryption": encryption,
            "parser_warnings": diagnostics.summary}


def merge(root, args, diagnostics):
    from pypdf import PdfWriter

    if not 2 <= len(args.input) <= MAX_MERGE_INPUTS:
        raise WorkflowError("Merge takes 2..20 --input files")
    writer = PdfWriter()
    remaining = MAX_INPUT
    sources = []
    for name in args.input:
        reader, source, encryption = open_pdf(root, name, "assemble", remaining)
        remaining -= len(source)
        if len(writer.pages) + len(reader.pages) > MAX_PAGES:
            raise WorkflowError("Merged document exceeds 200 pages")
        for page in reader.pages:
            writer.add_page(page)
        sources.append({"source": name, "source_sha256": hashlib.sha256(source).hexdigest(),
                        "pages": len(reader.pages), "encryption": encryption})
    write_new(root, args.output, pdf_bytes(writer))
    return {"status": "ok", "output": args.output, "pages": len(writer.pages), "sources": sources,
            "parser_warnings": diagnostics.summary}


def split(root, args, diagnostics):
    from pypdf import PdfWriter

    reader, source, encryption = open_pdf(root, args.input, "assemble")
    pages = selected_pages(args.pages, len(reader.pages))
    writer = PdfWriter()
    for number in pages:
        writer.add_page(reader.pages[number - 1])
    write_new(root, args.output, pdf_bytes(writer))
    return {"status": "ok", "output": args.output, "source_sha256": hashlib.sha256(source).hexdigest(),
            "encryption": encryption, "pages": pages, "parser_warnings": diagnostics.summary}


def rotate(root, args, diagnostics):
    from pypdf import PdfWriter

    reader, source, encryption = open_pdf(root, args.input, "assemble")
    pages = selected_pages(args.pages, len(reader.pages))
    # Cloning keeps document structure; only the selected pages' /Rotate values change.
    writer = PdfWriter(clone_from=reader)
    for number in pages:
        page = writer.pages[number - 1]
        page.rotation = int(page.rotation) + args.degrees
    write_new(root, args.output, pdf_bytes(writer))
    return {"status": "ok", "output": args.output, "source_sha256": hashlib.sha256(source).hexdigest(),
            "encryption": encryption, "rotated_pages": pages, "degrees": args.degrees,
            "parser_warnings": diagnostics.summary}


def wrapped_lines(text, font_name, font_size, width, metrics):
    for paragraph in text.split("\n"):
        line = ""
        for character in paragraph.expandtabs(4):
            if metrics.stringWidth(character, font_name, font_size) > width:
                raise WorkflowError("A font glyph is wider than the available page width")
            while line and metrics.stringWidth(line + character, font_name, font_size) > width:
                boundary = line.rfind(" ")
                if boundary > 0:
                    yield line[:boundary]
                    line = line[boundary + 1:]
                else:
                    yield line
                    line = ""
            line += character
        yield line


def font_covers(font, content):
    # ReportLab 5.0.1 has no public glyph-coverage query. TTFontFile.extractInfo documents
    # face.charWidths as the map of supported code points; fail closed if that internal changes.
    widths = getattr(getattr(font, "face", None), "charWidths", None)
    if not isinstance(widths, dict):
        raise WorkflowError("Installed ReportLab does not expose font coverage; Unicode generation is unavailable")
    return all(ord(char) in widths for char in content if char not in "\n\t")


def create(root, args):
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.pdfgen import canvas

    if not args.title.strip() or len(args.title) > 200 or not args.title.isprintable():
        raise WorkflowError("Title must contain 1..200 printable characters")
    if not 18 <= args.margin <= 144:
        raise WorkflowError("Margin must be 18..144 points")
    if not 6 <= args.font_size <= 36:
        raise WorkflowError("Font size must be 6..36 points")
    source = read_input(root, args.input, 4 * MAX_TEXT)
    try:
        content = source.decode("utf-8-sig").replace("\r\n", "\n")
    except UnicodeDecodeError:
        raise WorkflowError("Text input must be UTF-8; convert it before creating a PDF") from None
    if not content.strip() or len(content) > MAX_TEXT:
        raise WorkflowError("Text must contain 1..100000 characters")
    if any(ord(char) < 32 and char not in "\n\t" for char in content):
        raise WorkflowError("Text contains an unsupported control character")
    font_name = "Helvetica"
    if args.font:
        font_bytes = read_input(root, args.font, 8 * 1024 * 1024)
        font_name = "VcpDocumentFont"
        font = TTFont(font_name, io.BytesIO(font_bytes))
        pdfmetrics.registerFont(font)
        if not font_covers(font, content):
            raise WorkflowError("Selected font does not cover every input character")
    elif any(ord(char) > 126 for char in content):
        raise WorkflowError("Non-ASCII text requires --font with an authorized TrueType font")
    width, height = PAGE_SIZES[args.page_size]
    margin, size = args.margin, args.font_size
    leading = size * 15 / 11
    output = io.BytesIO()
    document = canvas.Canvas(output, pagesize=(width, height), invariant=1)
    document.setTitle(args.title)
    pages, y = 1, height - margin
    document.setFont(font_name, size)
    for line in wrapped_lines(content, font_name, size, width - 2 * margin, pdfmetrics):
        if y < margin:
            document.showPage()
            pages += 1
            if pages > MAX_PAGES:
                raise WorkflowError("Generated document exceeds 200 pages")
            document.setFont(font_name, size)
            y = height - margin
        document.drawString(margin, y, line)
        y -= leading
    document.save()
    write_new(root, args.output, output.getvalue())
    return {"status": "ok", "output": args.output, "pages": pages, "visual_review": "not_run"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, help="Authorized workspace root")
    subparsers = parser.add_subparsers(dest="command", required=True)
    new_file = "New file; existing files are never replaced"
    command = subparsers.add_parser("extract", help="Page-referenced text to a new JSON file")
    command.add_argument("--input", required=True)
    command.add_argument("--output", required=True, help=new_file)
    command.add_argument("--pages", help="One-based pages, e.g. 1,3-5")
    command = subparsers.add_parser("info", help="Page count, sizes, metadata and encryption status")
    command.add_argument("--input", required=True)
    command.add_argument("--output", help=f"{new_file}; omit to print the JSON")
    command = subparsers.add_parser("merge", help="Concatenate PDFs into a new PDF")
    command.add_argument("--input", required=True, action="append", help="Repeat in output order")
    command.add_argument("--output", required=True, help=new_file)
    command = subparsers.add_parser("split", help="Copy selected pages into a new PDF")
    command.add_argument("--input", required=True)
    command.add_argument("--output", required=True, help=new_file)
    command.add_argument("--pages", required=True, help="One-based pages, e.g. 1,3-5")
    command = subparsers.add_parser("rotate", help="Rotate selected pages clockwise in a new PDF")
    command.add_argument("--input", required=True)
    command.add_argument("--output", required=True, help=new_file)
    command.add_argument("--degrees", required=True, type=int, choices=(90, 180, 270))
    command.add_argument("--pages", help="One-based pages; default all")
    command = subparsers.add_parser("create", help="Wrap UTF-8 text into a new PDF")
    command.add_argument("--input", required=True)
    command.add_argument("--output", required=True, help=new_file)
    command.add_argument("--font", help="Workspace-relative TrueType font for Unicode text")
    command.add_argument("--title", default="Text document", help="Document title metadata")
    command.add_argument("--page-size", choices=tuple(PAGE_SIZES), default="letter")
    command.add_argument("--margin", type=float, default=54.0, help="Points, 18..144")
    command.add_argument("--font-size", type=float, default=11.0, help="Points, 6..36")
    args = parser.parse_args()
    try:
        root = Path(args.root).resolve(strict=True)
        if not root.is_dir():
            raise WorkflowError("Workspace root must be a directory")
        if args.command == "create":
            result = create(root, args)
        else:
            operation = {"extract": extract, "info": info, "merge": merge, "split": split, "rotate": rotate}[args.command]
            with pdf_parsing() as diagnostics:
                result = operation(root, args, diagnostics)
        print(json.dumps(result))
        return 0
    except ImportError as error:
        print(json.dumps({"status": "unavailable", "error": "missing_dependency", "module": error.name,
                          "message": "Install the skill's requirements in the authorized Python environment"}), file=sys.stderr)
    except (WorkflowError, OSError) as error:
        print(json.dumps({"status": "error", "error": type(error).__name__, "message": str(error)}), file=sys.stderr)
    except Exception as error:
        # Parser failures can include document contents. Do not echo arbitrary input.
        print(json.dumps({"status": "error", "error": type(error).__name__, "message": "PDF operation failed; input or font may be malformed"}), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
