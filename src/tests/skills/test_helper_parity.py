# SPDX-License-Identifier: Apache-2.0
import ast
import os
from pathlib import Path
import unittest

TESTS = Path(__file__).resolve().parent
SKILLS = Path(os.environ.get("VCP_SKILLS_ROOT", TESTS.parents[1] / "skills" / "builtin"))
HELPERS = (SKILLS / "pdf-workflows/scripts/pdf_workflows.py",
           SKILLS / "spreadsheet-workflows/scripts/spreadsheet_workflows.py")
# Each helper must stay a single materializable file, so the path and I/O boundary
# is duplicated on purpose. The copies must not drift apart.
SHARED = ("MAX_INPUT", "WorkflowError", "local_path", "read_input", "check_output", "write_new")


def definitions(path):
    source = path.read_text(encoding="utf-8")
    found = {}
    for node in ast.parse(source).body:
        if isinstance(node, (ast.FunctionDef, ast.ClassDef)):
            found[node.name] = ast.get_source_segment(source, node)
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    found[target.id] = ast.get_source_segment(source, node)
    return found


class HelperParityTests(unittest.TestCase):
    def test_shared_path_and_io_code_is_identical(self):
        pdf, spreadsheet = (definitions(path) for path in HELPERS)
        for name in SHARED:
            with self.subTest(name=name):
                self.assertIn(name, pdf)
                self.assertIn(name, spreadsheet)
                self.assertEqual(pdf[name], spreadsheet[name], f"{name} differs between the PDF and spreadsheet helpers")


if __name__ == "__main__":
    unittest.main()
