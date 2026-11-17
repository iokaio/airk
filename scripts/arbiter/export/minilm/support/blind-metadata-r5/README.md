# Blind metadata audit r5

Maintained source from `artifacts/advanced-split/models/a1-next-experiment-blind-metadata-r5/audit.py`; exact original SHA-256 and reversible edits are in [source-map.json](../../source-map.json). This is project analysis code, with no embedded corpus. Its fixed article identity, split computation, source/input checks, 60-second limit and 256 MiB Windows Job limit are retained. Graders import the resource observer without requiring private audit data.

Standalone execution requires Windows, Python 3.13.7, Unicode 15.1.0, the sealed PyArrow runtime, private `input-seal.json`, Hotpot parquet and owner-manifest inputs in the historical relative layout. Source/runtime/data seals must be freshly reviewed for the maintained source. The private audit directory is explicit and receives atomic result/completion JSON and their `.partial` files. It must be a fresh, human-owned guarded directory; audit every output parent before launch. No runtime environment, dataset, seal or result is shipped here.

After that preflight, from the repository root:

```powershell
$env:AIRK_BLIND_METADATA_ROOT = '<absolute-private-audit-directory>'
python -B scripts/arbiter/export/minilm/support/blind-metadata-r5/audit.py
```

Without that variable standalone execution refuses before establishing the Job or writing outputs. Data-free source checks: `python -B -S scripts/arbiter/export/minilm/test_migration.py`. No metadata/model experiment was run for this migration.
