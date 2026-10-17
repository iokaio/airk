# Written GPU and assembly evidence

The `specialist-gpu-facts/1` adapter family extracts bounded written syntax. It does not compile or execute code, validate graphics pipelines, expand preprocessing, infer an assembly ISA, or resolve references.

| Profile | Selection and evidence | Limits of interpretation |
| --- | --- | --- |
| `cuda` | `.cu`/`.cuh`; C++ namespaces, classes/structs and functions, written kernel qualifiers, launch bounds, attributes and kernel launch syntax | No NVCC, template instantiation, device execution or resolved launch target |
| `wgsl` | `.wgsl`; structs, functions, declared variables, attributes, written stages, groups and bindings | The admitted grammar includes Bevy extensions; preprocessing/import-path forms explicitly mark partial coverage and are not expanded |
| `glsl` | GLSL suffixes; declarations including multiple declarators, interface blocks, written storage qualifiers, layouts, functions and directives | Vertex/fragment/unknown stage is bound in the effective profile from the filename; written `#version` is evidence, not a compiler validation result |
| `assembly` | `.asm`, `.s`, `.S`; labels, constants, directives, sections, linkage declarations and written instructions including operands | Generic ISA remains unknown, coverage partial, and instructions are not classified as calls or branches |
| `assembly-x86-att`, `assembly-x86-intel`, `assembly-aarch64` | Explicit policy overrides; same original syntax with finite dialect-specific call/branch mnemonic classification | Written mnemonic classification is not resolved control flow. Uppercase `.S` remains preprocessing-unexpanded and partial |

All evidence preserves original UTF-8 byte spans and original line bounds, including CRLF and non-BMP text. Declaration facts use the existing `definition` kind, keep their actual grammar `syntax_kind`, and retain the specialized declaration category in code modifiers. Fingerprints cover only supported declaration dimensions; unknown bodies and documentation remain unknown. Qualified continuity uses the normal native declaration contract, not instruction similarity.

Sibling declarators retain their actual containing owner. A declaration selected from a child node does not become the owner of all of the traversal node's other children. The GLSL payload regression exercises global siblings, local siblings inside a function and an interface block through extraction, path-neutral conversion and binding.

Structured input is limited to 4 MiB; larger UTF-8 sources use [partial text-only extraction](ingestion-limits.md). Final serialized evidence remains limited to 16 MiB. Shared extraction budgets bound traversal to 500,000 work units and 10,000 facts, with a maximum owner depth of 128. Display names are UTF-8-safe and bounded; exact source spans remain complete. Traversal skips comments, literal contents and macro replacement text. Malformed syntax and preprocessing are explicitly partial. Source files are never written or executed.

## Exact dependencies

| Crate | Upstream revision | ABI and scanner | License |
| --- | --- | --- | --- |
| `tree-sitter-cuda` 0.21.1 | [48b066f](https://github.com/tree-sitter-grammars/tree-sitter-cuda/tree/48b066f334f4cf2174e05a50218ce2ed98b6fd01) | ABI 15; packaged C scanner and allocation/parser headers | MIT, archive license |
| `tree-sitter-wgsl-bevy` 0.1.4, dependency alias `tree-sitter-wgsl` | [d9306a7](https://github.com/tree-sitter-grammars/tree-sitter-wgsl-bevy/tree/d9306a798ede627001a8e5752f775858c8edd7e4) | ABI 14; packaged C scanner | MIT, archive license |
| `tree-sitter-glsl` 0.2.0 | [a6a297b](https://github.com/theHamsta/tree-sitter-glsl/tree/a6a297b59bd615589d55f4529313dc2c87e815b8) | ABI 14; generated C parser, no external scanner | MIT, captured exact-revision license |
| `tree-sitter-asm` 0.24.0 | [5bb5b03](https://github.com/RubixDev/tree-sitter-asm/tree/5bb5b03e3c1ce5853b5282b9fba060f7c7bbf11e) | ABI 14; generated C parser, no external scanner; optional WASM feature disabled | MIT, captured exact-revision license |

All four expose `tree-sitter-language` language functions compatible with the admitted Tree-sitter 0.25 runtime. Their build scripts compile native C and include MSVC UTF-8 handling. This source inspection does not replace platform qualification.

Published crate archive SHA-256 values:

```text
cuda-0.21.1      715eecfee69b15991de5b9f78009c6d4cb34e18d20d028304a75d38528cddb45
wgsl-bevy-0.1.4  c96503dc3a7f1d17d954d2569c65143a9af03d40c6ad83f7ba5241a3965c60d0
glsl-0.2.0      df0ee57cbf4c2b35d3a7f17a01bf33c0ef2e85987cb85e516ac8c6904c4e62ac
asm-0.24.0      743a0377253cd5505841e61ba1093a04b3a949b2b751752ff956040dda14f1bc
```

The GLSL and assembly archives omit their license files. Exact upstream revision copies are retained under `third_party/licenses/tree-sitter-glsl-0.2.0/` and `third_party/licenses/tree-sitter-asm-0.24.0/`. Their respective license SHA-256 values are `f44d7107a33801260dc82624c91a71ebf5918b4038a0fb0e3ef60e2dd9ba1999` and `0e9f43573925c2e4b49f328a45c4df22c882191d77d876b0d18ecd60bd21e17c`.

## Qualification status

Original fixtures under `crates/airk-ingest/tests/fixtures/specialist-gpu/` exercise required written facts, Unicode/CRLF spans, malformed input, preprocessing negatives, unknown ISA behavior and shared budget exhaustion. Four private tests include the payload-binding owner regression. Five kernel tests cover cold/warm extraction, compatible moves, retained source equality, bounded outline/key reads, profile overrides, stage changes and ISA selection.

On 2026-10-14, the final Linux kernel run passed all five GPU/assembly integration tests after correction of GLSL sibling ownership. The earlier run passed four of five and exposed that defect; the regression now also covers the direct payload-binding path.

Windows passed all four GPU private tests, including the final direct owner-binding regression. The full ingest run passed 212 tests and failed an obsolete registry expectation that uppercase `.S` selected a different language; after correction, all seven focused registry tests passed. Windows ingest all-target clippy passed with warnings denied; final P12 kernel/platform/package qualification remains separate. These receipts establish the stated fixture coverage, not universal dialect support or complete cross-platform qualification.
