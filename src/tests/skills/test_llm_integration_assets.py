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

# Vendor model names: a family prefix followed by a version digit or tier word.
# Bare vendor names ("Gemini API", "claude-api", "GPT-style") must not match.
MODEL_ID = re.compile(
    r"""\b(?:
        claude-(?:[0-9]|instant|(?:opus|sonnet|haiku)\b)
      | (?:opus|sonnet|haiku)[\s-][0-9]
      | (?:chat)?gpt-?[0-9]
      | o[0-9](?:-(?:mini|pro|preview))?\b
      | gemini-(?:[0-9]|pro|flash|ultra|nano)
      | gemma-?[0-9]
      | llama-?[0-9]
      | (?:mistral|mixtral|codestral|ministral)-(?:[0-9]|large|medium|small|tiny|nemo)
    )""",
    re.IGNORECASE | re.VERBOSE,
)
MODEL_ID_POSITIVE = [
    "claude-sonnet-4-5",
    "claude-opus-4-1",
    "claude-3-haiku",
    "Claude Sonnet 4.5",
    "gpt-4o",
    "gpt-5",
    "use o3 here",
    "o4-mini",
    "o1-preview",
    "gemini-2.5-pro",
    "gemini-pro",
    "llama-3",
    "Llama3.1",
    "mistral-large",
    "mixtral-8x7b",
]
MODEL_ID_NEGATIVE = [
    "the claude-api skill",
    "Gemini API",
    "GPT-style completion",
    "Mistral AI",
    "llama and alpaca",
    "a sonnet about opus magnum",
    "go1.22 and photo3",
    "OAuth2 and IPv4",
    "OpenAPI 3.0-style Schema",
    "o-ring",
]
PROVIDER_REFERENCES = {
    "references/anthropic.md",
    "references/openai.md",
    "references/gemini.md",
    "references/hosting.md",
}
# The Anthropic-shaped schema is derived from the neutral one (checked below),
# so it is on demand as well.
ON_DEMAND = PROVIDER_REFERENCES | {"assets/tool-schema.json"}
CONTEXT_BUDGET = 16 * 1024
REFERENCE_LIMIT = 64 * 1024
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

    def test_resource_roles_keep_provider_notes_on_demand(self):
        descriptor = load(DESCRIPTOR)
        roles = {}
        for resource in descriptor["resources"]:
            use = resource.get("use", "context")
            with self.subTest(path=resource["path"]):
                self.assertIn(use, {"context", "file", "reference"})
            roles[resource["path"]] = use
        references = {path for path, use in roles.items() if use == "reference"}
        self.assertEqual(references, ON_DEMAND)
        self.assertEqual(roles["assets/tool-schema-neutral.json"], "context")
        self.assertEqual(roles["references/tool-schemas.md"], "context")
        self.assertEqual(roles["LICENSE.txt"], "file")
        self.assertEqual(roles["UPSTREAM.md"], "file")
        for path in sorted(references):
            with self.subTest(reference=path):
                data = (PACKAGE / path).read_bytes()
                data.decode("utf-8")
                self.assertLessEqual(len(data), REFERENCE_LIMIT)
        context = [descriptor["body"]["path"]] + [
            path for path, use in roles.items() if use == "context"
        ]
        size = sum((PACKAGE / path).stat().st_size for path in context)
        self.assertLessEqual(size, CONTEXT_BUDGET)

    def test_body_names_each_reference_and_the_read_action(self):
        body = (PACKAGE / "SKILL.md").read_text(encoding="utf-8")
        self.assertIn('"action": "read"', body)
        self.assertIn('"skill": "llm-integration"', body)
        for path in sorted(ON_DEMAND):
            with self.subTest(reference=path):
                self.assertIn(f"`{path}`", body)

    def test_model_id_pattern_samples(self):
        for sample in MODEL_ID_POSITIVE:
            with self.subTest(positive=sample):
                self.assertIsNotNone(MODEL_ID.search(sample))
        for sample in MODEL_ID_NEGATIVE:
            with self.subTest(negative=sample):
                self.assertIsNone(MODEL_ID.search(sample))

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
                match = MODEL_ID.search(text)
                self.assertIsNone(match, match and match.group(0))
                self.assertIsNone(PRICE.search(text))


if __name__ == "__main__":
    unittest.main(verbosity=2)
