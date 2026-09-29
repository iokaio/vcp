# SPDX-License-Identifier: Apache-2.0
"""Local, bounded text-PDF extraction and generation. No OCR or PDF editing."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import sys

MAX_INPUT = 20 * 1024 * 1024
MAX_PAGES = 200
MAX_TEXT = 100_000


class WorkflowError(ValueError):
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


def extract(root, args):
    from pypdf import PdfReader

    source = read_input(root, args.input)
    reader = PdfReader(io.BytesIO(source), strict=True)
    if reader.is_encrypted:
        raise WorkflowError("Encrypted PDFs are unsupported; use an authorized decrypted copy")
    if not 1 <= len(reader.pages) <= MAX_PAGES:
        raise WorkflowError("PDF must contain 1..200 pages")
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
    result = {"status": "ok" if not empty else "no_extractable_text" if empty == len(pages) else "partial",
              "source": args.input, "source_sha256": hashlib.sha256(source).hexdigest(),
              "page_count": len(reader.pages), "pages": pages, "ocr": "not_run"}
    write_new(root, args.output, (json.dumps(result, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    return {"status": result["status"], "output": args.output, "pages": len(pages), "ocr": "not_run"}


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


def create(root, args):
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.pdfgen import canvas

    source = read_input(root, args.input, 4 * MAX_TEXT)
    content = source.decode("utf-8").replace("\r\n", "\n")
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
        if any(ord(char) not in font.face.charWidths for char in content if char not in "\n\t"):
            raise WorkflowError("Selected font does not cover every input character")
    elif any(ord(char) > 126 for char in content):
        raise WorkflowError("Non-ASCII text requires --font with an authorized TrueType font")
    output = io.BytesIO()
    document = canvas.Canvas(output, pagesize=(612, 792), invariant=1)
    document.setTitle("Text document")
    pages, y = 1, 738
    document.setFont(font_name, 11)
    for line in wrapped_lines(content, font_name, 11, 504, pdfmetrics):
        if y < 54:
            document.showPage()
            pages += 1
            if pages > MAX_PAGES:
                raise WorkflowError("Generated document exceeds 200 pages")
            document.setFont(font_name, 11)
            y = 738
        document.drawString(54, y, line)
        y -= 15
    document.save()
    write_new(root, args.output, output.getvalue())
    return {"status": "ok", "output": args.output, "pages": pages, "visual_review": "not_run"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, help="Authorized workspace root")
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("extract", "create"):
        command = subparsers.add_parser(name)
        command.add_argument("--input", required=True)
        command.add_argument("--output", required=True, help="New file; existing files are never replaced")
        if name == "extract":
            command.add_argument("--pages", help="One-based pages, e.g. 1,3-5")
        else:
            command.add_argument("--font", help="Workspace-relative TrueType font for Unicode text")
    args = parser.parse_args()
    try:
        root = Path(args.root).resolve(strict=True)
        if not root.is_dir():
            raise WorkflowError("Workspace root must be a directory")
        result = {"extract": extract, "create": create}[args.command](root, args)
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
