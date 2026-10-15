# Hardware and contract evidence: L5.09–11

These adapters retain selected original written syntax for Solidity, VHDL and Verilog/SystemVerilog. They do not execute contracts, analyze security, elaborate hardware, simulate designs, expand macros or resolve references.

| Profile | Selection | Bounded evidence |
| --- | --- | --- |
| `solidity` | `.sol` | Contracts/interfaces/libraries/structs/functions, events/errors, imports, written inheritance and call candidates |
| `vhdl` | `.vhd`, `.vhdl` | Entities/architectures/packages/components, named processes, ports/signals, libraries/use clauses and written instantiations |
| `verilog`, `systemverilog` | Qualified `.v`; `.sv`, `.svh`; explicit policy | Modules/interfaces/packages/classes/programs, functions/tasks, ports/signals, includes and written instantiations |

Ambiguous `.v` requires an explicit override or a strong leading module/macromodule/primitive header within 4,096 captured bytes. The discriminator skips comments and requires a complete header boundary; it does not interpret arbitrary files with that suffix as Verilog. Compiler directives, macros, generate constructs and Solidity Yul make extraction partial. Comments, literals and macro bodies do not produce invented declarations.

## Dependencies and provenance

| Exact archive | SHA256 | Upstream revision/native requirements |
| --- | --- | --- |
| [tree-sitter-solidity 1.2.13](https://static.crates.io/crates/tree-sitter-solidity/tree-sitter-solidity-1.2.13.crate) | `4eacf8875b70879f0cb670c60b233ad0b68752d9e1474e6c3ef168eea8a90b25` | JoranHonig/tree-sitter-solidity `4e938a46c7030dd001bc99e1ac0f0c750ac98254`; ABI15, C parser, no scanner |
| [tree-sitter-vhdl 1.4.0](https://static.crates.io/crates/tree-sitter-vhdl/tree-sitter-vhdl-1.4.0.crate) | `0feb025ddf87c45bb98335ef2a2493cac08ec6b74d33201d1bf75df2be08de23` | jpt13653903/tree-sitter-vhdl `889c521757471093ce5ac83502ca8a89e516404e`; ABI15, C parser/scanner with packaged headers |
| [tree-sitter-verilog 1.0.3](https://static.crates.io/crates/tree-sitter-verilog/tree-sitter-verilog-1.0.3.crate) | `d4e7e0360395852f1f6ff5b7b82c72dc6557d181073188df1d60ec469ea69c66` | tree-sitter/tree-sitter-verilog `521b535e41a5acd2c6539a922d4649bbe8275110`; ABI14, C parser, no scanner |

All three are MIT and expose tree-sitter-language 0.1 bindings compatible with the pinned 0.25.10 runtime. Their native builds include MSVC UTF-8 handling. Verilog's archive omits its license text; the notice bundle preserves the [exact upstream MIT license](https://raw.githubusercontent.com/tree-sitter/tree-sitter-verilog/521b535e41a5acd2c6539a922d4649bbe8275110/LICENSE), SHA256 `8d2ff8d1e308828099436c43dc8bfbb8b6f368ad2a012ab9802a80ca22b03b53`.

## Evidence and limits

Facts use original byte/line locations and `airk-tree-sitter/<profile>` syntax provenance, version `specialist-hardware-facts/1`. Native declaration fingerprints use full names and exact signatures/bodies where available; unsupported body/documentation dimensions stay unknown. Neither native keys nor written targets imply semantic or elaborated identities.

Sources are limited to 16 MiB, shared traversal to 500,000 nodes, facts to 10,000, fingerprint work to 64 MiB, owner depth to 128 and serialized facts to 16 MiB. Pending traversal queues and display details are separately bounded. Child parsing shares host work allowances; it does not reset them per region. Exhaustion fails closed, while malformed/unsupported syntax remains partial.

## Focused qualification

Windows native compilation and all three original hardware unit tests passed. They cover meaningful row facts, CRLF/non-BMP locations, ambiguous selection, shared budgets and negative comment/literal/malformed cases. All four Linux kernel pipeline cases passed, covering retained original bytes, bounded queries, native keys where qualified, warm and compatible-move reuse, and profile invalidation without unrelated reparsing. Final P12 platform/package qualification remains pending; these receipts do not establish complete compiler support or native macOS qualification. GPU/assembly profiles are separate rows and are not covered by this receipt.
