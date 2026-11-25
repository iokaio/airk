# Native MiniLM catalog

The active profile is `minilm-l6-v2-fp16-v1`, selected on supported CPUs.
It authenticates one shared ONNX weight asset with independent CPU FP32 and
CUDA FP16 graph views. Catalog and unpacked asset inventory are exact, and
legacy manifests retain their original embedding descriptors for readiness
and regeneration. GPU execution and its actual native observations belong
to the [MiniLM embedding contract](../minilm_fp16/CONTRACT.md).

## Historical original-baseline policy

Owner: airk-retrieval. Root recorded the original-baseline retention decision
after the preselected epoch1 candidate and both original unsigned controls tied
on the complete matched DEV suite. No epoch1 promotion or further model bakeoff
is selected. The former checkpoint515… trained-v2 catalog was unqualified and
is not evidence of a qualified retained baseline.

The preceding schema2 catalog contained exactly minilm-l6-v2-u8-v3 and
minilm-l6-v2-s8-v3. They restore the exact original publisher U8 and S8 graphs,
tokenizer, OnnxManifest ABI and Apache license bytes from the retained native
v1 catalog. Graph hashes are b941bf19…0fd21 and4278337f…02474. Precision is
uint8/int8 and quantization is absent, rather than a false trained/mixed policy.
The existing ORT1.30 provider implementation identity/options are unchanged;
its historical opaque name is not model training provenance. Profile IDs are
catalog identities at that time; weights and runtime were separately authenticated.

Only the native Rust owner constructs/captures the ModelCatalog, OnnxManifest
and airk.minilm.native-policy/2 files. Actual original asset hashes/lengths and
base metadata are strict. Assets use null URLs and authenticated local/offline
provisioning; no implicit runtime download is introduced. CPU selection remains
unsigned for ordinary x86_64 and signed for VNNI x86_64/aarch64, with serial B1
per-text dynamic activation semantics. Actual signed/platform execution must be
qualified independently; a graph filename or architecture observation is not a
platform qualification result.

Qualification begins with verified_platforms empty. Exact original U8 native
DEV and numerical observations remain their original scoped evidence. Task,
signed-profile numerical, general-document/language, resource, regeneration,
installed-platform/package/offline and final release gates are separate. Neither
Ready backend state nor source/catalog restoration populates platform proof.
The pure draft emitter executes no inference and activates no product model.

model_catalog_v1.json remains exact historical metadata under its original
ORT1.22.1 identity. Restoring its graph bytes does not relabel old vectors as
current runtime output. Authenticated old model references remain readable for
retirement/regeneration, while opening/reusing their graph requires the exact
current pipeline. Existing airk-state/1, ModelObjectSet and I4 bytes do not change.

Upgrade detection checks the complete current active manifest, not only the ORT
version. Both active precisions remain valid independently of current CPU default
for retained metadata. New product setup continues to enforce the exact CPU
selection. Retired same-runtime weights must become upgrade-pending before any
semantic query or incremental update can mix old/new vectors or cache keys.
Bounded authenticated ModelJson metadata supplies this check without reading
weights, tokenizer, graph or live sources. The existing actual native owner and
exact thread-local sealed-manifest lease remain the only private evaluation
exception; no serialized role or environment value authorizes execution.

For a genuinely missing retired shared ModelJson, the ordinary metadata path
first exposes an io::NotFound chain. Only a captured set with a still-verifiable
matching runtime owner and graph/tokenizer/license references that cannot match
either active profile may then be classified Pending. This refusal-only fallback
keeps lexical evidence available. Missing possibly-active assets, corrupt or
hash-changed metadata, aliases, foreign owners and other failures stay errors.

The owning service ModelSetup/Kernel upgrade path atomically replaces retired
graph/cache using retained extracted evidence, preserving source bytes, original
locations/text/facts and publication authority. Bounded vector turns resume
recomputation under the new pipeline. Backend availability, data coverage and
release qualification remain distinct; setup completion is not semantic Ready.
Full source rebuild/replacement uses the existing reviewed maintenance owner
when retained evidence is unavailable, without a silent destructive fallback.

Before CAL/A6, the actual native Service VectorDraft emitter binds the selected
original profile's inspect_object_plan pipeline, current runtime/model metadata,
source/index/publication scope and readiness. Fitted genericRead/calibration and
all four A6 conditions bind that same chosen MiniLM plus actual Focus/Context
profiles and frozen data/rubric/source/budget closure. Changed-pipeline calibration
does not transfer, and no old FINAL205 experiment is reopened.


An authenticated active ONNX manifest also requires the captured graph, tokenizer
and license reference lengths and digests to match that same active catalog
closure. A contradictory declared length is a metadata error, not Ready or a
retired-model Pending result. This comparison reads only bounded authenticated
ModelJson and compiled catalog metadata; it never opens model/tokenizer/license
payloads. The existing exact owner/thread-local evaluation lease remains the
separate private exception. Independent controls use the owning metadata planner
and shared-set sealer to recompute legitimate set identities with wrong neural
lengths; metadata descriptor equality alone cannot establish active eligibility.

Native component qualification fixtures retain an actual Store ServiceLease and
its admitted compute scope through every model call and session teardown. Store
is a test-only development dependency; production crate edges are unchanged. The
shared test guard creates fresh private control storage, grants no serialized or
environment execution role, and releases the thread scope before its native lease.
The existing parallel/cache, INT8 normalization and tail-retrieval cases retain
their identities and complete original assertions. The all-profile INT8 case
uses the provisioner's AIRK_BUNDLED_MODEL_DIR root when explicitly supplied;
ordinary asset/catalog/ABI verification and the existing fallback remain intact.
