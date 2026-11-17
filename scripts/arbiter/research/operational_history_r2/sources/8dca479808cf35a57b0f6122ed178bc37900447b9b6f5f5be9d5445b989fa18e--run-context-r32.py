# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env=dict(os.environ);env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2')),env.update(AIRK_WRITE_COMBINED_CONTEXT_CASES='1')
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures,harness','--out','artifacts/advanced-split/receipts/native-context-r32','--target-dir','artifacts/v190/p10-target']
rows=[('workspace-clippy',[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('focused-context',[cargo,'nextest','run','--profile','ci','-p','airk-core','-p','airk-kernel','-p','airk-frontend','-p','airk-mcp','--lib','--all-features','--locked','--timings','-E','test(verification::combined_projection_tests::) | test(native_parent_coverage_requires_exact_union_without_cropping_units) | test(native_train_source_census_detects_extra_missing_and_alias_paths) | test(emit_combined_context_semantic_cases) | test(context_byte_admission_charges_closed_envelope_escaping_and_original_metadata) | test(combined_answer_counts_materialized_multi_hit_envelopes_without_changing_native_limits) | test(ordinary_multi_hit_count_refusals_are_identical_for_text_and_structured_envelopes) | test(combined_frontend::answer::verification::tests::) | test(answer_inner_reads_cannot_mint_verification_budgets_but_parse_keeps_explicit_controls)'])]
for name,cmd in rows:
 status=subprocess.run(base+[name]+cmd,env=env).returncode
 if status:raise SystemExit(status)


