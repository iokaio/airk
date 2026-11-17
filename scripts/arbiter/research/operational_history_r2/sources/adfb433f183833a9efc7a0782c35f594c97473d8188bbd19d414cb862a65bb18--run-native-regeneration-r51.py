# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/native-regeneration-r51','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
sel='test(strict_original_embedding_cache_refuses_unknown_tampered_and_revoked_before_forward) | test(authenticated_cache_import_preserves_literal_vector_bits_keys_and_old_scope) | test(authenticated_cache_import_refuses_nonfinite_duplicate_or_foreign_census) | test(regeneration_operation_anchor_rejects_every_frozen_tree_before_creating_output) | test(saved_unsupported_graph_spec_never_reports_ready_and_preserves_scoped_metadata)'
rows=[('scoped-clippy',True,[cargo,'clippy','-p','airk-kernel','-p','airk-retrieval','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('cache-and-readiness-controls',False,[cargo,'nextest','run','--profile','ci','-p','airk-kernel','-p','airk-retrieval','--lib','--all-features','--locked','--timings','--fail-fast','-E',sel]),('capture-kernel-image',True,[cargo,'test','-p','airk-kernel','--lib','--all-features','--locked','--timings','--no-run','--message-format=json-render-diagnostics'])]
for label,allow,cmd in rows:
 result=subprocess.run(base+(['--allow-no-tests'] if allow else [])+[label]+cmd,env=env)
 if result.returncode:raise SystemExit(result.returncode)
