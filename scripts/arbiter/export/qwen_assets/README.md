# Qwen publisher asset acquisition

These two versioned entry points preserve the original selected-model download/filter/size/digest/concurrency algorithm. They now import without filesystem or network work and require an explicit reviewed manifest/hash and existing private output root. The two originals were identical; source-provenance.json retains both original locations and maps the new bytes. Publisher manifests and all weights/data remain private artifacts.

From the repository root, under the verified same-user execution path with output/cache parents preflighted:

```powershell
python -B scripts/arbiter/export/qwen_assets/qwen3-challenger-assets-r1/acquire_candidates.py --manifest artifacts/<reviewed-publisher-manifest.json> --manifest-sha256 <reviewed-sha256> --output-root artifacts/<guarded-private-root>
```

Use the sibling qwen3-reranker06-assets-r1 entry point for the Focus manifest. Obtain the exact manifest and hash from the approved publisher acquisition evidence; these recipes do not invent or regenerate catalog pins. Both write candidate-assets under the supplied output root, preserve interruption refusal and verify cached bytes. All generated descendants require a same-user ownership audit before consumption. No download or model run was performed for this migration.

The manifest hash is checked before output creation. Every asset is checked against its original size and SHA-256 or Git blob identity; Python/training/YAML model code remains excluded as in the original. Archived head.pt is never unpickled. This captures the research acquisition recipe and does not grant rights, training approval or product qualification. Required publisher metadata, network and runtime inputs are separate from source ownership.
