# Native metadata projection

The [projection owner](../projection.rs) consumes authenticated private producer, aggregate, source and legal metadata through the native held-source interface. [strict.rs](strict.rs) rejects duplicate JSON keys; [checkpoint.rs](checkpoint.rs) authenticates the original bounded checkpoint ancestry under its [contract](checkpoint/CONTRACT.md). [incremental.rs](incremental.rs) authenticates a fresh one-object core derivation without relabeling the historical parent. [tests.rs](tests.rs) exercises projection and refusal behavior; [incremental controls](incremental/README.md) cover its distinct boundaries.

The [parent contract](../CONTRACT.md) owns public semantics. The checkpoint UTC parser is shared by visibility only; checkpoint receipt authority and historical hashes remain unchanged. Metadata custody is supplied by the caller and retained through its original deadline. No projection creates a device, execution permit or qualification.

Validation uses the scoped Clippy and provenance Nextest commands in the [parent README](../README.md), with actual selected counts. Incremental source is maintained here but remains uncompiled/unexecuted pending combined review; its reviewed recipe digest is intentionally unavailable until the genuine producer freeze.
