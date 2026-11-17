# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
# Prepared current13crate no-default Clippy plus two focused selections; ROOT ONLY.
set -eu
umask 077
[ "${AIRK_CARGO_OWNER_BATON:-}" = "mac-current-read-78dc37a517d9bb84" ] || { echo 'Root Cargo owner baton required' >&2; exit 29; }
cd "$HOME/code/airk"
bootstrap_python="$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13"
physical_root=$("$bootstrap_python" -I -B -c 'import pathlib,sys; assert sys.version_info[:3]==(3,13,18); print(pathlib.Path.cwd().resolve())')
cd "$physical_root"
platform_python="$physical_root/artifacts/tools/neo-python-3.13.18/bin/python3.13"
platform_target="$physical_root/artifacts/v190/p10-target"
platform_receipts="$physical_root/artifacts/advanced-split/receipts/mac-current-read-r16"
PATH="$physical_root/artifacts/tools/neo-nextest-0.9.148:$HOME/.cargo/bin:$PATH"
export PATH AIRK_MAC_CLI_PHYSICAL_ROOT="$physical_root" CARGO_TARGET_DIR="$platform_target" CARGO_INCREMENTAL=0 PYTHONDONTWRITEBYTECODE=1
native_command_entered=false
export AIRK_MAC_CLI_NATIVE_COMMAND_ENTERED="$native_command_entered"
unset AIRK_BUNDLED_MODEL_DIR AIRK_VECTOR_MODEL AIRK_VECTOR_MODEL_MANIFEST AIRK_VECTOR_TOKENIZER ORT_LIB_LOCATION ORT_LIB_PROFILE ORT_PREFER_DYNAMIC_LINK
mkdir "$platform_receipts"
source_check() {
 "$platform_python" -I -B - <<'PY'
import hashlib,json
from pathlib import Path
p=Path('artifacts/advanced-split/receipts/mac-current-read-source-r16.json')
raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()=='5c818598b1ca5f1d970c619374e80e5f8c9ea95b4feac18d1f37c21ee76dfe28'
v=json.loads(raw)
helpers={'verify-junit.py': '7e55842a7e59d81223903d79e3a0aa2046b7c717deb2d97726eb162d33a3d4fa', 'selections.json': '587bdd471a5f562f1c4ec41518dc5c8620f527debe0f468ede5fd8d8ca0a24af'}
for name,digest in helpers.items():
 p=Path('artifacts/advanced-split/receipts/mac-current-read-helpers-r16')/name
 assert p.is_file() and not p.is_symlink() and hashlib.sha256(p.read_bytes()).hexdigest()==digest,name
for name,row in v['files'].items():
 p=Path(name);assert p.is_file() and not p.is_symlink() and hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256'],name
print('Exact current public source',v['source_set_sha256'],len(v['files']))
PY
}

target_guard() {
 "$platform_python" -I -B - <<'PY'
import os
from pathlib import Path
root=Path.cwd().resolve();declared=Path(os.environ['AIRK_MAC_CLI_PHYSICAL_ROOT'])
assert root==declared and os.path.samefile(root,Path.home()/'code/airk'),'Not the approved checkout object'
expected=root/'artifacts/v190/p10-target';actual=Path(os.environ['CARGO_TARGET_DIR']).resolve()
for p in (expected,*expected.parents):
 assert not p.is_symlink(),p
 if p==root:break
assert actual==expected and actual.is_relative_to(root/'artifacts/v190'),actual
PY
}
compiler_idle() {
 "$platform_python" -I -B - <<'PY'
import subprocess
from pathlib import Path
rows=subprocess.check_output(['/bin/ps','-axo','comm='],text=True).splitlines()
active=[p for p in rows if Path(p.strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt','clang','clang++','cc','c++','ld','ar')]
assert not active,active
PY
}
observe_state() {
 "$platform_python" -I -B - <<'PY'
import json,os,subprocess
from pathlib import Path
root=Path.cwd().resolve();target=root/'artifacts/v190/p10-target';rows=subprocess.check_output(['/bin/ps','-axo','comm='],text=True).splitlines()
state={'physical_root':str(root),'target':str(target),'target_exists':target.exists(),'target_symlink':target.is_symlink(),'native_command_entered':os.environ.get('AIRK_MAC_CLI_NATIVE_COMMAND_ENTERED'),'active_compilers':[p.strip() for p in rows if Path(p.strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt','clang','clang++','cc','c++','ld','ar')]}
if target.exists() and not target.is_symlink():
 info=target.stat();state.update(target_device=info.st_dev,target_inode=info.st_ino,target_entries=sorted(p.name for p in target.iterdir()))
print(json.dumps(state,indent=2))
PY
}

finish() {
 status=$?
 set +e
 trap - EXIT HUP INT TERM
 source_check || status=3
 target_guard || { echo 'Physical workspace/target guard failed; no target traversal/clean' >"$platform_receipts/cleanup-refused.txt"; observe_state >"$platform_receipts/cleanup-state.json"; exit 7; }
 observe_state >"$platform_receipts/cleanup-state.json"
 # Retain native receipts/logs plus actual timing/JUnit bytes outside target
 # BEFORE the mandatory clean, even if compilation or a test failed.
 "$platform_python" -I -B - "$platform_target" "$platform_receipts" <<'PY'
import hashlib,html,json,re,shutil,sys
from pathlib import Path
root=Path.cwd().resolve();target=Path(sys.argv[1]).resolve();out=Path(sys.argv[2]).resolve()
assert target==root/'artifacts/v190/p10-target' and out.is_relative_to(root/'artifacts/advanced-split/receipts')
rows=[]
for folder in [target/'cargo-timings',target/'nextest/ci',root/'target/nextest/ci']:
 if not folder.exists():continue
 for p in folder.rglob('*'):
  if not p.is_file() or p.is_symlink() or p.suffix not in ('.html','.xml','.json'):continue
  assert p.stat().st_size<=8*1024*1024,p
  name=('configured-target/' if p.is_relative_to(target) else 'workspace-default-target/')+str(p.relative_to(target if p.is_relative_to(target) else root/'target'))
  dest=out/'retained-before-clean'/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,dest)
  rows.append({'source':str(p),'retained':str(dest.relative_to(out)),'sha256':hashlib.sha256(dest.read_bytes()).hexdigest()})
(out/'retained-artifacts.json').write_text(json.dumps(rows,indent=2)+'\n')
compiler_rows=[]
for row in rows:
 if not row['retained'].endswith('.html'):continue
 text=(out/row['retained']).read_text(errors='replace')
 for value in re.findall(r'<td>rustc:</td><td>(.*?)</td>',text,re.S):
  observed=html.unescape(re.sub(r'<[^>]+>','\n',value)).strip()
  compiler_rows.append({'origin':'Actual Cargo-generated timing HTML','retained':row['retained'],'value':observed})
rustc_cache=target/'.rustc_info.json';cache=None
if rustc_cache.is_file() and not rustc_cache.is_symlink():
 data=rustc_cache.read_bytes();assert len(data)<=4*1024*1024
 (out/'actual-cargo-rustc-info.json').write_bytes(data);cache={'origin':'Cargo target compiler-query cache, not an unrelated standalone rustc invocation','sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)}
(out/'compiler-identity.json').write_text(json.dumps({'actual_cargo_timing_rows':compiler_rows,'actual_cargo_query_cache':cache,'toolchain_context':'toolchain-context.json','nextest_identity':'nextest-version.txt is cargo-nextest0.9.148/cd1, not Rust compiler identity','note':'R3 Cargo-generated timing reported rustc1.98.0/88d9 matching stdlib paths; no observed compiler mismatch'},indent=2)+'\n')

PY
 retain_status=$?
 if [ "$retain_status" -ne 0 ]; then echo 'Retention failed; target preserved for root recovery' >&2; exit "$retain_status"; fi
 if compiler_idle; then
  cargo clean --target-dir "$platform_target" >"$platform_receipts/clean.log" 2>&1 || { cat "$platform_receipts/clean.log"; exit 5; }
 else
  echo 'Compiler still active; target preserved, no cleanup claimed' >"$platform_receipts/cleanup-refused.txt"
  exit 6
 fi
 cat "$platform_receipts/clean.log"
 exit "$status"
}

# The trap is installed BEFORE any source/target/TMPDIR/version guard or Cargo.
# An early refusal retains observed state and cleans only after strong guard
# and compiler-exit proof; a guard/retention refusal explicitly preserves target.
trap finish EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
observe_state >"$platform_receipts/initial-state.json"
source_check
target_guard
compiler_idle
mkdir "$platform_receipts/native-fixtures"
TMPDIR=$("$platform_python" -I -B -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "$platform_receipts/native-fixtures")
export TMPDIR TEMP="$TMPDIR" TMP="$TMPDIR"
"$platform_python" -I -B -c 'import os,pathlib,tempfile; p=pathlib.Path(os.environ["TMPDIR"]); assert p.resolve()==p and p.stat().st_mode & 0o077==0; assert pathlib.Path(tempfile.gettempdir())==p; print("resolved private TMPDIR",p)'
cargo nextest --version | tee "$platform_receipts/nextest-version.txt"
grep -q '0.9.148' "$platform_receipts/nextest-version.txt"

"$platform_python" -I -B - "$platform_receipts" <<'PY'
import hashlib,json,os,shutil,subprocess,sys,tomllib
from pathlib import Path
root=Path.cwd().resolve();out=Path(sys.argv[1]);raw=(root/'rust-toolchain.toml').read_bytes()
commands=[['rustup','show','active-toolchain'],['rustup','which','cargo'],['rustup','which','rustc'],['cargo','--version','--verbose']]
observations=[]
for command in commands:
 r=subprocess.run(command,capture_output=True,text=True,check=True);observations.append({'command':command,'stdout':r.stdout,'stderr':r.stderr})
(out/'toolchain-context.json').write_text(json.dumps({'physical_workspace':str(root),'pinned_channel':tomllib.loads(raw.decode())['toolchain']['channel'],'toolchain_file_sha256':hashlib.sha256(raw).hexdigest(),'cargo_path':shutil.which('cargo'),'rustup_queries':observations,'environment_overrides':{key:os.environ.get(key) for key in ['RUSTUP_TOOLCHAIN','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER']},'semantics':'These are workspace Cargo/rustup selection observations. Actual compiler identity is separately retained from Cargo-produced timing/.rustc_info; no standalone rustc-Vv substitution.'},indent=2)+'\n')
PY
native_command_entered=true
export AIRK_MAC_CLI_NATIVE_COMMAND_ENTERED="$native_command_entered"
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --fixture-base "$TMPDIR" --target-dir "$platform_target" mac-current-clippy "$platform_python" scripts/dev-check.py -p airk-cli -p airkctl -p airk-inference -p airk-supervisor -p airk-installer -p airk-ipc -p airk-service -p airk-kernel -p airk-core -p airk-host -p airk-mcp -p airk-https -p airk-frontend --target-dir "$platform_target" --incremental off
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --fixture-base "$TMPDIR" --target-dir "$platform_target" mac-current-baseline cargo nextest run --profile ci -p airk-cli -p airkctl -p airk-ipc -p airk-kernel -p airk-service --lib --no-default-features --locked --timings --target-dir "$platform_target" -E 'package(airk-cli) | (package(airkctl) & (test(agents::config::retention::tests::config_retained_available_bytes_observes_actual_pinned_apfs_directory) | test(agents::config::retention::tests::config_retained_available_bytes_widens_native_fields_and_refuses_overflow))) | (package(airk-ipc) & (test(control::tests::arbiter_expired_after_held_connect_refuses_before_writing_any_request) | test(control::tests::arbiter_framed_response_can_arrive_after_five_seconds_inside_original_deadline) | test(control::tests::arbiter_framed_response_stops_at_original_deadline_without_replay) | test(control::tests::arbiter_native_response_budget_uses_original_clock_and_never_refreshes_it) | test(control::tests::tools_call_remaining_keeps_long_queries_and_never_refreshes_short_parents) | test(control::tests::tools_call_frame_retains_original_parent_until_response_and_sends_once))) | (package(airk-kernel) & (test(arbiter::tests::native_initial_retention_exact_boundary_keeps_full_receipt_through_finish_and_pack) | test(arbiter::tests::native_initial_retention_one_byte_over_boundary_refuses_without_raising_budget) | test(arbiter::tests::native_initial_retention_small_budget_refuses_instead_of_packing_an_empty_success) | test(arbiter::tests::native_packet_packing_keeps_original_search_and_omits_whole_later_receipts) | test(arbiter::tests::native_packet_packing_omits_later_search_before_retaining_additional_context) | test(arbiter::tests::native_packet_packing_refuses_oversized_initial_search_without_removing_or_truncating_it))) | (package(airk-service) & (test(service_runtime::tools_call::tests::singleton_answer_narrows_ninety_second_wire_before_local_dispatch_and_never_refreshes_it) | test(service_runtime::tools_call::tests::singleton_admission_cannot_restore_an_earlier_deadline_or_rebind_original_identity) | test(service_runtime::tools_call::tests::both_wire_modes_map_to_their_mcp_mode) | test(service_runtime::tools_call::tests::failure_messages_keep_the_chain_and_the_cli_bound) | test(service_runtime::tools_call::tests::a_selection_that_cannot_resolve_is_a_typed_failure) | test(service_runtime::tools_call::tests::without_a_project_only_the_in_process_help_answers) | test(service_runtime::tools_call::tests::combined_defaults_keep_the_cli_shape_and_need_projects_to_narrow) | test(service_runtime::tools_call::tests::supplied_short_and_long_parents_are_validated_before_selection)))' --test-threads 2
"$platform_python" -I -B artifacts/advanced-split/receipts/mac-current-read-helpers-r16/verify-junit.py artifacts/advanced-split/receipts/mac-current-read-helpers-r16/selections.json baseline "$platform_target" "$platform_receipts" mac-current-baseline
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --fixture-base "$TMPDIR" --target-dir "$platform_target" mac-current-delivery cargo nextest run --profile ci -p airk-core -p airk-host -p airk-mcp -p airk-https -p airk-frontend --lib --no-default-features --locked --timings --target-dir "$platform_target" -E '(package(airk-core) & (test(combined_context::tests::combined_context_literal_allocations_preserve_one_global_budget) | test(combined_context::tests::combined_context_allowance_rejects_foreign_bindings_and_bad_ordinals) | test(combined_context::tests::combined_context_unknown_work_holds_full_original_reservations) | test(combined_context::tests::combined_context_known_work_cannot_spend_unknown_assessment_or_token_credit) | test(combined_context::tests::combined_context_completed_assessment_requires_positive_actual_token_charge) | test(combined_context::tests::combined_context_explicit_unqueried_members_cannot_claim_complete) | test(combined_context::tests::combined_context_accounting_overflow_and_noncanonical_rosters_refuse) | test(combined_context::tests::combined_context_known_failed_overrun_retains_observation_without_enlarging_capacity) | test(combined_context::tests::context_delivery_binding_required_fields_cannot_disappear_or_wildcard_profile) | test(combined_context::tests::context_delivery_extractor_retains_full_packet_and_fences_actual_reply_roster) | test(combined_context::tests::context_delivery_known_work_cannot_omit_its_actual_native_packet) | test(combined_context::tests::context_delivery_known_cannot_hide_unknown_work_or_actual_charged_ordinal) | test(read_attempt::tools_call_tests::tools_call_attempt_none_retains_all_seven_literal_m3_messages) | test(read_attempt::tools_call_tests::tools_call_attempt_some_roundtrips_original_absolute_and_ninety_second_query) | test(read_attempt::tools_call_tests::tools_call_attempt_old_strict_reader_refuses_some_before_effect) | test(read_attempt::tools_call_tests::tools_call_attempt_malformed_budget_never_reaches_validated_effect))) | (package(airk-host) & (test(read_attempt::tests::combined_members_reuse_exact_captured_expiry_and_only_narrow_children) | test(read_attempt::tests::sampled_conversion_charges_nanoseconds_and_repeated_frames_without_refresh))) | (package(airk-mcp) & (test(read_attempt::context_identity_tests::context_partition_narrowing_keeps_original_identity_cancel_and_single_retry) | test(read_attempt::context_identity_tests::context_partition_cannot_rebind_or_replace_original_wire_identity) | test(read_attempt::delivery_tests::one_outer_guard_is_shared_and_cannot_be_replaced_by_another_binding_roster) | test(read_attempt::delivery_tests::private_members_defer_registration_to_one_full_parent_reply) | test(read_attempt::delivery_tests::no_guard_still_honors_original_cancellation_at_actual_poll))) | (package(airk-https) & (test(http_gateway::tests::native_policy_fences_buffered_tls_bytes_at_actual_write_poll) | test(http_gateway::tests::native_source_delivery_is_nested_once_inside_policy_and_credential_cuts) | test(http_gateway::tests::policy_lock_covers_actual_http_effect_and_rechecks_after_partial_write) | test(http_gateway::tests::cancellation_after_outer_policy_check_refuses_actual_http_write_cut) | test(http_gateway::tests::narrowed_deadline_expiring_inside_source_gate_refuses_actual_http_cut) | test(admin_plane::native::tests::native_original_delivery_custody_survives_final_chunk_and_fences_late_policy_change))) | (package(airk-frontend) & (test(combined_frontend::answer::tests::malformed_context_success_is_unknown_nonreplayable_and_failure_bytes_stay_full) | test(project_client::read_lifetime::tests::complete_write_retains_capacity_after_cancellation_until_actual_reply) | test(project_client::read_lifetime::tests::eof_retains_capacity_until_exact_supervisor_reap_and_cancellation_is_not_settlement) | test(project_client::read_lifetime::tests::actual_partial_frame_closes_socket_and_cannot_return_capacity_from_cancellation)))' --test-threads 2
"$platform_python" -I -B artifacts/advanced-split/receipts/mac-current-read-helpers-r16/verify-junit.py artifacts/advanced-split/receipts/mac-current-read-helpers-r16/selections.json delivery "$platform_target" "$platform_receipts" mac-current-delivery
source_check
