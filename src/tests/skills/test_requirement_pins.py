# SPDX-License-Identifier: Apache-2.0
import json
import os
from pathlib import Path
import re
import unittest

TESTS = Path(__file__).resolve().parent
SKILLS = Path(os.environ.get("VCP_SKILLS_ROOT", TESTS.parents[1] / "skills" / "builtin"))


def pins(path):
    return {line.strip() for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")}


class RequirementPinTests(unittest.TestCase):
    def test_dependency_license_records_match_tested_versions(self):
        components = TESTS.parents[1] / "third_party" / "components"
        for skill, component in (("pdf-workflows", "pdf"), ("spreadsheet-workflows", "spreadsheet")):
            with self.subTest(skill=skill):
                manifest = json.loads((components / f"{component}-skill-dependencies.json").read_text(encoding="utf-8"))
                normalize = lambda name: re.sub(r"[-_.]+", "-", name).lower()
                recorded = {normalize(item["name"]): item["version"] for item in manifest["dependencies"]}
                tested = {normalize(name): version for name, version in
                          (pin.split("==") for pin in pins(TESTS / f"requirements-{skill}.txt"))}
                self.assertEqual(recorded, tested, "license provenance must follow the pinned dependency versions")
                for dependency in manifest["dependencies"]:
                    for license_file in dependency["license_files"]:
                        self.assertIn(f'-{dependency["version"]}.dist-info/', license_file["path"])

    def test_test_environment_pins_every_package_requirement(self):
        for skill in ("pdf-workflows", "spreadsheet-workflows"):
            with self.subTest(skill=skill):
                package = pins(SKILLS / skill / "requirements.txt")
                tests = pins(TESTS / f"requirements-{skill}.txt")
                self.assertTrue(package, skill)
                self.assertLessEqual(package, tests, "test pins must match the shipped package pins")


if __name__ == "__main__":
    unittest.main()
