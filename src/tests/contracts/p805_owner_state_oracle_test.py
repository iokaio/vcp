# SPDX-License-Identifier: Apache-2.0
"""Fault controls for the independent task-scoped retention oracle; no VCP run."""
import copy
import importlib.util
import pathlib
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("oracle", pathlib.Path(__file__).resolve().parents[3] / "scripts/evals/p805-owner-state-oracle.py")
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)
task = "root-task"
def record(kind, identity, value):
    return {"collection": kind, "id": identity, "workspace": "workspace", "value": value}
state = {"records": {
    "task:root-task": record("task", task, {"scope": {"task": task}, "parent": None, "state": "completed"}),
    "artifact:output": record("artifact", "output", {"spec": {"scope": {"task": task}}, "state": "complete"}),
    "reservation:money": record("reservation", "money", {"phase": "settled", "liability": "0"}),
    "index_intent:index": record("index_intent", "index", {"document_type": "vcp_memory_index_intent_v1"}),
}, "events": [{"event": {"id": "event-one", "task": task}}]}
expected = [{"kind": "event", "id": "event-one"}, {"kind": "record", "id": "artifact:output"}, {"kind": "record", "id": "task:root-task"}]
result = oracle.oracle(state, task)
assert result["supported"] and result["selected"] == expected and result["dependent"] == [] and result["protected_if_purge"] == []
for collection, key, change in [
    ("task", "task:root-task", {"state": "paused"}),
    ("reservation", "reservation:money", {"phase": "reconciliation_pending", "liability": "8"}),
    ("artifact", "artifact:output", {"state": "pending"}),
]:
    altered = copy.deepcopy(state)
    altered["records"][key]["value"].update(change)
    result = oracle.oracle(altered, task)
    assert result["supported"] and result["protected_if_purge"] == expected and result["protection_facts"]
altered = copy.deepcopy(state)
altered["records"]["task:other"] = record("task", "other", {"scope": {"task": "other"}, "parent": task, "state": "completed"})
assert not oracle.oracle(altered, task)["supported"]
altered = copy.deepcopy(state)
altered["records"]["claim:truth"] = record("claim", "truth", {"document_type": "vcp_memory_version_v1"})
assert not oracle.oracle(altered, task)["supported"]
altered = copy.deepcopy(state)
altered["events"].append({"event": {"id": "taskless-copy", "task": None, "artifacts": ["output"]}})
assert not oracle.oracle(altered, task)["supported"]
import hashlib
from contextlib import closing
import json
import sqlite3
import struct
import tempfile

with tempfile.TemporaryDirectory(prefix="vcp-p805-oracle-test-") as directory:
    root = pathlib.Path(directory)
    sqlite_root, files_root = root / "sqlite", root / "files"
    sqlite_root.mkdir()
    files_root.mkdir()
    event = {"id": "event-one", "task": task, "session": "session"}
    envelope = {"version": 1, "sequence": "1", "watermark": "1", "event": event}
    record_bytes = json.dumps(state["records"]["task:root-task"], separators=(",", ":")).encode()
    with closing(sqlite3.connect(sqlite_root / "canonical.sqlite")) as db:
        db.executescript("CREATE TABLE records(key TEXT,payload BLOB,digest TEXT); CREATE TABLE events(payload BLOB,watermark INTEGER,seq TEXT); CREATE TABLE commits(watermark INTEGER);")
        db.execute("INSERT INTO records VALUES(?,?,?)", ("task:root-task", record_bytes, hashlib.sha256(record_bytes).hexdigest()))
        db.execute("INSERT INTO events VALUES(?,?,?)", (json.dumps(envelope).encode(), 1, "1"))
        db.execute("INSERT INTO commits VALUES(1)")
        db.commit()
    commit = {"receipt": {"watermark": "1"}, "transaction": {"mutations": [{"record": json.loads(record_bytes)}], "events": [event]}}
    payload = json.dumps(commit).encode()
    header = b"VCPJ0001" + struct.pack("<II", len(payload), len(payload) ^ 0xFFFFFFFF) + b"0" * 64
    frame = header + payload + hashlib.sha256(header + payload).hexdigest().encode() + b"VCPCMIT1"
    journal = files_root / "canonical.frames"
    journal.write_bytes(frame)
    assert oracle.load(sqlite_root, "sqlite") == oracle.load(files_root, "files")
    journal.write_bytes(frame[:-1])
    try:
        oracle.load(files_root, "files")
        raise AssertionError("partial committed journal accepted")
    except ValueError as error:
        assert "digest/trailer" in str(error)
    with closing(sqlite3.connect(sqlite_root / "canonical.sqlite")) as db:
        db.execute("UPDATE records SET digest=?", ("0" * 64,))
        db.commit()
    try:
        oracle.load(sqlite_root, "sqlite")
        raise AssertionError("corrupt canonical record accepted")
    except ValueError as error:
        assert "digest mismatch" in str(error)
    (files_root / ".replay-roots").mkdir()
    try:
        oracle.load(files_root, "files")
        raise AssertionError("unqualified rewritten journal accepted")
    except ValueError as error:
        assert "separately qualified decoder" in str(error)

print("11 independent oracle controls passed, including SQLite/Files parity, committed-frame corruption, record corruption and replay refusal")
