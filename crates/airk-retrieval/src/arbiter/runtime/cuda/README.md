# Native CUDA runtime

This component owns authenticated CUDA runtime loading, device custody, resource
claims, provider setup and final execution placement. Its current executable
policy requires strict GPU placement; a serialized policy or diagnostic report
never grants execution authority. See [CONTRACT.md](CONTRACT.md).

`windows.rs` retains authenticated files and verifies mapped modules. `modules.rs`
classifies exact module identities. Device and session custody retain resources
when native settlement is uncertain; `../session_owner.rs` owns session teardown.

`placement.rs` observes the final same-core SessionState provider map.
`graph_metadata.rs` additionally records bounded final graph IDs, ordered edges,
types, attributes and small CPU-resident integer constants. Its callbacks never
copy model weights or read GPU tensors. Unsupported values and outer bindings
remain explicit, and consumers derive from the complete observed edge inventory.

`diagnostic.rs` exposes observation-only test-support paths with actual teardown
proof. The graph-only path discloses capped-log truncation independently of graph
completeness. Existing placement diagnostics retain their log-completeness check.
Neither path returns a Scorer, dispatches a forward, or qualifies a hybrid policy.
Pure semantic fixtures live beside each owner. Runtime admission and qualification
require separate authenticated native evidence under the original resource limits.
