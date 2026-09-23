# SPDX-License-Identifier: Apache-2.0
"""Read-only fixture sizing; never opens the original canonical root for writing."""
import hashlib
import json
import pathlib
import sqlite3
import struct
import sys

root = pathlib.Path(sys.argv[1])
backend = sys.argv[2]
if list(root.glob("*replay*")):
    raise RuntimeError("Replay-base fixture needs a separately declared counter")
if backend == "sqlite":
    with sqlite3.connect((root / "canonical.sqlite").resolve().as_uri() + "?mode=ro", uri=True) as db:
        db.execute("PRAGMA query_only=ON")
        commits, watermark = db.execute("SELECT COUNT(*), MAX(watermark) FROM commits").fetchone()
        events = db.execute("SELECT COUNT(*) FROM events").fetchone()[0]
        records = db.execute("SELECT COUNT(*) FROM records").fetchone()[0]
else:
    commits = events = watermark = 0
    current = {}
    chain = b"0" * 64
    with (root / "canonical.frames").open("rb") as stream:
        while header := stream.read(80):
            if len(header) != 80 or header[:8] != b"VCPJ0001":
                raise RuntimeError("Invalid journal header")
            length, inverse = struct.unpack("<II", header[8:16])
            if not 0 < length <= 64 * 1024 * 1024 or inverse != length ^ 0xFFFFFFFF or header[16:] != chain:
                raise RuntimeError("Invalid journal length or chain")
            payload, trailer = stream.read(length), stream.read(72)
            chain = hashlib.sha256(header + payload).hexdigest().encode()
            if len(payload) != length or trailer != chain + b"VCPCMIT1":
                raise RuntimeError("Invalid or partial committed journal frame")
            commit = json.loads(payload)
            commits += 1
            watermark = int(commit["receipt"]["watermark"])
            events += len(commit["transaction"]["events"])
            # Records are counted separately by the public command if the mutation
            # representation changes; sizing must never infer a wrong count.
            for mutation in commit["transaction"]["mutations"]:
                record = mutation.get("record")
                if record is not None:
                    current[(record["collection"], record["id"])] = True
                elif mutation:
                    raise RuntimeError("Unsupported sizing mutation")
    records = len(current)
print(json.dumps({"backend": backend, "commit_count": commits, "event_count": events,
                  "record_count": records, "watermark": str(watermark),
                  "method": "sqlite-read-only-counts" if backend == "sqlite" else "verified-journal-frames"}))
