# Arbiter Hugging Face mirror

These release tools verify and publish an explicitly reviewed public payload
to `iokaio/airk-arbiter-models`. The compiled Rust catalog and existing
promotion decision still own installation and release authority. The payload
directory is a checked, flat distribution staging directory. Prepare it from
approved bytes, separate from private CAL, corpus, source or tool output. Keep
the inventory outside the payload directory.

## Prepare and verify the inventory

Use a checked Python from the repository root. Names are shared on both
hosts. Generate a new name when bytes change. Transport part names leave the
actual ONNX graph and installed external-data location intact.

```powershell
python -I -B -S scripts/arbiter/distribution/arbiter_mirror.py name --slot context-compact --upstream qwen3-1.7b --variant windows-cuda-fp16 --revision 1 --suffix model.onnx.data.part001
```

Create a private JSON inventory with schema
`airk.private.arbiter-mirror-inventory/1`, repo `iokaio/airk-arbiter-models`,
and `files` as an ordered array of `{path, bytes, sha256}` pins.
`path` is the exact flat, lowercase public filename. `card` is `null` or the same pin
shape for the staged `README.md` Model card. Hash and review the exact
inventory bytes. Check verifies every file, rejects extra directory entries
and prints the approved pins without network access.

```powershell
python -I -B -S scripts/arbiter/distribution/arbiter_mirror.py check --directory <abs-stage> --inventory <abs-inventory> --inventory-sha256 <inventory-sha256>
```

## Publish and read back

Publish through a separately authorized release operation, using the reviewed
Python `huggingface_hub` SDK. The tool obtains an in-process credential from
`AIRK_HF_TOKEN` or the active `hf auth login` session. Use a token scoped to the
model repository. Copy the expected full head commit from the public repository
before the authorized operation. The upload verifies existing exact bytes and
submits only missing files against that head. A differing published filename
needs a new artifact revision. Maintain any later Model card edits separately
from the immutable artifact URL.

```powershell
python -I -B scripts/arbiter/distribution/arbiter_mirror.py publish-hub --directory <abs-stage> --inventory <abs-inventory> --inventory-sha256 <inventory-sha256> --expected-head <full-hub-head-commit>
python -I -B -S scripts/arbiter/distribution/arbiter_mirror.py verify-hub --directory <abs-stage> --inventory <abs-inventory> --inventory-sha256 <inventory-sha256> --commit <full-published-hub-commit>
```

Both successful commands return an anonymous length-and-SHA-256 readback for
each runtime file. An optional Model card is verified separately from the
runtime roster. Capture stdout at a fresh checked private path and pin its
exact bytes before handing it to native preparation. After an uncertain
upload, inspect the repository head and verify the actual published revision
before initiating another write.

## Native source maps and release inputs

Prepare one baseline map per selected native profile with the existing
`Vec<AssetDownload>` type. Explicitly nominate each logical asset, ordered
part, exact final GitHub release URL, byte length and SHA-256. Existing reviewed
upstream-only assets can stay in the original map. The mirrored roster is the
union of the GitHub-primary assets across all baseline maps. Each map may
reuse a part if its public name and digest match those already reviewed.

Supply a private request with schema `airk.private-arbiter-source-mirror-input/1`,
the explicit `github_tag`, an ordered array `baseline_maps` of existing
`PinnedFile` references and the pinned `hf_report`. Set
`AIRK_ARBITER_SOURCE_MIRROR_INPUT`, `AIRK_ARBITER_SOURCE_MIRROR_INPUT_SHA256`
and `AIRK_ARBITER_SOURCE_MIRROR_OUTPUT` in the checked native development shell.
Run the ignored Service selector
`service_runtime::arbiter::final_distribution::source_mirror::emit_reviewed_download_source_maps`
against a fresh guarded output path. The selector creates that directory.
Each emitted `download-sources-NNN.json` is an existing typed download map.
Use its exact SHA in the recorded promotion decision and the matching
`Candidate.download_sources`. Publication still requires the original exact
model qualification and rights proofs.

The GitHub release workflow continues to acquire the exact approved build input
IDs through `AIRK_ARBITER_BUILD_INPUT_ASSETS`. Keep the same public filenames
and hashes on both hosts and verify the served GitHub backup after publication.
Use the normal native Provision entry point for an anonymous full install and
for whole-asset fallback with controlled primary failures. The release workflow
and authorized publication actions remain separate from this preparation.

## Local tests

Run `python -B -m unittest discover -s scripts/arbiter/distribution -p 'test_*.py'`
with fixture scratch in an approved, checked location. These tests use
synthetic payloads and mocked network and SDK clients.
