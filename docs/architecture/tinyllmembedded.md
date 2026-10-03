**Yes—and VCP already contains most of the runtime foundation I would use for the first experiment.** Its `vcp-embedding` crate depends on **Candle 0.11.0**, with GPU-related default features disabled, and uses it for CPU-only MiniLM inference. That same Candle release includes a quantized **SmolLM2-360M-Instruct** example. You would not need to introduce a separate model server just to try a tiny generative model. [GitHub](https://raw.githubusercontent.com/iokaio/vcp/main/src/crates/vcp-embedding/Cargo.toml)

For VCP, my recommendation is:

> **Start with SmolLM2-360M-Instruct through the existing Candle dependency. Compare it with Granite 4.0-350M through `llama-cpp-2` before selecting a production model. Keep this as a bounded, advisory helper—not the coding agent or a policy authority.**

I inspected the repository’s relevant source and design documents; I have not built VCP or benchmarked these combinations on Windows.

## Models worth considering

These are small enough to make CPU-only embedding worth evaluating. The sizes below are **model files, not total runtime memory**.

| Model | Example quantized footprint | License | My assessment for VCP |
|---|---:|---|---|
| **SmolLM2-135M-Instruct** | About **105 MB Q4_K_M**, or **145 MB Q8_0** | Apache 2.0 | The smallest candidate I would test. Treat it as a footprint baseline for very narrow classification or extraction, rather than the default helper. [Hugging Face](https://huggingface.co/bartowski/SmolLM2-135M-Instruct-GGUF) |
| **SmolLM2-360M-Instruct** | About **271 MB Q4_K_M**; the author-published Q8_0 is **386 MB** | Apache 2.0 | **Lowest-friction first experiment**, because VCP’s pinned Candle release already has an example for it. [Hugging Face](https://huggingface.co/bartowski/SmolLM2-360M-Instruct-GGUF) |
| **Granite 4.0-350M** | Author-published **237 MB Q4_K_M**, or **378 MB Q8_0** | Apache 2.0 | A strong comparison candidate. IBM explicitly targets on-device use and lists classification, extraction, summarization, and function calling among its capabilities. That is not proof of accuracy on VCP’s decisions, but it is a relevant intended use. [Hugging Face](https://huggingface.co/ibm-granite/granite-4.0-350m-GGUF) |
| **LFM2.5-350M** | Author-published **229 MB Q4_K_M** | Custom LFM Open License | Technically interesting: Liquid recommends extraction, structured outputs, and tool use, while explicitly advising against programming and knowledge-intensive tasks. I would not make it VCP’s unconditional bundled default because of its licensing restrictions. [Hugging Face](https://huggingface.co/LiquidAI/LFM2.5-350M-GGUF/tree/main) |

The SmolLM2 Q4 sizes above come from **third-party quantizations**. For a released product, I would either pin and qualify those exact artifacts or produce your own quantizations from pinned original weights.

**The licensing distinction matters here.** SmolLM2 and Granite use Apache 2.0. LFM’s custom license includes a **$10 million annual-revenue threshold affecting commercial use**. That complicates adoption by enterprise users even when the application distributing or loading it is open source. [Hugging Face](https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct)

## Rust-native versus Rust-callable

Both approaches below can run **inside the VCP engine process**. The difference is the implementation and deployment dependency, not whether inference requires a server.

### 1. Candle: the natural first choice for this repository

VCP already has these dependencies:

```toml
candle-core = { version = "=0.11.0", default-features = false }
candle-nn = { version = "=0.11.0", default-features = false }
candle-transformers = { version = "=0.11.0", default-features = false }
```

That is the existing manifest, not a proposed addition. The pinned Candle example loads SmolLM2 through `candle_transformers::models::quantized_llama` and reads GGUF weights. [GitHub](https://raw.githubusercontent.com/iokaio/vcp/main/src/crates/vcp-embedding/Cargo.toml)

I would reuse the **dependency and asset-management patterns**, but not put generation into `MiniLm`. That adapter is specifically a BERT-based embedding implementation with its own dimensions, token limits, and verified asset specification. A text-generating helper should have a separate interface and model state. [GitHub](https://raw.githubusercontent.com/iokaio/vcp/main/src/crates/vcp-embedding/src/lib.rs)

For the first prototype, I would use the author-published **SmolLM2-360M Q8_0** artifact, because it matches Candle’s example. Then test Q4 against the same evaluation set rather than assuming the smaller quantization preserves the decisions you care about. [GitHub](https://raw.githubusercontent.com/huggingface/candle/0.11.0/candle-examples/examples/quantized/main.rs)

The important limitation is that **a GGUF loader is not universal model support**. Candle’s SmolLM2 example establishes a useful starting point; it does not establish that Granite or LFM will work through that same model implementation.

### 2. `llama-cpp-2`: the broader model-comparison route

`llama-cpp-2` provides Rust bindings to the native `llama.cpp` library. The wrapper supports static linking, optional acceleration backends, and JSON-schema-to-grammar functionality. This is a library integration, not a requirement to launch `llama-server`. [GitHub](https://raw.githubusercontent.com/utilityai/llama-cpp-rs/main/llama-cpp-2/Cargo.toml)

I would choose this route when testing Granite or LFM, or when grammar-constrained generation becomes important enough to justify another native dependency. Their publishers provide GGUF artifacts for `llama.cpp`-compatible inference. [Hugging Face](https://huggingface.co/ibm-granite/granite-4.0-350m-GGUF)

The tradeoff is additional native build and packaging qualification. In particular, the wrapper’s defaults include **OpenMP**; I would make those features deliberate rather than accepting them without reviewing the Windows runtime dependency closure. [GitHub](https://raw.githubusercontent.com/utilityai/llama-cpp-rs/main/llama-cpp-2/Cargo.toml)

**For VCP, I would not add this second runtime until the Candle prototype establishes that the helper use case is valuable—or a comparison model materially outperforms it.**

## Where I would insert it in VCP

Your design already identifies the right boundary: **`DecisionEvaluator`**, described in ADR-020 for bounded Boolean, Choice, and Score questions, with explicit abstention and provenance. The document keeps validation, policy, budgets, persistence, and side effects under Rust’s control. It is a proposed design, not evidence of a completed or qualified local evaluator. [GitHub](https://github.com/iokaio/vcp/blob/main/docs/adr/020-bounded-semantic-decisions.md)

There is also an explicit entry in the decision design for a **local semantic decision model**, currently classified as an unselected future option requiring pinned assets, license review, resource measurements, and native Windows evidence. This would therefore be a deliberate extension of that design, not simply another OpenRouter model identifier. [GitHub](https://github.com/iokaio/vcp/blob/main/docs/architecture/decision-evaluation-design.md)

I would add a local adapter behind that boundary and keep the flow:

**Engine supplies bounded evidence → local model produces a typed suggestion → Rust validates it → existing policy decides what happens.**

Good first experiments would be identifying a request as explanation versus implementation, classifying a short observed failure into a fixed category, or suggesting a memory-claim category from a small evidence excerpt. Each should have a usable `unknown` or `abstain` result.

I would **not** initially use it to decide whether a change is correct, whether a command is authorized, whether tests can be skipped, or whether a task is complete. That also preserves VCP’s existing separation between advisory judgments and actual authority. [GitHub](https://github.com/iokaio/vcp/blob/main/docs/architecture/decision-evaluation-design.md)

For example, a useful helper request might be:

```text
Classify the observed failure.

Allowed answers:
compile_error
test_assertion
missing_dependency
environment_error
unknown

Evidence:
<short, bounded diagnostic excerpt>

Return one allowed answer.
```

That is a much better initial target than “reason about the repository and choose what the agent should do next.”

For fixed-choice questions, I would prefer constrained labels or candidate-answer scoring over unconstrained prose. For small structured outputs, `llama.cpp` supports grammar constraints and conversion of a subset of JSON Schema. **Valid structure still does not establish a correct semantic answer.** [GitHub](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/grammars/README.md)

## What “virtually any Windows developer machine” requires

I would treat that as an explicit release target—not a property conferred by choosing a 350M model. VCP’s current installation documentation itself makes no general minimum-hardware or Windows-version support claim. [GitHub](https://github.com/iokaio/vcp/blob/main/docs/usage/beta-installation.md)

These are the constraints I would set for the experiment:

| Concern | Proposed VCP baseline |
|---|---|
| Hardware | CPU-only, with no GPU or NPU requirement; qualify on a modest x64 developer laptop with 8 GB RAM |
| Context | Start around **512–1,024 total tokens**, not the model’s maximum supported context |
| Output | Prefer a label; otherwise cap output around **16–64 tokens** |
| Concurrency | One helper request at a time initially; bounded queue and worker-thread use |
| Memory | Aim for **less than 1 GB incremental peak working set** for the helper, including loading and inference—not just the weight file |
| Startup | Load lazily and retain the model across calls; do not reload weights for each decision |
| Failure | Missing model, load failure, timeout, or invalid output returns an unavailable/abstention result and preserves deterministic behavior |

Those numbers are **proposed qualification targets**, not measured performance claims.

Three deployment details deserve particular attention:

**CPU instruction compatibility.** Do not distribute a binary compiled with `target-cpu=native` and assume it will run on older processors. Also, `GGML_NATIVE=OFF` alone does not establish a conservative baseline: `llama.cpp` has separate AVX, AVX2, BMI2, and other instruction-set options. Inspect the effective build configuration and test the exact packaged binary on the oldest supported CPU class. [GitHub](https://raw.githubusercontent.com/utilityai/llama-cpp-rs/main/llama-cpp-sys-2/build.rs)

**Model files versus executable files.** In-process inference does not require embedding hundreds of megabytes of weights into `vcp.exe`. I would provision a separate, versioned model asset with a digest, tokenizer/chat-template identity, license, and quantization record. VCP’s existing MiniLM loader already demonstrates file-only loading and SHA-256 verification without an implicit download or remote fallback. [GitHub](https://raw.githubusercontent.com/iokaio/vcp/main/src/crates/vcp-embedding/src/lib.rs)

**Cancellation and responsiveness.** I would run inference on an engine-owned, bounded blocking worker, check cancellation between decoding steps, and bound prompt-processing work. A local helper should not freeze the CLI or undermine pause behavior. Nor should a local failure silently send previously local evidence to OpenRouter; any remote fallback should remain an explicit policy choice.

## My concrete recommendation

**Begin with Candle + SmolLM2-360M-Instruct Q8_0, then evaluate Q4_K_M.** This minimizes architectural churn and uses an inference path already demonstrated in VCP’s pinned dependency version. [GitHub](https://raw.githubusercontent.com/iokaio/vcp/main/src/crates/vcp-embedding/Cargo.toml)

Use a few hundred representative, labeled VCP microtasks to compare it against your deterministic baseline. Measure correct answers, harmful misclassifications, abstention, cold-load time, warm end-to-end latency, peak memory, and responsiveness while the machine is compiling code.

Then compare **Granite 4.0-350M Q4_K_M through `llama-cpp-2`**. Select it only if the quality or performance improvement warrants the extra native dependency. Keep **SmolLM2-135M** as the smaller-footprint challenger rather than assuming the smallest model is the best default.

**The feasible product here is a small local semantic helper. The critical qualification is not whether it can run—it is whether its mistakes are sufficiently rare, bounded, and harmless to improve VCP over ordinary Rust rules.**