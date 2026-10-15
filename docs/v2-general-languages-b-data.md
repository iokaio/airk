# General languages B: data-oriented batch

These adapters implement bounded selected evidence for L4.08–11. The installed grammars compiled and five original adapter unit tests passed on Windows; all five data-batch kernel pipeline cases passed on Linux. No execution, macro expansion, import resolution, Nix evaluation, Julia dispatch, Zig comptime evaluation, or Clojure reader feature selection is performed.

## Dependency evidence

Official crate archives were inspected in memory; no upstream implementation or test fixtures were copied into AIRK. All generated ABIs are within the pinned tree-sitter 0.25.10 runtime's accepted 13–15 range. Windows native grammar builds and original adapter tests passed; final P12 platform qualification remains pending.

| Exact crate | Archive SHA256 | Upstream revision | ABI/build/license |
| --- | --- | --- | --- |
| tree-sitter-nix 0.3.0 | `4952a9733f3a98f6683a0ccd1035d84ab7a52f7e84eeed58548d86765ad92de3` | `ea1d87f7996be1329ef6555dcacfa63a69bd55c6` | ABI13, C parser/scanner; MIT included |
| tree-sitter-zig 1.1.2 | `ab11fc124851b0db4dd5e55983bbd9631192e93238389dcd44521715e5d53e28` | `b670c8df85a1568f498aa5c8cae42f51a90473c0` | ABI14, C11 parser, no scanner; MIT metadata |
| tree-sitter-julia 0.23.1 | `4144731a178812ee867619b1e98b3b91e54c1652304b26e5ebe3175b701de323` | `a8e1262997d5a45520a06cbe1b86c0737d507054` | ABI14, C11 parser/scanner; MIT metadata |
| tree-sitter-clojure 0.1.0 | `4004884cc509449a1d78fa3e1f02b4e953d0a8065984445304795e72e885338c` | `6248a3f3d3c76ec7d33c52e7553761facdcb4ab5` | ABI14, C parser, no scanner; MIT wrapper metadata plus bundled CC0-1.0 grammar |

Archives: [Nix](https://static.crates.io/crates/tree-sitter-nix/tree-sitter-nix-0.3.0.crate), [Zig](https://static.crates.io/crates/tree-sitter-zig/tree-sitter-zig-1.1.2.crate), [Julia](https://static.crates.io/crates/tree-sitter-julia/tree-sitter-julia-0.23.1.crate), [Clojure](https://static.crates.io/crates/tree-sitter-clojure/tree-sitter-clojure-0.1.0.crate).

Zig and Julia archives omit their license files. Bundle the exact upstream texts, preserving notices:

- [Zig LICENSE](https://raw.githubusercontent.com/tree-sitter-grammars/tree-sitter-zig/b670c8df85a1568f498aa5c8cae42f51a90473c0/LICENSE), SHA256 `0eea8dc45e89deeb03c7799bbbc7b4688f365fb274562f4540ecfebdea82e727`.
- [Julia LICENSE](https://raw.githubusercontent.com/tree-sitter/tree-sitter-julia/a8e1262997d5a45520a06cbe1b86c0737d507054/LICENSE), SHA256 `74f800107f200f4e28da33e9c3f86230d37d8f4c0a4916f7111e800c395ee3a2`.
- Clojure's archive `grammar-src/COPYING.txt` is CC0-1.0, SHA256 `a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499`. Its `.gitmodules` identifies the bundled grammar as `https://github.com/sogaiu/tree-sitter-clojure.git`. Preserve this separate notice alongside wrapper MIT metadata; do not relabel the grammar MIT. The archive digest pins the bundled grammar bytes even though wrapper VCS metadata does not identify the submodule commit.

Nix/Zig/Julia use tree-sitter-language 0.1 bindings. Clojure additionally depends on tree-sitter ^0.25.6, compatible with pinned 0.25.10, and uses Rust edition 2024. Zig/Julia C11 builds conditionally add the MSVC UTF-8 flag. Nix/Clojure use portable cc builds with compiler-supported warning flags; native Windows compilation passed with the pinned runtime.

## Adapter contracts and limits

`languages_b_data::analyze(path, text, adapter)` creates a fresh host allowance. `analyze_budgeted(path, text, adapter, budget)` shares node, fact and fingerprint allowances across host children; callers admit source/map bytes once. IDs: `nix`, `zig`, `julia`, `clojure`, `edn`. The common limits are 16 MiB source, 500,000 visited nodes, 10,000 facts, 64 MiB fingerprint work, 128 owner/context depth, bounded display details and 16 MiB serialized evidence. Exhaustion fails closed.

Native syntax facts use `airk-tree-sitter/<adapter>` with `general-b-facts/1`. Clojure macro-form interpretation and EDN data cues use `airk-language-structure/<adapter>` with the same version; these require explicit producer admission. They have no declaration fingerprints and remain partial. Clojure reader branches carry written `reader:<feature>` scope context without selecting a platform. Quoted, discarded, evaluated and tagged forms are not interpreted. EDN list values never become call/declaration evidence.

Nix bindings, named lambdas, applications and written import arguments are selected. Zig functions, container-valued declarations, variables, tests and builtins are selected. Julia modules, long-form declarations, selected short-form methods, import/using/export and call spellings are selected. Julia multi-expression bodies remain unknown fingerprints rather than hashing one expression as the whole body; macro expansion and documentation continuity remain unsupported. Native declaration names/signatures use the common fingerprint domains where complete source spans are available.

Registered default suffixes are `.nix`, `.zig`, `.jl`, `.clj`, `.cljs`, `.cljc`, and separate `.edn`. No unregistered suffix inference is needed. Exact/scoped policy overrides retain existing precedence. The original fixtures cover CRLF/Unicode positions, quoted/EDN negatives, reader contexts, warm reuse, moves, and unrelated-file isolation during profile overrides. The original AST assertions passed on Windows. Linux integration verified retained original bytes, warm and compatible-move reuse, bounded native key lookup where qualified, and policy invalidation without unrelated-file reparsing. Structural Clojure/EDN facts do not acquire native declaration continuity.

## Focused validation receipt

The complete P10 focused run passed 18 Windows adapter tests (6 first-batch, 5 data, 7 functional) and 19 Linux kernel pipeline tests (5/5/9). An earlier Linux ingest run passed 186 tests with one helper ignored. These receipts do not claim macOS validation or completion of the final P12 release/platform gate.
