# V2 location and budget contract, revision 1

Phase 1 contract for A1/A2; implemented in [location.rs](../crates/airk-core/src/location.rs), with [adversarial fixtures](../crates/airk-core/tests/location_adversarial.rs). Runtime adapter conversion remains phase 2; composite support remains phase 8. Existing `DocumentLocation` serialization is unchanged.

`ByteSpan` uses half-open UTF-8 byte offsets. `SourceMap` version 1 covers every decoded byte with ordered, nonoverlapping segments. Verbatim segments must contain identical local/host bytes. Decoded segments identify an entire encoded unit; partial selections cannot claim exact coordinates within that unit. Unmapped segments represent synthetic or unsupported text. An empty selection has no exact mapping. Empty text accepts an empty map.

Mapped host ranges must be ordered, nonempty, UTF-8 aligned and nonoverlapping. Host gaps remain separate spans: JSON array separators, quotes and dedenting gaps must not be disguised as decoded source. `original_spans` validates the complete map before returning exact pieces. Decoders remain responsible for proving their transformation; a structurally valid decoded map alone cannot prove decoder correctness. Unsupported mappings retain diagnostics and broader original locations in adapter output.

`RegionLocation` represents embedded spans or notebook cells without temporary filenames. A notebook position is a current one-based locator, separate from an optional written cell ID. It is not a continuity identity. Original spans refer to the original host bytes; cell-local lines refer to decoded text. Existing page/table/sheet/slide locations remain intrinsic document evidence.

One non-cloneable `HostBudget` is passed by mutable reference through nested adapters. It caps 256 region attempts, depth 4, 100,000 map segments, 500,000 syntax nodes and 10,000 facts. Native source text can use at most 16 MiB; document adapters must supply their stricter 4 MiB extraction allowance. Policies can lower allowances. Failed region attempts and failed resource charges consume allowance; retrying with a fresh child budget is forbidden. Exhaustion must become an explicit partial/omission result at integration. These are work counters, not wall-time preemption.

The tests cover escaped JSON, noncontiguous arrays, Unicode/CRLF, synthetic text, malformed spans, source ordering and shared budget exhaustion. This contract does not yet qualify any composite adapter or notebook parser.
