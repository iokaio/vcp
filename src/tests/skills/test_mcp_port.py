# SPDX-License-Identifier: Apache-2.0
import ast
import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(os.environ.get("VCP_SKILLS_ROOT", Path(__file__).resolve().parents[2] / "skills" / "builtin"))
SOURCE = ROOT / "mcp-development" / "assets" / "paginated_result.py"
namespace = {}
exec(compile(SOURCE.read_text(encoding="utf-8"), str(SOURCE), "exec"), namespace)
page = namespace["paginated_result"]


class PaginationPortTests(unittest.TestCase):
    def test_walks_dataset_without_missing_or_repeating_ids(self):
        records = [{"id": str(i)} for i in range(53)]
        offset = 0
        received = []
        while True:
            result = page(records[offset:offset + 20], len(records), 20, offset)
            self.assertEqual(result["count"], len(result["items"]))
            received.extend(item["id"] for item in result["items"])
            if not result["has_more"]:
                self.assertIsNone(result["next_offset"])
                break
            self.assertGreater(result["next_offset"], offset)
            offset = result["next_offset"]
        self.assertEqual(received, [str(i) for i in range(53)])

    def test_empty_dataset_and_past_end_are_terminal(self):
        for total, offset in ((0, 0), (3, 3), (3, 10)):
            with self.subTest(total=total, offset=offset):
                result = page([], total, offset=offset)
                self.assertFalse(result["has_more"])
                self.assertIsNone(result["next_offset"])

    def test_untrusted_numeric_inputs_are_rejected(self):
        for field in ("total", "offset", "limit"):
            for value in (True, False, "1", 1.5, None):
                args = {"items": [], "total": 0, "limit": 20, "offset": 0}
                args[field] = value
                with self.subTest(field=field, value=value):
                    with self.assertRaises(ValueError):
                        page(**args)

    def test_bad_bounds_and_nonprogressing_pages_are_rejected(self):
        bad_calls = [
            ([], -1, 20, 0), ([], 0, 20, -1), ([], 0, 0, 0),
            ([], 0, 101, 0), ([1, 2], 2, 1, 0), ([1], 0, 20, 0),
            ([1], 3, 20, 3), ([], 5, 20, 0), ((1,), 1, 20, 0),
        ]
        for args in bad_calls:
            with self.subTest(args=args):
                with self.assertRaises(ValueError):
                    page(*args)

    def test_max_limit_is_configurable(self):
        records = [{"id": str(i)} for i in range(250)]
        result = page(records[:250], 250, 250, 0, max_limit=250)
        self.assertEqual(result["count"], 250)
        self.assertFalse(result["has_more"])
        small = page(records[:5], 250, 5, 0, max_limit=5)
        self.assertEqual(small["next_offset"], 5)
        with self.assertRaises(ValueError):
            page([], 0, 6, 0, max_limit=5)
        with self.assertRaises(ValueError):
            page([], 0, 101, 0)
        self.assertEqual(page([], 0, 100, 0)["count"], 0)
        self.assertEqual(namespace["DEFAULT_MAX_LIMIT"], 100)

    def test_invalid_max_limit_is_rejected(self):
        for value in (0, -1, True, False, "100", 100.0, None):
            with self.subTest(max_limit=value):
                with self.assertRaises(ValueError):
                    page([], 0, 1, 0, max_limit=value)

    def test_response_is_json_serializable(self):
        result = page([{"id": "alpha", "name": "Snow \u96ea"}], 1)
        self.assertEqual(json.loads(json.dumps(result)), result)


def fenced_blocks(text, language):
    """Return fenced code blocks of one language; fail on an unclosed fence."""
    blocks = []
    for part in text.split(f"```{language}\n")[1:]:
        if "```" not in part:
            raise AssertionError(f"unclosed {language} fence")
        blocks.append(part.split("```", 1)[0])
    return blocks


JS_LITERAL = re.compile(
    r'"(?:\\.|[^"\\\n])*"'
    r"|'(?:\\.|[^'\\\n])*'"
    r"|`(?:\\.|[^`\\])*`"
    r"|//[^\n]*"
    r"|/\*.*?\*/",
    re.S,
)


def strip_js_literals(code):
    """Blank out JS/TS strings and comments so bracket matching sees only code."""
    return JS_LITERAL.sub('""', code)


def brackets_balanced(code):
    pairs = {")": "(", "]": "[", "}": "{"}
    stack = []
    for char in strip_js_literals(code):
        if char in "([{":
            stack.append(char)
        elif char in pairs:
            if not stack or stack.pop() != pairs[char]:
                return False
    return not stack


class ReferenceGuidanceTests(unittest.TestCase):
    PACKAGE = ROOT / "mcp-development"
    REFERENCES = ("references/protocol.md", "references/python.md", "references/typescript.md")

    def read(self, name):
        return (self.PACKAGE / name).read_text(encoding="utf-8")

    def test_descriptor_marks_split_references_on_demand(self):
        descriptor = json.loads(self.read("skill.json"))
        roles = {item["path"]: item.get("use", "context") for item in descriptor["resources"]}
        for name in self.REFERENCES:
            with self.subTest(reference=name):
                self.assertEqual(roles.get(name), "reference")
                self.assertTrue((self.PACKAGE / name).is_file())
                self.assertLessEqual(len((self.PACKAGE / name).read_bytes()), 64 * 1024)
        self.assertNotIn("references/server-patterns.md", roles)
        self.assertFalse((self.PACKAGE / "references" / "server-patterns.md").exists())
        self.assertEqual(roles.get("assets/paginated_result.py"), "file")
        self.assertNotIn("context", roles.values())

    def test_body_names_each_reference_and_read_action(self):
        body = self.read("SKILL.md")
        self.assertIn('"action":"read","skill":"mcp-development"', body)
        for name in self.REFERENCES:
            with self.subTest(reference=name):
                self.assertIn(f"`{name}`", body)
        self.assertNotIn("server-patterns.md", body)

    def test_python_guidance_uses_mcp_2_api(self):
        text = self.read("references/python.md")
        self.assertIn("from mcp.server import MCPServer", text)
        self.assertNotIn("from mcp.server.fastmcp import", text)
        self.assertIn("mcp` 2.2.0", text)

    def test_python_snippets_parse(self):
        blocks = fenced_blocks(self.read("references/python.md"), "python")
        self.assertGreaterEqual(len(blocks), 2)
        for block in blocks:
            ast.parse(block)
        for name in ("references/protocol.md", "references/typescript.md"):
            for block in fenced_blocks(self.read(name), "python"):
                ast.parse(block)

    def test_typescript_snippet_structure(self):
        # Structural check: no TypeScript compiler is a test dependency.
        text = self.read("references/typescript.md")
        self.assertEqual(text.count("```") % 2, 0)
        blocks = fenced_blocks(text, "typescript")
        self.assertEqual(len(blocks), 1)
        code = blocks[0]
        self.assertTrue(brackets_balanced(code))
        self.assertIn('import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";', code)
        self.assertIn('import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";', code)
        self.assertRegex(code, r'new McpServer\(\{ name: "issues", version: "[^"]+" \}\)')
        self.assertRegex(code, r'server\.registerTool\(\s*"issues_search",')
        for key in ("inputSchema:", "outputSchema:", "annotations:", "structuredContent: output"):
            with self.subTest(key=key):
                self.assertIn(key, code)
        self.assertIn("await server.connect(new StdioServerTransport());", code)
        for deprecated in ("server.tool(", "server.resource(", "server.prompt("):
            self.assertNotIn(deprecated, code)

    def test_bracket_checker_detects_imbalance(self):
        self.assertTrue(brackets_balanced('f({ a: "})", b: [1] }); // )'))
        self.assertFalse(brackets_balanced("f({ a: [1 }]);"))
        self.assertFalse(brackets_balanced("f(("))

    def test_typescript_snippet_parses_as_module_when_node_available(self):
        # The snippet uses no TypeScript-only syntax, so Node can check it as
        # an ES module. This is a syntax check, not a type-check.
        node = shutil.which("node")
        if node is None:
            self.skipTest("node not on PATH; structural check only")
        code = fenced_blocks(self.read("references/typescript.md"), "typescript")[0]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "snippet.mjs"
            path.write_text(code, encoding="utf-8")
            result = subprocess.run([node, "--check", str(path)], capture_output=True,
                                    text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_protocol_revision_and_topics_are_named(self):
        protocol = self.read("references/protocol.md")
        for needle in ("2026-07-28", "outputSchema", "structuredContent", "Elicitation",
                       "Resources", "Prompts", "Authorization", "Inspector", "Pagination",
                       "Transports", "isError"):
            with self.subTest(needle=needle):
                self.assertIn(needle, protocol)
        self.assertIn("registerTool", self.read("references/typescript.md"))
        self.assertIn("2025-11-25", self.read("references/typescript.md"))

    def test_asset_license_survives_materialization(self):
        text = self.read("assets/paginated_result.py")
        self.assertEqual(text.splitlines()[0], "# SPDX-License-Identifier: Apache-2.0")
        self.assertNotIn("../LICENSE.txt", text)
        self.assertIn("https://www.apache.org/licenses/LICENSE-2.0", text)
        self.assertIn("https://github.com/anthropics/skills/blob/8a1541c4a3ffa5a20a5a91de0dcf3f0bab1d1ef4/"
                      "skills/mcp-builder/reference/python_mcp_server.md", text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
