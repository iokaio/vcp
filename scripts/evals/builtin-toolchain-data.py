# SPDX-License-Identifier: Apache-2.0
"""Exercise only the frozen data fixture using local Python standard libraries."""
import csv
from decimal import Decimal
import json
from pathlib import Path
import runpy
import sys


if len(sys.argv) != 2 or sys.argv[1] not in ("valid", "duplicates"):
    raise ValueError("Usage: builtin-toolchain-data.py valid|duplicates")

source = Path("input.csv")
original = source.read_bytes()
schema = json.loads(Path("schema.json").read_text(encoding="utf-8"))
if schema != {
    "key": "id",
    "columns": {"id": "integer", "amount": "decimal_or_null"},
    "duplicate_policy": "reject",
    "source_is_immutable": True,
}:
    raise ValueError("Unexpected data fixture schema")
with source.open(encoding="utf-8", newline="") as handle:
    rows = list(csv.DictReader(handle))
if rows != [
    {"id": "1", "amount": "12.50"},
    {"id": "1", "amount": "8.25"},
    {"id": "2", "amount": ""},
]:
    raise ValueError("Unexpected frozen data rows")

total = runpy.run_path(str(Path("transform.py").resolve()))["total"]
try:
    # A distinct-key subset checks decimal precision and the declared null value.
    # The duplicate case always supplies every original input row unchanged.
    if sys.argv[1] == "valid":
        actual = total([dict(rows[0]), dict(rows[2])])
        if not isinstance(actual, Decimal) or actual != Decimal("12.50"):
            raise AssertionError("Valid unique rows must aggregate to Decimal 12.50")
        print("Unique-key decimal/null aggregation passed.")
    else:
        try:
            actual = total([dict(row) for row in rows])
        except ValueError:
            print("Duplicate keys rejected before aggregation.")
        else:
            raise AssertionError(
                f"Duplicate key 1 was accepted; aggregate {actual} violates reject policy"
            )
finally:
    if source.read_bytes() != original:
        raise AssertionError("Data check changed immutable input.csv")
