# MiniLM evaluation support

[Blind metadata r5](blind-metadata-r5/README.md) retains the grading resource-budget helper and standalone metadata-audit implementation. Source identities and reversible path edits are in [the migration map](../source-map.json). Run `python -B -S scripts/arbiter/export/minilm/test_migration.py` from the repository root. No private input, model, export or grading result is included.

[recipe_paths.py](recipe_paths.py) supplies read-only Git-ignore and fresh-output admission for preparation. It requires explicit AIRK_RECIPE_RUN_ROOT; identity and output-parent ACL auditing remain the launcher duty. See the [preparation commands](../README.md#private-preparation-interface).
