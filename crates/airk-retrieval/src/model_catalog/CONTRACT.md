# Native model catalog and trained-v2 decision

Owner: `airk-retrieval`. Decision recorded 14 November 2026 before consumers.

The active `/2` catalog contains exactly the separately identified trained
`minilm-l6-v2-u8-v2` and `minilm-l6-v2-s8-v2` candidates. Their graph hashes,
tokenizer, retained notices, trained-checkpoint origin, explicit mixed-precision
policy and ONNX Runtime 1.30 pipeline identity are immutable. Six FFN output
matrices and embeddings remain FP32; 7,077,888 of 10,616,832 constant linear
parameters are quantized. This is not an all-linear-INT8 export. Signed weights
and zero points are an exact -128 recoding of the unsigned export; the standard
ONNX low7/high1 two-pass lowering avoids saturating AVX2 pair products.

The graph is derived trained content; the manifest's upstream source/revision
identifies its base, not a URL hosting these derived bytes. Active asset URLs
are null: assets are bundled or explicitly provisioned from authenticated local
build inputs. No fabricated upstream URL or implicit download is permitted.
The provisioner hashes the actual copied bytes before publishing a manifest.
Native full205, resource and installed-platform gates remain pending; empty
verified-platform lists must not be described as qualified shipment evidence.

`model_catalog_v1.json` retains exact original catalog bytes, profile IDs, graph
hashes and runtime 1.22.1. It is compatibility metadata only. Old object sets
remain readable as retained metadata after exact historical descriptor proof;
they cannot execute through the new encoder or advertise semantic readiness.
The existing explicit regeneration/rebuild workflow owns migration. Existing
airk-state/1, ModelObjectSet and I4 bytes do not change.


Lazy shared metadata uses the already authenticated ModelObjectSet legacy identity
and recognizes the descriptor's retained runtime/platform inference digest. It
must not regenerate the current runtime identity merely because the current
binary parses an older manifest. This read-only metadata path never opens
weights or admits historical execution; semantic upgrade/rebuild and active
product catalog fences remain unchanged.
