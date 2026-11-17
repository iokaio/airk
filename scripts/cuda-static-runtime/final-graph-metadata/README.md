# Final ONNX Runtime graph metadata collector

This folder owns AIRK's application-private C++ diagnostic for observing the
final optimized graph in an initialized ONNX Runtime session. The
[collector patch](collector.patch) adds `airk_ort_collect_final_graph_metadata_v1`
to the same translation unit that owns the session C API. Its safety and record
requirements are in [CONTRACT.md](CONTRACT.md).

The collector reports final GPU/CPU assignment, graph/node/value identities,
ordered connections, tensor types and dimensions, attributes, and bounded small
CPU-resident integer/bool constants. It helps determine whether CPU-assigned
nodes are bounded shape/control calculations before a separately identified
hybrid execution policy can be considered. Operator names alone are insufficient.
It does not run inference, copy model weights, read GPU tensor contents, enable
product CPU fallback, or qualify a model.

## Build use

Apply this additive patch to the authenticated AIRK ONNX Runtime 1.30.0 C API
source that already contains the reviewed final-placement collector. The exact
base SHA256 is `7acabdbefcd7ee3936305ff150fc2369c3d341b6838e10113435ebab03378ab8`.
The reviewed resulting source SHA256 is
`db00479a06d0a1e8fadf4cc834ffe5a6da0b745ee2820b06a0821fb1dbeca97b`.
This is not a patch for an arbitrary upstream checkout. The original translation
unit, including the placement collector, must remain byte-identical as a prefix.

The incremental producer is responsible for checking these inputs, compiling a
fresh object, and deriving fresh session and aggregate archives from authenticated
unchanged inputs. The new archive must receive a new producer identity. Never
apply this patch in place to retained evidence or label the result with its
parent build's identity. Generated source copies, objects, libraries, private
machine recipes and execution receipts stay outside this source folder.

## Integration status

As of 17 November 2026, the C++ source has passed primary and independent source
review, including bounds, lifetime and x64 ABI checks. It has not yet been
compiled or exercised. Rust decoding, diagnostic integration and the maintained
incremental producer are being integrated separately. Their required behavior
is documented in the contract; this README does not claim those checks passed.
The build still depends on the authenticated parent source and build inputs;
this folder is not a complete standalone ONNX Runtime source distribution.
