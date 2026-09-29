# SPDX-License-Identifier: Apache-2.0
"""Read, create, edit and explicitly recalculate a bounded basic XLSX subset."""
import argparse
from datetime import date, datetime
import hashlib
import io
import json
import math
from pathlib import Path
import re
import sys
from zipfile import ZipFile

MAX_BYTES = 20 * 1024 * 1024
MAX_CELLS = 100_000
SAFE_FUNCTIONS = {"SUM", "AVERAGE", "MIN", "MAX", "COUNT", "COUNTA", "IF", "ROUND", "ABS"}
CELL = re.compile(r"[A-Z]{1,3}[1-9][0-9]{0,6}\Z")
RANGE = re.compile(r"(?:(?:'[^']+'|[A-Za-z_][A-Za-z0-9_ ]*)!)?\$?[A-Z]{1,3}\$?[1-9][0-9]{0,6}(?::\$?[A-Z]{1,3}\$?[1-9][0-9]{0,6})?\Z")
PART = re.compile(r"(?:\[Content_Types\]\.xml|_rels/\.rels|docProps/(?:app|core)\.xml|xl/(?:workbook|styles|sharedStrings)\.xml|xl/_rels/workbook\.xml\.rels|xl/theme/theme[0-9]+\.xml|xl/worksheets/sheet[0-9]+\.xml|xl/worksheets/_rels/sheet[0-9]+\.xml\.rels)\Z")


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


def read_input(root, name):
    with local_path(root, name).open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise WorkflowError("Input exceeds 20 MiB")
    return data


def write_new(root, name, data):
    if len(data) > MAX_BYTES:
        raise WorkflowError("Output exceeds 20 MiB")
    with local_path(root, name, existing=False).open("xb") as stream:
        stream.write(data)


def validate_archive(data):
    from defusedxml.ElementTree import fromstring

    with ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len(entries) > 256 or len({entry.filename for entry in entries}) != len(entries):
            raise WorkflowError("Too many or duplicate workbook parts")
        if sum(entry.file_size for entry in entries) > 32 * 1024 * 1024:
            raise WorkflowError("Expanded workbook exceeds 32 MiB")
        for entry in entries:
            if not PART.fullmatch(entry.filename):
                raise WorkflowError("Workbook contains unsupported parts; preserve it with another tool")
            if entry.file_size > 8 * 1024 * 1024 or entry.flag_bits & 1:
                raise WorkflowError("Oversized or encrypted workbook part")
            element = fromstring(archive.read(entry))
            for node in element.iter():
                tag = node.tag.rsplit("}", 1)[-1]
                if tag in {"extLst", "definedName", "oleObject", "control"}:
                    raise WorkflowError("Workbook contains unsupported extensions or defined names")
                if tag == "Relationship" and node.get("TargetMode", "").lower() == "external":
                    raise WorkflowError("External workbook relationships are unsupported")
                if "macroenabled" in node.get("ContentType", "").lower():
                    raise WorkflowError("Macro-enabled workbooks are unsupported")


def validate_formula(value):
    from openpyxl.formula import Tokenizer
    from openpyxl.utils.cell import range_boundaries

    if not isinstance(value, str) or not value.startswith("=") or len(value) > 4096:
        raise WorkflowError("Formula must start with '=' and fit 4096 characters")
    if any(char in value for char in "[]|\r\n"):
        raise WorkflowError("External references and control characters are unsupported")
    tokens = Tokenizer(value).items
    if not tokens:
        raise WorkflowError("Empty formula")
    for token in tokens:
        if token.type == "FUNC" and token.subtype == "OPEN" and token.value[:-1].upper() not in SAFE_FUNCTIONS:
            raise WorkflowError("Formula function is outside the supported local subset")
        if token.type == "OPERAND" and token.subtype == "RANGE" and not RANGE.fullmatch(token.value):
            raise WorkflowError("Only bounded local A1 formula references are supported")
        if token.type == "OPERAND" and token.subtype == "RANGE":
            first_col, first_row, last_col, last_row = range_boundaries(token.value.rsplit("!", 1)[-1])
            if not 1 <= first_col <= last_col <= 256 or not 1 <= first_row <= last_row <= 10000 or (last_col - first_col + 1) * (last_row - first_row + 1) > MAX_CELLS:
                raise WorkflowError("Formula range exceeds the supported dimensions")


def validate_workbook(workbook):
    area = 0
    if len(workbook.worksheets) > 32:
        raise WorkflowError("Workbook exceeds 32 sheets")
    for sheet in workbook:
        area += sheet.max_row * sheet.max_column
        if sheet.max_row > 10000 or sheet.max_column > 256 or area > MAX_CELLS:
            raise WorkflowError("Workbook exceeds 10000 rows, 256 columns or 100000 total rectangular cells")
        for row in sheet.iter_rows():
            for cell in row:
                if cell.data_type == "f":
                    validate_formula(cell.value)


def load(data, cached=False):
    import openpyxl

    validate_archive(data)
    workbook = openpyxl.load_workbook(io.BytesIO(data), data_only=cached, keep_links=False, rich_text=True)
    validate_workbook(workbook)
    return workbook


def assign(cell, specification):
    if not isinstance(specification, dict) or set(specification) - {"type", "value", "number_format"}:
        raise WorkflowError("Cells require a typed object with optional number_format")
    kind, value = specification.get("type"), specification.get("value")
    if kind == "blank":
        if value is not None:
            raise WorkflowError("Blank cell value must be null")
        cell.value = None
    elif kind == "string":
        if not isinstance(value, str) or len(value) > 32767 or any(ord(char) < 32 and char not in "\t\n\r" for char in value):
            raise WorkflowError("String cells must contain valid text within 32767 characters")
        cell.value = value
        cell.data_type = "s"  # '=...' input stays text unless explicitly typed formula.
    elif kind == "number":
        if type(value) not in (int, float) or not math.isfinite(value) or (type(value) is int and abs(value) > 999_999_999_999_999):
            raise WorkflowError("Use finite numbers; identifiers over 15 digits must be strings")
        cell.value = value
    elif kind == "boolean":
        if type(value) is not bool:
            raise WorkflowError("Boolean cell requires true or false")
        cell.value = value
    elif kind in {"date", "datetime"}:
        if not isinstance(value, str):
            raise WorkflowError("Dates require ISO strings")
        cell.value = date.fromisoformat(value) if kind == "date" else datetime.fromisoformat(value)
        if isinstance(cell.value, datetime) and cell.value.tzinfo is not None:
            raise WorkflowError("Excel dates have no timezone; normalize explicitly first")
    elif kind == "formula":
        validate_formula(value)
        cell.value = value
    else:
        raise WorkflowError("Supported types: blank, string, number, boolean, date, datetime, formula")
    if "number_format" in specification:
        if not isinstance(specification["number_format"], str) or len(specification["number_format"]) > 128:
            raise WorkflowError("number_format must be a string within 128 characters")
        cell.number_format = specification["number_format"]


def apply_specification(workbook, specification, creating):
    from openpyxl.utils.cell import coordinate_to_tuple

    if not isinstance(specification, dict) or set(specification) != {"sheets"} or not isinstance(specification["sheets"], list) or not 1 <= len(specification["sheets"]) <= 32:
        raise WorkflowError("Specification requires 1..32 sheets")
    seen = set()
    for item in specification["sheets"]:
        if not isinstance(item, dict) or set(item) != {"name", "cells"}:
            raise WorkflowError("Each sheet requires name and cells")
        name = item["name"]
        if not isinstance(name, str) or not name or len(name) > 31 or re.search(r"[\\/*?:\[\]]", name) or name.casefold() in seen:
            raise WorkflowError("Sheet names must be valid and unique")
        seen.add(name.casefold())
        if not isinstance(item["cells"], dict) or len(item["cells"]) > MAX_CELLS:
            raise WorkflowError("Cells must be a bounded address-to-value object")
        if not creating and name not in workbook.sheetnames:
            raise WorkflowError("Edits must name an existing sheet")
        sheet = workbook.create_sheet(name) if creating else workbook[name]
        for address, value in item["cells"].items():
            if not isinstance(address, str) or not CELL.fullmatch(address):
                raise WorkflowError("Cell addresses must use uppercase A1 notation")
            row, column = coordinate_to_tuple(address)
            if row > 10000 or column > 256:
                raise WorkflowError("Cell is outside the supported sheet dimensions")
            assign(sheet[address], value)
    validate_workbook(workbook)


def json_value(value):
    if isinstance(value, (date, datetime)):
        return value.isoformat()
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def cached_value(cell):
    # OOXML's explicit string-cache type distinguishes an empty string result
    # from an absent numeric cache; openpyxl returns None for both raw values.
    if cell.value is None and cell.data_type == "str":
        return ""
    return json_value(cell.value)


def inspect(root, args):
    from openpyxl.utils.cell import range_boundaries

    source = read_input(root, args.input)
    workbook, cached = load(source), load(source, cached=True)
    if args.sheet not in workbook.sheetnames:
        raise WorkflowError("Requested sheet does not exist")
    if not re.fullmatch(r"[A-Z]{1,3}[1-9][0-9]{0,6}(?::[A-Z]{1,3}[1-9][0-9]{0,6})?", args.range):
        raise WorkflowError("Use a bounded A1 range")
    first_col, first_row, last_col, last_row = range_boundaries(args.range)
    if not 1 <= first_col <= last_col <= 256 or not 1 <= first_row <= last_row <= 10000 or (last_col - first_col + 1) * (last_row - first_row + 1) > 10000:
        raise WorkflowError("Inspection range exceeds the supported dimensions or 10000 cells")
    cells = []
    for row in workbook[args.sheet].iter_rows(min_row=first_row, max_row=last_row, min_col=first_col, max_col=last_col):
        for cell in row:
            item = {"address": cell.coordinate, "type": cell.data_type, "value": json_value(cell.value), "number_format": cell.number_format}
            if cell.data_type == "f":
                item.update(cached_value=cached_value(cached[args.sheet][cell.coordinate]), cache_freshness="unknown")
            cells.append(item)
    result = {"status": "ok", "source": args.input, "source_sha256": hashlib.sha256(source).hexdigest(),
              "sheet": args.sheet, "range": args.range, "date_epoch": workbook.epoch.isoformat(),
              "cells": cells, "recalculation": "not_run"}
    write_new(root, args.output, (json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode("utf-8"))
    return {"status": "ok", "output": args.output, "cells": len(cells), "recalculation": "not_run"}


def modify(root, args):
    from openpyxl import Workbook
    from openpyxl.cell.rich_text import CellRichText
    from openpyxl.workbook.properties import CalcProperties

    creating = args.command == "create"
    if creating:
        workbook = Workbook()
        workbook.remove(workbook.active)
    else:
        workbook = load(read_input(root, args.input))
    specification = json.loads(read_input(root, args.input if creating else args.changes))
    apply_specification(workbook, specification, creating)
    workbook.calculation = CalcProperties(calcMode="auto", fullCalcOnLoad=True, forceFullCalc=True)
    empty_strings = []
    for sheet in workbook:
        for row in sheet.iter_rows():
            for cell in row:
                if cell.data_type == "s" and cell.value == "":
                    # openpyxl omits the inline-string element for a plain "".
                    # Its public rich-text API emits an explicit empty text node,
                    # preserving the distinction from a blank cell on reopening.
                    empty_strings.append((sheet.title, cell.coordinate))
                    cell.value = CellRichText([""])
    result = io.BytesIO()
    workbook.save(result)
    # Check the saved format with a fresh reader before creating the destination.
    reopened = load(result.getvalue())
    for sheet, address in empty_strings:
        if reopened[sheet][address].value != "" or reopened[sheet][address].data_type != "s":
            raise WorkflowError("Saved workbook did not preserve a literal empty string")
    write_new(root, args.output, result.getvalue())
    return {"status": "ok", "output": args.output, "sheets": workbook.sheetnames,
            "recalculation": "not_run", "formula_caches": "invalidated_on_save"}


def recalculate(root, args):
    import formualizer

    source = read_input(root, args.input)
    workbook = load(source)
    # Formualizer 0.9.3 reads literal empty text as blank (e.g. COUNTA then
    # undercounts). Do not write fresh-looking caches with changed semantics.
    if any(cell.data_type == "s" and cell.value == "" for sheet in workbook
           for row in sheet.iter_rows() for cell in row):
        raise WorkflowError("Recalculation of literal empty-string cells is unsupported by this engine; use another authorized engine")
    formulas = {(sheet.title, cell.coordinate): cell.value for sheet in workbook
                for row in sheet.iter_rows() for cell in row if cell.data_type == "f"}
    calculated = formualizer.recalculate_xlsx_bytes(source, error_location_limit=20)
    summary = calculated["summary"]
    if summary.get("status") != "success" or summary.get("errors") != 0 or summary.get("total_errors") != 0 or summary.get("evaluated") != len(formulas):
        raise WorkflowError("Recalculation did not complete without formula errors; no output was written")
    output = calculated["bytes"]
    reopened, cached = load(output), load(output, cached=True)
    if reopened.sheetnames != workbook.sheetnames or reopened.epoch != workbook.epoch:
        raise WorkflowError("Recalculation changed workbook structure")
    for (sheet, address), formula in formulas.items():
        if reopened[sheet][address].value != formula or reopened[sheet][address].data_type != "f" or cached_value(cached[sheet][address]) is None or cached[sheet][address].data_type == "e":
            raise WorkflowError("Recalculation failed formula preservation or cache validation")
    with ZipFile(io.BytesIO(source)) as original, ZipFile(io.BytesIO(output)) as updated:
        if set(original.namelist()) != set(updated.namelist()):
            raise WorkflowError("Recalculation changed workbook parts")
        for name in original.namelist():
            if not re.fullmatch(r"xl/worksheets/sheet[0-9]+\.xml", name) and original.read(name) != updated.read(name):
                raise WorkflowError("Recalculation changed an unrelated workbook part")
    write_new(root, args.output, output)
    return {"status": "ok", "output": args.output, "source_sha256": hashlib.sha256(source).hexdigest(),
            "recalculation": "completed_supported_subset", "engine": "formualizer", "formulas": len(formulas),
            "cache_cells_changed": calculated["cache_cells_changed"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("inspect", "create", "edit", "recalculate"):
        command = subparsers.add_parser(name)
        command.add_argument("--input", required=True)
        command.add_argument("--output", required=True, help="New file; existing files are never replaced")
        if name == "inspect":
            command.add_argument("--sheet", required=True)
            command.add_argument("--range", required=True)
        elif name == "edit":
            command.add_argument("--changes", required=True)
    args = parser.parse_args()
    try:
        root = Path(args.root).resolve(strict=True)
        if not root.is_dir():
            raise WorkflowError("Workspace root must be a directory")
        operation = {"inspect": inspect, "create": modify, "edit": modify, "recalculate": recalculate}[args.command]
        result = operation(root, args)
        print(json.dumps(result))
        return 0
    except ImportError as error:
        print(json.dumps({"status": "unavailable", "error": "missing_dependency", "module": error.name,
                          "message": "Install the skill's requirements in the authorized Python environment"}), file=sys.stderr)
    except (WorkflowError, OSError) as error:
        print(json.dumps({"status": "error", "error": type(error).__name__, "message": str(error)}), file=sys.stderr)
    except Exception as error:
        print(json.dumps({"status": "error", "error": type(error).__name__, "message": "Workbook operation failed; input may be malformed or unsupported"}), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
