# SPDX-License-Identifier: Apache-2.0
"""Bounded existing helper cases; selected installed resources, never acquisition."""
import hashlib
import ctypes
import importlib.metadata
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import unittest

CASES = {
    "pdf": ("test_pdf_workflows.py", [
        "PdfWorkflows.test_generation_and_page_selected_extraction_preserve_sources",
        "PdfWorkflows.test_blank_page_has_explicit_no_text_outcome",
        "PdfWorkflows.test_merge_concatenates_inputs_into_a_new_pdf",
        "PdfWorkflows.test_existing_output_and_escaping_or_special_paths_are_rejected",
    ]),
    "spreadsheet": ("test_spreadsheet_workflows.py", [
        "SpreadsheetWorkflows.test_create_and_inspect_typed_cells_and_formula_freshness",
        "SpreadsheetWorkflows.test_recalculation_corrects_stale_caches_and_preserves_formulas_and_parts",
        "SpreadsheetWorkflows.test_csv_import_applies_explicit_types_and_rejects_ambiguity",
        "SpreadsheetWorkflows.test_csv_export_uses_cached_values_and_neutralizes_formula_text",
        "SpreadsheetWorkflows.test_source_and_output_paths_are_preserved",
    ]),
    "mcp": ("test_mcp_port.py", ["PaginationPortTests"]),
}
EXPECTED = {"pdf": 4, "spreadsheet": 5, "mcp": 7}


def digest(file):
    with open(file, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def plain(file):
    file = Path(file)
    if not file.is_absolute():
        raise ValueError("Absolute helper input required")
    for current in [file, *file.parents]:
        stat = current.lstat()
        if stat.st_file_attributes & 0x400:
            raise ValueError("Redirected helper dependency rejected")
    return file


def inventory(root, output, exclude=()):
    root = plain(root)
    rows = []
    total = 0
    for directory, folders, files in os.walk(root, followlinks=False):
        folders[:] = sorted(name for name in folders if Path(directory, name) not in exclude)
        for name in folders:
            plain(Path(directory, name))
        for name in sorted(files):
            file = plain(Path(directory, name))
            length = file.stat().st_size
            total += length
            if len(rows) >= 100000 or total > 2 * 1024**3:
                raise ValueError("Dependency inventory exceeds bound")
            rows.append({"path": file.relative_to(root).as_posix(), "bytes": length, "sha256": digest(file)})
    rows.sort(key=lambda row: row["path"])
    data = json.dumps(rows, ensure_ascii=True, separators=(",", ":")).encode()
    Path(output).write_bytes(data)
    return {"root": str(root), "files": len(rows), "bytes": total,
            "inventory": str(output), "sha256": hashlib.sha256(data).hexdigest()}


def prerequisites(spec, label):
    if sys.version_info[:3] != (3, 11, 9) or sys.prefix == sys.base_prefix:
        raise ValueError("Existing isolated Python 3.11.9 virtual environment required")
    config = plain(Path(sys.prefix, "pyvenv.cfg"))
    if "include-system-site-packages = false" not in config.read_text(encoding="utf-8").lower():
        raise ValueError("Virtual environment must exclude system site packages")
    packages = {}
    for file in [Path(spec["skills"], "pdf-workflows/requirements.txt"),
                 Path(spec["skills"], "spreadsheet-workflows/requirements.txt"),
                 Path(spec["tests"], "requirements-spreadsheet-workflows.txt")]:
        for line in plain(file).read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            name, expected = line.split("==")
            actual = importlib.metadata.version(name)
            if actual != expected:
                raise ValueError("Required helper dependency version differs: " + name)
            packages[name.lower().replace("_", "-")] = actual
    output = Path(spec["output"])
    base = plain(Path(sys.base_prefix))
    # Store Python venv launchers name a Windows App Execution Alias. Bind the
    # loaded process image, not the alias's mutable routing or unreadable bytes.
    image = ctypes.create_unicode_buffer(32768)
    length = ctypes.windll.kernel32.GetModuleFileNameW(None, image, len(image))
    if not length or length >= len(image):
        raise ValueError("Loaded Python process image unavailable")
    actual_image = plain(Path(image.value))
    if actual_image.parent != base or actual_image.name.lower() not in ("python.exe", "python3.11.exe"):
        raise ValueError("Loaded Python image differs from selected base runtime")
    alias = Path(sys._base_executable)
    alias_stat = alias.lstat()
    runtime = [plain(Path(sys.executable)), actual_image, config]
    runtime += [plain(file) for file in sorted(base.glob("*.dll"))]
    trees = [inventory(Path(sys.prefix, "Lib/site-packages"), output / f"python-{label}-packages.json"),
             inventory(base / "Lib", output / f"python-{label}-stdlib.json", (base / "Lib/site-packages",)),
             inventory(base / "DLLs", output / f"python-{label}-dlls.json")]
    return {"version": sys.version, "packages": packages,
            "base_launcher": {"path": str(alias), "attributes": alias_stat.st_file_attributes,
                              "reparse_tag": alias_stat.st_reparse_tag},
            "runtime": [{"path": str(file), "sha256": digest(file)} for file in runtime], "trees": trees}


def run(spec, group):
    filename, names = CASES[group]
    source = plain(Path(spec["tests"], filename))
    if os.environ.get("VCP_SKILLS_ROOT") != spec["skills"] or not sys.dont_write_bytecode:
        raise ValueError("Installed skill selection and bytecode refusal required")
    module_spec = importlib.util.spec_from_file_location("vcp_installed_helper_cases", source)
    module = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(module)
    suite = unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromName(name, module) for name in names)
    if suite.countTestCases() != EXPECTED[group]:
        raise ValueError("Exact helper case count differs")
    log = io.StringIO()
    result = unittest.TextTestRunner(stream=log, verbosity=2).run(suite)
    text = log.getvalue()
    Path(spec["output"], group + "-tests.log").write_text(text[:1024**2], encoding="utf-8")
    passed = result.wasSuccessful() and not result.skipped and result.testsRun == EXPECTED[group] and len(text) <= 1024**2
    return {"status": "pass" if passed else "fail", "group": group, "cases": names,
            "tests_run": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
            "skipped": len(result.skipped), "harness_sha256": digest(source),
            "log_sha256": digest(Path(spec["output"], group + "-tests.log"))}


if __name__ == "__main__":
    try:
        spec = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8-sig"))
        mode = sys.argv[2]
        value = prerequisites(spec, mode) if mode in ("before", "after") else run(spec, mode)
        print(json.dumps(value, ensure_ascii=True))
        sys.exit(0 if value.get("status", "pass") == "pass" else 1)
    except Exception as error:
        print(json.dumps({"status": "fail", "error": str(error)[:1000]}))
        sys.exit(1)
