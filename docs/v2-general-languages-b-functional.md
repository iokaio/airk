# General languages B: functional batch

L4.12–15 uses original fixtures and bounded adapters. Native grammar compilation and seven original adapter unit tests passed on Windows; all nine functional kernel pipeline cases passed on Linux, including exact mapped literate spans. No compiler, package manager, preprocessor, script runtime, Template Haskell, or macro evaluation is invoked.

## Dependency admission receipt

All four official archives include MIT license text. Every generated parser has ABI15, within the pinned tree-sitter 0.25.10 range. All use C11 parsers and C scanners through `cc`, with conditional MSVC UTF-8 flags and normal `tree-sitter-language = 0.1` bindings. Newer dev-only tree-sitter requirements do not upgrade the consumer runtime. Windows native compilation and adapter fixtures subsequently passed; metadata compatibility alone was not used as a qualification receipt.

| Exact archive | SHA256 | Upstream revision |
| --- | --- | --- |
| [tree-sitter-ocaml 0.26.0](https://static.crates.io/crates/tree-sitter-ocaml/tree-sitter-ocaml-0.26.0.crate) | `43adb10fcccbcbbce0f4adc9dab7cd55f0a5eafc2d71bd4c03fcc5d6767066e0` | `e3c9cf368f68bffd2f81188229aefa7b434eda65` |
| [tree-sitter-haskell 0.24.1](https://static.crates.io/crates/tree-sitter-haskell/tree-sitter-haskell-0.24.1.crate) | `781de30e0d1ec8e6db4ed799d36ce1cedaf4684374dada45ebe2a8ea44c3f814` | `97288e585b0bd44199720280d067fa35bacd81ff` |
| [tree-sitter-erlang 0.21.0](https://static.crates.io/crates/tree-sitter-erlang/tree-sitter-erlang-0.21.0.crate) | `8079a9cfe733c3b6749bb3c683c0e997ee68a2305f7a210800bbbc53e82d3f64` | `0386a11c835950f5e1ebfbcbf49863d8495de4e0` |
| [tree-sitter-fsharp 0.3.12](https://static.crates.io/crates/tree-sitter-fsharp/tree-sitter-fsharp-0.3.12.crate) | `87affbae0d85926cebaa1abb18660904540708ab2c9ed7547f4809e6188125db` | `aefd0c8741bdf3aeb827a228aa4a996a6536697e` |

Upstreams: [OCaml](https://github.com/tree-sitter/tree-sitter-ocaml), [Haskell](https://github.com/tree-sitter/tree-sitter-haskell), [Erlang](https://github.com/WhatsApp/tree-sitter-erlang), [F#](https://github.com/ionide/tree-sitter-fsharp). OCaml bundles separate implementation, interface and type parsers; AIRK uses implementation/interface. F# bundles implementation and signature parsers; `.fsi` must select the signature profile explicitly.

## Extraction boundary

`languages_b_functional::analyze(path, text, adapter)` and `analyze_budgeted(path, text, adapter, budget)` support `ocaml`, `ocaml-interface`, `haskell`, `erlang`, `fsharp`, `fsharp-signature`. Native producer IDs are `airk-tree-sitter/<adapter>`, version `general-b-facts/1`. Selected facts retain original byte spans and written names; call targets are candidates only. Interface signatures are distinct evidence from implementation bodies. Erlang macro/include forms remain written syntax, never expansion results.

Structured source limits are 4 MiB (larger UTF-8 sources use [partial text-only extraction](ingestion-limits.md)), 500,000 visited nodes, 10,000 facts, 64 MiB shared fingerprint work, 128 owner depth and 16 MiB serialized evidence. The shared-budget API does not readmit source bytes; the host admits child text/map bytes once. Native declarations use common name/qualified-name/signature/body domains when complete source information exists. Multiple body children remain unknown rather than incorrectly hashing one child as the complete body. Unknown documentation dimensions remain unknown. Duplicate clauses and overloads do not imply automatic continuity.

`literate::discover(path, text, budget)` extracts `haskell-literate` hosts with Haskell child regions. Consecutive Bird lines remove only a literal leading `>` and one optional ASCII space, with one verbatim source-map segment per retained original line. Newlines, including CRLF, are retained; no generated separator or wrapper is introduced. LaTeX code environments retain direct original body spans. Whole prefixes/delimiters are excluded from Markdown prose, while outside prose remains available. Every empty/failed attempt counts against the shared region cap. Unterminated environments are omitted with partial diagnostics. Distinct regions have independent parsing scopes; AIRK does not claim a combined literate compiler model.

Nine original UTF-8 fixtures cover `.ml`, `.mli`, `.hs`, `.lhs`, `.erl`, `.hrl`, `.fs`, `.fsi`, and `.fsx`. Seven private tests and nine active kernel tests passed in their respective Windows unit and Linux integration gates. Additional original required-fact fixtures cover OCaml exceptions/module types/include, Haskell classes/instances/type aliases/signatures, and F# types/members/nested modules. Linux pipeline assertions cover original retained bytes, warm reuse, compatible moves, bounded native key queries where qualified, and isolated profile invalidation.

## Focused validation receipt

All 18 P10 adapter unit tests passed on Windows, including the required-fact correction; all 19 P10 kernel pipeline cases passed on Linux. The earlier Linux ingest suite passed 186 tests with one helper ignored. No macOS or final P12 platform qualification is implied. Source-form limits, unknown fingerprint dimensions and unresolved typeclass/interface relationships remain unchanged.
