# Specialist business languages: focused receipt

L5.01–04 adapters are integrated after the P10 gate. Seven focused original business adapter tests passed on Windows, including native grammar compilation and the explicit structural free-COBOL fallback. All nine original kernel pipeline cases passed on Linux, including the explicit free-form COBOL override. Final P12 platform/package qualification remains separate. No compiler, runtime, copybook expansion or conditional-source evaluation is invoked.

## Dependencies

| Approved package | Archive SHA256 | Exact upstream |
| --- | --- | --- |
| [tree-sitter-fortran 0.6.0](https://static.crates.io/crates/tree-sitter-fortran/tree-sitter-fortran-0.6.0.crate) | `ea5fea41363a9b59666001dc26c4b356d4e164da27c6ce31145e00aba6bf9b16` | stadelmanma/tree-sitter-fortran `48593829df8a929bfe58528eedcf405171921005` |
| [tree-sitter-pascal 0.10.2](https://static.crates.io/crates/tree-sitter-pascal/tree-sitter-pascal-0.10.2.crate) | `adb51e9a57493fd237e4517566749f7f7453349261a72a427e5f11d3b34b72a8` | Isopod/tree-sitter-pascal `2f28b717be47cf592241e1b7bec3b2b906f59148` |
| [arborium-cobol 2.18.2](https://static.crates.io/crates/arborium-cobol/arborium-cobol-2.18.2.crate), alias tree-sitter-cobol | `f5f4f892443d18dfe484cb58e4365fc6e544e5372a1d48c8d6582f52dfabfc57` | zharinov/tree-sitter-cobol `befb226951b65585617f42ecbe526d00c124100c` |

Fortran ABI15 and COBOL ABI15 use C scanners; Pascal ABI14 has no scanner. All normal bindings use tree-sitter-language 0.1, compatible in metadata with pinned runtime 0.25.10. Pascal explicitly uses C11; Fortran/COBOL use cc default C with supported warning flags. COBOL's Rust wrapper requires Rust 1.85/edition 2024 and exports `language()`. None of these observations substitutes for Windows/Linux native build and fixture qualification.

Pascal includes MIT text. Fortran and COBOL omit texts from their archives, so bundle:

- [Fortran exact MIT](https://raw.githubusercontent.com/stadelmanma/tree-sitter-fortran/48593829df8a929bfe58528eedcf405171921005/LICENSE), SHA256 `c0fc02a86ae2b179c4729694c523bbf42da674e5136ed8e12e3cb2f26556ea9f`.
- [COBOL grammar exact MIT](https://raw.githubusercontent.com/zharinov/tree-sitter-cobol/befb226951b65585617f42ecbe526d00c124100c/LICENSE), SHA256 `d724405ce238a22c0d35769c5a36b386ad5958192efe8bbb304fb2896254575f`.
- [Arborium wrapper MIT at v2.18.2](https://raw.githubusercontent.com/bearcove/arborium/v2.18.2/LICENSE-MIT), SHA256 `b11bd86bfb5de2e6e2fc1ff69951ad1d693a3390f7e49704263a508540681bfb`.

Rejected candidates: tree-sitter-cobol 0.1.0 contains a placeholder binary, not a parser; tree-sitter-delphi 0.1.0 omits its license and its declared upstream revision's license URL returns 404. arborium-vb 2.18.2 has compatible ABI15 metadata, but the exact advertised CodeAnt-AI/tree-sitter-vb-dotnet revision has no license file in its recursive Git tree despite MIT metadata. AIRK therefore uses an original bounded structural VB.NET adapter, not that grammar or a VB6 substitute.

## Prepared boundaries

`specialist_business::analyze` and `analyze_budgeted` accept `fortran-free`, `fortran-fixed`, `cobol-free`, `cobol-fixed`, `delphi`, `vbnet`. Source-form choices belong in effective profile identity. Proposed defaults `.f` fixed, `.f90/.f95` free and COBOL fixed remain conditional on actual original-byte fixtures. The native parsers receive unmodified source. Fixed-form comment lines are excluded; unsupported continuation indicators and nonblank sequence/tail columns produce partial coverage. No source normalization or offset guessing is used.

Fortran selects programs/modules/functions/subroutines, written use/include, declarations and call spellings. Fixed-column COBOL selects native program names/divisions/sections/paragraphs/data entries/COPY/written CALL targets. The admitted scanner hardcodes fixed columns and cannot parse free source reliably. Explicit `cobol-free` therefore uses an original bounded structural token recognizer for those written forms, with no padding, normalization or grammar-continuity claim. It preserves quoted literals and `*>` comment boundaries; unsupported literal continuations fail closed. Delphi selects units/programs/types/routines/members/uses/calls. Its pinned grammar has no package node, so a separate bounded original token recognizer emits package/requires/contains forms with structural provenance. Comments and quoted strings are token boundaries; dependencies are not loaded.

VB.NET recognizes selected Imports/Namespace/type/module/Sub/Function/Property line forms and written receiver calls. Apostrophe/REM comments and doubled-quote string contents cannot create declarations; unterminated strings fail closed. XML/attribute-like constructs cause conservative remainder omission. Continuations, conditional directives, attributes and compiler binding remain explicitly partial. Structural facts carry no declaration fingerprints or resolved owner IDs.

Native producer `airk-tree-sitter/<profile>` and structural producers `airk-business-structure/vbnet`, `airk-business-structure/cobol-free` and `airk-business-structure/delphi` use `specialist-facts/1`. Structural producers need explicit allowlist admission. Common ceilings are 4 MiB structured source (larger UTF-8 sources use [partial text-only extraction](ingestion-limits.md)), 500,000 nodes/tokens, 10,000 facts, 64 MiB fingerprint work, bounded display details and 128 owner/block depth. Child APIs share HostBudget; callers admit original/map bytes once.

Nine original UTF-8 source fixtures, seven private tests and nine kernel tests are authored. All seven business unit tests passed on Windows with `cargo test -p airk-ingest --lib specialist_business:: --locked -j1`. The free-form fixture first exposed the fixed-column scanner limitation; its structural replacement and literal/comment negatives now pass. Linux passed all nine business kernel pipeline cases, including explicit free-COBOL selection, retained original bytes, bounded queries, warm/move reuse and profile invalidation. Final P12 platform/package qualification remains pending. Capability, source-form and continuity claims require those tests and native gates; unknown or structural dimensions must not be promoted to grammar-complete or compiler semantics.
