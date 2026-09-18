# SPDX-License-Identifier: Apache-2.0
# Independent library reference for public synthetic embedding qualification.
import argparse
import os
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
import hashlib
import json
from pathlib import Path
import socket
import sys
import time
import torch
import transformers
from transformers import AutoTokenizer, BertModel

def reject_network(*args, **kwargs):
    raise RuntimeError("Unexpected reference network attempt")

socket.socket.connect = reject_network
socket.socket.connect_ex = reject_network
socket.create_connection = reject_network
parser = argparse.ArgumentParser()
parser.add_argument("--assets-root", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
source = Path(__file__).resolve()
root = args.assets_root.resolve()
output = args.output.resolve()
if root.is_relative_to(source.parents[3]) or output.is_relative_to(root):
    parser.error("Model assets must be outside the checkout, and output outside assets")
record = json.loads((source.parents[2] / "third_party/components/minilm-assets.json").read_text(encoding="utf-8"))
assert record["revision"] == "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
for item in record["files"]:
    content = (root / item["path"]).read_bytes()
    assert len(content) == item["bytes"]
    assert hashlib.sha256(content).hexdigest() == item["sha256"]
torch.set_num_threads(4)
started = time.perf_counter()
tokenizer = AutoTokenizer.from_pretrained(root, local_files_only=True, trust_remote_code=False)
model = BertModel.from_pretrained(root, local_files_only=True, use_safetensors=True, attn_implementation="eager").eval().cpu()
fixture = json.loads((source.parents[1] / "fixtures/local-embeddings/minilm-golden.json").read_text(encoding="utf-8"))
texts = [case["text"] for case in fixture["cases"]]
assert len(texts) == 4  # Only text is used; existing vectors are never an oracle for this generator.
tokens = tokenizer(texts, padding=True, truncation=True, max_length=256, return_tensors="pt")
with torch.no_grad():
    hidden = model(**tokens).last_hidden_state
    mask = tokens["attention_mask"].unsqueeze(-1).to(hidden.dtype)
    mean = (hidden * mask).sum(dim=1) / mask.sum(dim=1)
    vectors = torch.nn.functional.normalize(mean, p=2, dim=1)
result = {"status":"pass", "model_revision":record["revision"],
          "python":sys.version.split()[0], "torch":torch.__version__,
          "transformers":transformers.__version__, "elapsed_ms":(time.perf_counter()-started)*1000,
          "input_sha256":hashlib.sha256(json.dumps(texts, ensure_ascii=False, separators=(",", ":")).encode("utf-8")).hexdigest(),
          "vectors":vectors.tolist(), "input_ids":tokens["input_ids"].tolist()}
output.parent.mkdir(parents=True, exist_ok=True)
with output.open("x", encoding="utf-8") as file:
    json.dump(result, file, indent=2)
    file.write("\n")
print(json.dumps({"status":"pass", "cases":len(texts), "input_sha256":result["input_sha256"]}))
