# SPDX-License-Identifier: Apache-2.0
import os
from pathlib import Path
import unittest

TESTS = Path(__file__).resolve().parent
SKILLS = Path(os.environ.get("VCP_SKILLS_ROOT", TESTS.parents[1] / "skills" / "builtin"))


def pins(path):
    return {line.strip() for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")}


class RequirementPinTests(unittest.TestCase):
    def test_test_environment_pins_every_package_requirement(self):
        for skill in ("pdf-workflows", "spreadsheet-workflows"):
            with self.subTest(skill=skill):
                package = pins(SKILLS / skill / "requirements.txt")
                tests = pins(TESTS / f"requirements-{skill}.txt")
                self.assertTrue(package, skill)
                self.assertLessEqual(package, tests, "test pins must match the shipped package pins")


if __name__ == "__main__":
    unittest.main()
