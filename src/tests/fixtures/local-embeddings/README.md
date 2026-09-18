# Local embedding reference fixtures

These four texts are original synthetic examples about pausing work, retaining
local indexes and an unrelated recipe. They contain no repository history or
private user content. Their 384-dimensional reference vectors were generated
locally from all-MiniLM-L6-v2 at
`1110a243fdf4706b3f48f1d95db1a4f5529b4d41`, whose model card declares Apache-2.0.
[Asset identities](../../../third_party/components/minilm-assets.json) and
[runtime attribution](../../../../THIRD_PARTY_NOTICES.md#local-cpu-embedding-adapter-and-reference)
are separate from this synthetic input's authorship.

The independent [generator](../../support/minilm-reference.py) uses PyTorch
2.8.0+cpu, Transformers 4.57.1, Tokenizers 0.22.2 and NumPy 2.2.6 on CPython
3.13.12/Windows x64. It loads only local safetensors with remote code disabled,
uses eager BERT attention, masks padding before mean pooling and normalizes with
PyTorch. Existing fixture vectors never participate in generation. The Rust
adapter implements pooling independently and compares every value with absolute
tolerance `1e-5`, then checks unit norms, batching, truncation and model reopen.

To reproduce from a fresh disposable CPython 3.13 environment, after explicitly
[acquiring the assets](../../../../docs/development/local-embeddings.md):

```powershell
py -3.13 -m venv artifacts/embedding-reference-env
artifacts/embedding-reference-env/Scripts/python.exe -m pip --isolated install --disable-pip-version-check -r src/tests/fixtures/local-embeddings/reference-requirements.txt
artifacts/embedding-reference-env/Scripts/python.exe src/tests/support/minilm-reference.py --assets-root <verified-model-directory> --output artifacts/embedding-reference-new.json
```

The [requirements](reference-requirements.txt) pin all 25 installed reference
packages with native Windows CPython 3.13 wheel hashes, using public PyPI and the
official PyTorch CPU index. They are maintenance inputs; normal builds and CI
consume the committed fixture and do not install Python/PyTorch. The generator
rejects assets inside the checkout and output inside the asset directory, and
requires a new output file. Set a disposable home/profile when running it to
exclude ordinary ambient configuration discovery. Its Python socket interception
is a diagnostic, not proof of Windows OS network enforcement.

Review regenerated vectors and input/model/runtime identities together before
updating `minilm-golden.json`. Do not increase tolerance to conceal a numerical
or preprocessing regression. This tiny fixture qualifies an implementation seam;
it is not a recall benchmark, language-support claim or held-out product evaluation.
