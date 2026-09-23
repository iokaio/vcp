# SPDX-License-Identifier: Apache-2.0
"""Independent read-only canonical decoder for fresh, single-task owner fixtures.

This does not import VCP or call its selector/retention implementation. Unsupported
prior rewrite/claim/delegation shapes stay outside the narrow retention oracle.
"""
import hashlib
from contextlib import closing
import json
import pathlib
import sqlite3
import struct
import sys


def load(root, backend):
    if any(root.glob("*replay*")) or (root / ".replay-roots").exists():
        raise ValueError("Prior replay/rewrite requires a separately qualified decoder")
    records, events = {}, []
    if backend == "sqlite":
        with closing(sqlite3.connect((root / "canonical.sqlite").resolve().as_uri() + "?mode=ro", uri=True)) as db:
            db.execute("PRAGMA query_only=ON")
            for key, payload, digest in db.execute("SELECT key,payload,digest FROM records ORDER BY key"):
                if hashlib.sha256(payload).hexdigest() != digest:
                    raise ValueError("Record digest mismatch")
                records[key] = json.loads(payload)
            events = [json.loads(row[0]) for row in db.execute("SELECT payload FROM events ORDER BY watermark,CAST(seq AS INTEGER)")]
            watermark = str(db.execute("SELECT MAX(watermark) FROM commits").fetchone()[0] or 0)
    elif backend == "files":
        chain, sequence, watermark = b"0" * 64, {}, "0"
        with (root / "canonical.frames").open("rb") as stream:
            while header := stream.read(80):
                if len(header) != 80 or header[:8] != b"VCPJ0001":
                    raise ValueError("Invalid committed frame header")
                length, inverse = struct.unpack("<II", header[8:16])
                if not 0 < length <= 64 * 1024 * 1024 or inverse != length ^ 0xFFFFFFFF or header[16:] != chain:
                    raise ValueError("Invalid committed frame length/chain")
                payload, trailer = stream.read(length), stream.read(72)
                chain = hashlib.sha256(header + payload).hexdigest().encode()
                if len(payload) != length or trailer != chain + b"VCPCMIT1":
                    raise ValueError("Invalid committed frame digest/trailer")
                commit = json.loads(payload)
                watermark = commit["receipt"]["watermark"]
                for mutation in commit["transaction"]["mutations"]:
                    record = mutation.get("record")
                    if record is None:
                        raise ValueError("Unsupported independent decoder mutation")
                    records[record["collection"] + ":" + record["id"]] = record
                for event in commit["transaction"]["events"]:
                    session = event["session"]
                    sequence[session] = sequence.get(session, 0) + 1
                    events.append({"version": 1, "sequence": str(sequence[session]), "watermark": watermark, "event": event})
    else:
        raise ValueError("Explicit SQLite/Files backend required")
    return {"records": records, "events": events, "watermark": watermark}


def oracle(state, task):
    records, events = state["records"], state["events"]
    tasks = [row for row in records.values() if row["collection"] == "task"]
    reasons = []
    if len(tasks) != 1 or tasks[0]["id"] != task or tasks[0]["value"].get("parent"):
        reasons.append("Retention oracle requires exactly one root task and no child graph")
    selected = []
    eligible = {"task", "artifact", "turn", "effect", "verification", "attempt", "settlement"}
    for key, row in records.items():
        value = row["value"]
        kind = value.get("document_type", "")
        # Ingestion progress is held in Claim storage but is not governed memory
        # and is not an eligible retention target (retention.rs::valid_record).
        # Keep all unknown claim documents unsupported.
        ingestion = kind in {"vcp_ingestion_job_v1", "vcp_ingestion_cursor_v1"}
        diagnostic_only = value.get("finding") is None
        if ingestion and kind == "vcp_ingestion_job_v1" and not diagnostic_only:
            try:
                finding = json.loads(value["finding"])
                diagnostic_only = isinstance(finding, dict) and finding.get("outputs") == 0 and isinstance(finding.get("findings"), list) and all(isinstance(item, dict) and item.get("code") in {"source_observed", "unsupported_observation", "work_observation"} for item in finding["findings"])
            except (ValueError, TypeError):
                diagnostic_only = False
        if ingestion and (value.get("scope", {}).get("task") != task or (kind == "vcp_ingestion_job_v1" and (value.get("root") != task or value.get("results") != [] or not diagnostic_only))):
            reasons.append("Ingestion progress outside the sole task or already produced governed results")
        if (row["collection"] == "claim" and not ingestion) or kind in {"vcp_memory_proposal_v1", "vcp_memory_version_v1", "vcp_memory_result_v1"} or kind.startswith(("vcp_memory_redacted_", "vcp_escalation_", "vcp_optimization_forecast", "vcp_retention_")):
            reasons.append("Existing memory/advisory/forecast/retention lineage outside narrow oracle")
        if value.get("redaction") or value.get("state") == "purged":
            reasons.append("Previously redacted source outside fresh owner oracle")
        scope = value.get("spec", {}).get("scope") if row["collection"] == "artifact" else value.get("scope")
        if row["collection"] in eligible:
            if not scope or scope.get("task") != task:
                reasons.append("Eligible record outside the one declared owner task")
            else:
                selected.append({"kind": "record", "id": key})
        if row["collection"] == "projection" and scope:
            reasons.append("Scoped projection requires separate dependency proof")
    for envelope in events:
        if envelope.get("redaction"):
            reasons.append("Previously redacted event outside fresh owner oracle")
        if envelope["event"].get("task") == task:
            selected.append({"kind": "event", "id": envelope["event"]["id"]})
    selected_records = {item["id"] for item in selected if item["kind"] == "record"}
    selected_record_ids = {records[key]["id"] for key in selected_records}
    for envelope in events:
        event = envelope["event"]
        if event.get("task") != task:
            facts = event.get("data", {}).get("facts", event.get("data", {}).get("records", []))
            refers = any("artifact:" + identity in selected_records for identity in event.get("artifacts", []))
            refers |= isinstance(facts, list) and any(isinstance(fact, dict) and fact.get("id") in selected_record_ids for fact in facts)
            if refers:
                reasons.append("Taskless/other-task event references selected history; independent dependency closure is required")
    protected = False
    protection_facts = []
    terminal_task = {"completed", "failed", "cancelled"}
    terminal_effect = {"succeeded", "failed", "cancelled"}
    for row in records.values():
        value, collection = row["value"], row["collection"]
        blocked = (collection in {"task", "turn"} and value["state"] not in terminal_task)
        blocked |= collection == "effect" and value["state"] not in terminal_effect
        blocked |= collection == "reservation" and (value["phase"] not in {"settled", "released", "explicitly_resolved"} or value["liability"] != "0")
        blocked |= collection == "artifact" and value["state"] not in {"complete", "aborted"}
        if blocked:
            protected = True
            protection_facts.append({"collection": collection, "id": row["id"], "state": value.get("state", value.get("phase")), "liability": value.get("liability")})
    return {"supported": not reasons, "not_run_reasons": sorted(set(reasons)), "selector_arguments": ["--task", task],
            "selected": sorted(selected, key=lambda x: (x["kind"], x["id"])), "dependent": [],
            "protected_if_purge": sorted(selected, key=lambda x: (x["kind"], x["id"])) if protected else [],
            "protection_facts": protection_facts,
            "basis": "Independent canonical task-scoped ID inventory; all eligible history belongs to one root task, so closure adds no targets. Any active recovery, liability or unterminated capture protects the atomic batch."}


if __name__ == "__main__":
    root, backend, task, destination = sys.argv[1:]
    state = load(pathlib.Path(root), backend)
    result = {"schema": "p805-independent-owner-state/1", "backend": backend, "task": task, "state": state, "retention_oracle": oracle(state, task)}
    with open(destination, "x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    print(json.dumps({"file": destination, "events": len(state["events"]), "records": len(state["records"]), "retention_oracle_supported": result["retention_oracle"]["supported"]}))
