# SPDX-License-Identifier: Apache-2.0
import json
import os
import re
import unittest
from pathlib import Path

ROOT = Path(
    os.environ.get("VCP_SKILLS_ROOT")
    or Path(__file__).resolve().parents[2] / "skills" / "builtin"
)
PACKAGE = ROOT / "llm-integration"
DESCRIPTOR = PACKAGE / "skill.json"
NEUTRAL = PACKAGE / "assets" / "tool-schema-neutral.json"
ANTHROPIC = PACKAGE / "assets" / "tool-schema.json"

MODEL_ID = re.compile(r"\b(claude|gpt|gemini|llama|mistral)-[0-9]", re.IGNORECASE)
PRICE = re.compile(r"\$\s?[0-9]")
LINK = re.compile(r"\]\(([^)\s]+)\)")


def package_files():
    return sorted(p for p in PACKAGE.rglob("*") if p.is_file())


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def walk_objects(schema, where="parameters"):
    """Yield (location, schema) for every object schema nested in schema."""
    if isinstance(schema, dict):
        if schema.get("type") == "object" or "properties" in schema:
            yield where, schema
        for key, value in schema.items():
            yield from walk_objects(value, f"{where}.{key}")
    elif isinstance(schema, list):
        for index, value in enumerate(schema):
            yield from walk_objects(value, f"{where}[{index}]")


class LlmIntegrationAssetTests(unittest.TestCase):
    def test_every_json_file_parses(self):
        json_files = [p for p in package_files() if p.suffix == ".json"]
        self.assertIn(NEUTRAL, json_files)
        self.assertIn(ANTHROPIC, json_files)
        for path in json_files:
            with self.subTest(path=path.relative_to(PACKAGE).as_posix()):
                self.assertIsInstance(load(path), dict)

    def test_neutral_schema_is_closed_object(self):
        tool = load(NEUTRAL)
        self.assertEqual(set(tool), {"name", "description", "parameters"})
        self.assertRegex(tool["name"], r"^[A-Za-z_][A-Za-z0-9_]{0,63}$")
        self.assertTrue(tool["description"].strip())
        params = tool["parameters"]
        self.assertEqual(params["type"], "object")
        self.assertIsInstance(params["properties"], dict)
        self.assertTrue(params["properties"])
        for where, obj in walk_objects(params):
            with self.subTest(object=where):
                self.assertIs(obj.get("additionalProperties"), False)
                required = obj.get("required", [])
                self.assertEqual(len(required), len(set(required)))
                self.assertLessEqual(set(required), set(obj.get("properties", {})))

    def test_anthropic_asset_wraps_the_neutral_schema(self):
        neutral = load(NEUTRAL)
        wrapped = load(ANTHROPIC)
        self.assertEqual(set(wrapped), {"name", "description", "input_schema"})
        self.assertEqual(wrapped["name"], neutral["name"])
        self.assertEqual(wrapped["description"], neutral["description"])
        self.assertEqual(wrapped["input_schema"], neutral["parameters"])

    def test_descriptor_lists_exactly_the_package_files(self):
        descriptor = load(DESCRIPTOR)
        description = descriptor["description"]
        self.assertIn("Use when", description)
        self.assertLessEqual(len(description.encode("utf-8")), 1024)
        listed = [descriptor["body"]["path"]] + [r["path"] for r in descriptor["resources"]]
        self.assertEqual(len(listed), len(set(listed)))
        present = {
            p.relative_to(PACKAGE).as_posix() for p in package_files() if p != DESCRIPTOR
        }
        self.assertEqual(set(listed), present)

    def test_relative_markdown_links_resolve(self):
        for path in package_files():
            if path.suffix != ".md":
                continue
            for target in LINK.findall(path.read_text(encoding="utf-8")):
                if re.match(r"^[a-z]+:", target) or target.startswith("#"):
                    continue
                with self.subTest(path=path.name, target=target):
                    resolved = (path.parent / target.split("#", 1)[0]).resolve()
                    self.assertTrue(resolved.is_file())
                    self.assertTrue(resolved.is_relative_to(PACKAGE.resolve()))

    def test_no_model_identifiers_or_prices(self):
        for path in package_files():
            text = path.read_text(encoding="utf-8")
            with self.subTest(path=path.relative_to(PACKAGE).as_posix()):
                self.assertIsNone(MODEL_ID.search(text))
                self.assertIsNone(PRICE.search(text))


if __name__ == "__main__":
    unittest.main(verbosity=2)
