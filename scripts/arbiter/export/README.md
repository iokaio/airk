# Model export and reference implementations

- [MiniLM](minilm/README.md) contains versioned encoder export and quantization
  recipes, their grading dependencies and reversible source provenance.
- [Qwen](qwen/README.md) contains Focus/Context references, selected exports,
  external-data packing and numerical preparation recipes.

Use the recipe-specific README for exact prerequisites and commands. Model
weights, datasets, generated ONNX graphs and execution receipts remain external.
Fresh exports must use explicit private output directories and retain new source
and artifact identities; moving source code does not qualify an old or new model.
