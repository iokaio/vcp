# SPDX-License-Identifier: Apache-2.0
import json
import os
import posixpath
import re
import unittest
from pathlib import Path

SKILLS = Path(os.environ.get("VCP_SKILLS_ROOT", Path(__file__).resolve().parents[2] / "skills" / "builtin"))
PACKAGE = SKILLS / "frontend-design"
ROLES = {"context", "file", "reference"}
# Body plus context resources sent on activation (SH-13). References are read on demand.
CONTEXT_BUDGET_BYTES = 8192
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")


def load_descriptor():
    return json.loads((PACKAGE / "skill.json").read_text(encoding="utf-8"))


class FrontendDesignPackageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.descriptor = load_descriptor()
        cls.body = cls.descriptor["body"]["path"]
        cls.resources = cls.descriptor["resources"]
        cls.declared = {cls.body} | {r["path"] for r in cls.resources}

    def text(self, relative):
        return (PACKAGE / relative).read_text(encoding="utf-8")

    def test_resources_list_exactly_the_package_files(self):
        on_disk = {p.relative_to(PACKAGE).as_posix() for p in PACKAGE.rglob("*") if p.is_file()}
        paths = [r["path"] for r in self.resources]
        self.assertEqual(len(paths), len(set(paths)), "duplicate resource paths")
        self.assertEqual(set(paths), on_disk - {"skill.json", self.body})

    def test_resource_roles_are_known(self):
        for resource in self.resources:
            with self.subTest(path=resource["path"]):
                self.assertIn(resource.get("use", "context"), ROLES)

    def test_topic_references_are_on_demand_and_routed(self):
        body = self.text(self.body)
        references = [r for r in self.resources if r["path"].startswith("references/")]
        self.assertEqual(len(references), 3)
        for resource in references:
            with self.subTest(path=resource["path"]):
                self.assertEqual(resource.get("use"), "reference")
                self.assertIn(resource["path"], body)
        self.assertIn('"action":"read","skill":"frontend-design"', body)

    def test_relative_links_resolve_to_declared_files(self):
        sources = [self.body] + [r["path"] for r in self.resources
                                 if r["path"].startswith("references/") and r["path"].endswith(".md")]
        for source in sources:
            for target in LINK.findall(self.text(source)):
                if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("#"):
                    continue
                resolved = posixpath.normpath(posixpath.join(posixpath.dirname(source), target.split("#", 1)[0]))
                with self.subTest(source=source, target=target):
                    self.assertIn(resolved, self.declared)

    def test_context_stays_within_budget(self):
        size = len((PACKAGE / self.body).read_bytes())
        size += sum(len((PACKAGE / r["path"]).read_bytes())
                    for r in self.resources if r.get("use", "context") == "context")
        self.assertLessEqual(size, CONTEXT_BUDGET_BYTES)

    def test_body_keeps_wcag_baseline(self):
        body = self.text(self.body)
        for needle in ("WCAG 2.2 AA", "4.5:1", "3:1", "24 by 24 CSS px", "visible",
                       "320 CSS px", "prefers-reduced-motion: reduce"):
            with self.subTest(needle=needle):
                self.assertIn(needle, body)

    def test_contrast_and_reduced_motion_are_not_repeated(self):
        for relative in ("references/design-and-copy.md", "references/theming-and-performance.md"):
            text = self.text(relative)
            for needle in ("4.5:1", "3:1", "prefers-reduced-motion"):
                with self.subTest(file=relative, needle=needle):
                    self.assertNotIn(needle, text)
        self.assertNotIn("prefers-reduced-motion", self.text("references/accessibility.md"))

    def test_description_is_bounded_and_says_when_to_use(self):
        description = self.descriptor["description"]
        self.assertIn("Use when", description)
        self.assertLessEqual(len(description.encode("utf-8")), 1024)

    def test_upstream_has_no_stale_version_line(self):
        self.assertNotIn("VCP 2.0.0", self.text("UPSTREAM.md"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
