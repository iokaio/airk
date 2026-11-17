# Historical standalone patch source, r1

This maintained research collection accounts for the 1,359 patch/diff paths in the reviewed broad-delta native lane. It preserves authored patch bytes without applying them. Failed, superseded, partial and already-integrated proposals remain distinct historical inputs; none becomes a current implementation or qualification claim merely by preservation.

`rescue-map.json` records the exact delta receipt, original paths, lengths, raw hashes and dispositions. `source-provenance.json` is the standard map for authored copies and exact existing maintained patch owners. Identical patches share one digest-prefixed destination in `sources/`. The original path identifies the recipe/version and sibling layout. Sources with an exact reachable Git blob, if any, are identified by blob and anchor commit rather than duplicated. No line endings or patch content are changed.

`source-disposition.json` accounts separately for upstream dependency inputs. ORT 1.30 patches must match original full source inventories; ORT 1.22 patches are compared to retained upstream archive members, disclosing CRLF-only normalization when necessary. Flatbuffers, Protobuf and RE2 patches are compared to archive members after checking the original ORT dependency SHA1. Exact archive, metadata, license and source-inventory evidence pins accompany exclusions. These are upstream build inputs, not AIRK-authored implementations; they remain in their original provisioned source trees.

## Family purposes

| Original family | Purpose and limits |
|---|---|
| `advanced-split` native/service proposals and model runtime proposals | Service ownership, model catalog, calibration, native delivery and source-admission changes. Diffs retain their historical base assumptions and are not directly applicable to today's modules. |
| `final-plan` GPU, FP16, streaming and final-graph proposals | Loader, bounded execution, export/reference, provenance and graph metadata implementation iterations. Separate variant identities and original limits remain verbatim. |
| `branch-review-r1` and `qa-1.8.9/composed-review-*` | Combined and subsystem review diffs for native, docs, contracts, tools and client changes. Large combined patches intentionally overlap smaller review slices. |
| `v190` P6–P14 and post-P14 proposals | Platform, process custody, persistence, upgrade, retirement and read-parity implementation/control iterations. Historical failed corrections are preserved, not silently accepted. |
| Client/UI, shell and test patches across these families | Reproduction and regression changes for Admin/Workbench, command adapters, packaging and test harnesses. They can reference historical private output paths and must not be run/applied in place. |
| Upstream ORT and extracted dependency trees | Excluded from authored capture only with pinned inventories/archive/license evidence; exclusion never means deletion is safe. |

## Verification and use

Run from the repository root in the verified same-human execution shell:

```powershell
python -B -S scripts/arbiter/research/proposals/patch-history-r1/verify_patch_history.py --originals
```

This reads hashes and metadata only; it does not import archived code, apply a patch, invoke a compiler, run a model or write files. `--plan` rebuilds the read-only disposition proposal from the frozen broad delta, provisioned upstream archives and current maintained/Git source identities, printing JSON. The standard maps are retained records; future source changes require explicit reconciliation.

Before reviving a patch, inspect its original source/base context, owning maintained contract and current callers; isolate and review the intended change, recover required licensed private inputs separately, then run scoped tests under fresh same-user output/process custody. No materialization or automatic application command is supplied. Private corpora, training examples, credentials, weights, runtime state and binary receipts are not copied. Patch-header and credential-literal review found no such payload candidates in this captured lane; this is not a general claim about all artifacts.

Original headers and notices remain intact. Authored changes to upstream ORT retain their context; the [ORT MIT license](../native-client-r1/ORT-LICENSE.txt) and [third-party notices](../native-client-r1/ORT-ThirdPartyNotices.txt) accompany those source derivatives. Repository-authored implementation follows the repository license. Upstream exclusion rows identify their separate original license evidence.

Four read-only refusal controls check missing/duplicate source rows, unknown dispositions and the exact permitted newline normalization. Run python -B -S scripts/arbiter/research/proposals/patch-history-r1/test_verify_patch_history.py; no test fixture files or patch application occur.

The 412 upstream ORT/dependency patch records use the shared upstream_build_input role. Raw records bind the actual source SHA-256; CRLF-to-LF records bind the strict UTF-8, non-NUL source Git blob after only CRLF replacement. Original archive/inventory evidence, member hashes and historical representation observations remain unchanged. This metadata alignment does not apply patches or assert a historical build ran.
