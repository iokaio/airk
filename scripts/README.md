# Repository tooling

The [source rescue catalogue](../docs/arbiter/source-rescue.md) links every receiving README and records coverage and reproduction limits.

Build, verification and model-reproduction implementation belongs in tracked
source folders. Generated artifacts, datasets, weights, caches and execution
receipts remain separate inputs/outputs; they are not a substitute for source.

- [Arbiter research and reproduction](arbiter/README.md): training, export,
  evaluation, analysis, versioned recipes and source provenance.
- [CUDA static runtime](cuda-static-runtime/README.md): native build verification
  and the final optimized graph collector.
- [Incremental CUDA producer](cuda-static-incremental/README.md): authenticated
  single-translation-unit rebuilds and input-freeze generation.
- [Native process helpers](cuda-native-execution/README.md): maintained owned
  execution primitives used by CUDA and model checks.
- [Local build workflow](local-build-README.md): application build and packaging.

Each workflow README states prerequisites, commands, output locations and limits.
A source migration or passing lightweight test is not model or release qualification.
