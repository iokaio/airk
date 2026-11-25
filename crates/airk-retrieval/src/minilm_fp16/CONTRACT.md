# MiniLM FP16 native embedding

The approved MiniLM asset is ordinary ONNX. The top-level CPU graph runs with
FP32 arithmetic after exact widening of its FP16 learned tensor initializers.
The existing CPU adapter and pinned model catalog own that route. The
authenticated file's standard ONNX metadata carries one bounded alternative
graph at airk.cuda_graph.onnx.base64. Its FP16 learned tensors reference exact
byte ranges in the canonical asset under the logical name minilm-fp16.bin.
No external path is opened. The native loader authenticates the complete asset
before extracting metadata and checks every external data range.

Both routes bind the same tokenizer, BERT input truncation, padding, mask and
384-dimensional L2-normalized masked-mean readout. CUDA batches up to 32 texts
with one shared token width drawn from 32, 64, 128 and 256. Output order matches
input order. Tests compare tokenizer arrays against the established CPU adapter
for an independently selected set. The FP16 GPU arithmetic and FP32 CPU
arithmetic have distinct numeric tolerances under one approved model identity.

The owning native Inference lane acquires the existing authenticated CUDA
runtime and adapter with its actual ServiceLease and original operation
deadline. Retrieval requires that compute role. A product open requires a
compiled AIRK_APPROVED_MINILM_CUDA_QUALIFICATION row matching the model asset,
alternative graph, native runtime manifest, actual adapter UUID and signed
driver image/API. The row also binds the complete final native graph assignment
digest and reviewed cold, warm, headroom and host capacity requirements.
Missing or differing evidence refuses the GPU route. CUDA provider setup is
error-on-failure and disables TF32 and CUDA Graphs. The complete final
SessionState assignment must match its compiled digest. The approved graph
allows exactly 46 root ONNX nodes on CPU, each with its reviewed name, domain,
operator and subgraph. Every other final node must use CUDA. Neither the
configured provider list nor CUDA DLL availability establishes placement or
execution.

The A6000 receipt from the first native probe recorded 210 CUDA nodes and 46
CPU shape controls with the exact graph SHA
8d6ee9ccbeeb7a2c9a5dca313cb7b63ba7eb860fdfeab00bed7cc2fb3056bcb4.
Strict ONNX shape and element inference on that same graph independently
matched all 46 observed nodes and reported 156 INT64 and two BOOL incident
tensor slots. They are scalar or at most four elements. The two symbolic
lengths in Equal and Where are driven by ConstantOfShape using the shape of
the length-four Reshape_2 result, with scalar multiplication preserving the
length. The CPU nodes neither consume nor produce floating activations or
learned weights. A different graph SHA requires a new shape-control review.

Each accepted batch reserves warm capacity and keeps exact tensor transfers
and preallocated output until the actual native device synchronizes. The
original deadline also governs result delivery. CudaBatchFailure identifies
whether device work settled so the owner can enforce safe CPU fallback with
the original budget. Session teardown synchronizes before and after dropping
the native session. Unknown settlement poisons the runtime and retains the
native device, service lease and affected session or transfer resources.

The Engine checks the compiled row and its host allowance before loading the
alternate model view or opening native runtime files. It requests the row's
one explicit adapter; admission checks the actual live driver and device
rather than treating compile-time identity as a device witness. An unknown
load or teardown result uses the exact held device's poison observation as
well as process poison to refuse CPU fallback.

A Windows x64 test-support development image can instead compile the separate
AIRK_APPROVED_MINILM_CUDA_DEVELOPMENT row for an exact authenticated Scientific
/1 runtime and the pinned A6000 development fixture. It has the same graph,
placement, driver, capacity and settlement requirements. Product and
development rows are mutually exclusive. The development row is limited to a
private campaign image and carries no product /2 or Arbiter qualification.

Private test-support can observe placement from the authenticated runtime before
qualification. The diagnostic observation only returns placement data. The
native exercise requires the exact independently retained placement digest,
shape-control policy, and caller-declared measured or bounded capacity. The
probe supplies private evidence and grants no product eligibility.
