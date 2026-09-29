# SPDX-License-Identifier: Apache-2.0
import ast
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / "skills" / "builtin"
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

    def test_response_is_json_serializable(self):
        result = page([{"id": "alpha", "name": "Snow \u96ea"}], 1)
        self.assertEqual(json.loads(json.dumps(result)), result)


if __name__ == "__main__":
    unittest.main(verbosity=2)
