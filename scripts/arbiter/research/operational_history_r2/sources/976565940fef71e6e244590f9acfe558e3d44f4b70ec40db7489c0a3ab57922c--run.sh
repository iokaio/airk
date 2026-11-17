# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
# Fresh RUN-ONLY correction: applied source-r2.json is verified, never reapplied.
set -eu
umask 077
[ "${AIRK_CARGO_OWNER_BATON:-}" = "mac-cli-await-9296e2874af4af41" ] || { echo 'Root Cargo owner baton required' >&2; exit 29; }
cd "$HOME/code/airk"
bootstrap_python="$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13"
physical_root=$("$bootstrap_python" -I -B -c 'import pathlib,sys; assert sys.version_info[:3]==(3,13,18); print(pathlib.Path.cwd().resolve())')
cd "$physical_root"
platform_python="$physical_root/artifacts/tools/neo-python-3.13.18/bin/python3.13"
platform_target="$physical_root/artifacts/v190/p10-target"
platform_receipts="$physical_root/artifacts/advanced-split/receipts/mac-cli-await-r6"
PATH="$physical_root/artifacts/tools/neo-nextest-0.9.148:$HOME/.cargo/bin:$PATH"
export PATH AIRK_MAC_CLI_PHYSICAL_ROOT="$physical_root" CARGO_TARGET_DIR="$platform_target" CARGO_INCREMENTAL=0 PYTHONDONTWRITEBYTECODE=1
native_command_entered=false
export AIRK_MAC_CLI_NATIVE_COMMAND_ENTERED="$native_command_entered"
unset AIRK_BUNDLED_MODEL_DIR AIRK_VECTOR_MODEL AIRK_VECTOR_MODEL_MANIFEST AIRK_VECTOR_TOKENIZER ORT_LIB_LOCATION ORT_LIB_PROFILE ORT_PREFER_DYNAMIC_LINK
mkdir "$platform_receipts"
source_check() {
 "$platform_python" -I -B - <<'PY'
import hashlib,json,sys
from pathlib import Path
assert sys.version_info[:3]==(3,13,18)
p=Path('artifacts/advanced-split/receipts/mac-cli-await-source-r6.json');raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()=='908e962c94f48f48f6f2301fc45b40d1d7080b74f29ffd6934cdedf5c59dde51'
v=json.loads(raw)
for name,row in v['files'].items():
 path=Path(name);assert path.is_file() and not path.is_symlink() and hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256'],name
for name in v['retired_auto_discovery']:assert not Path(name).exists(),name
print('Current sealed public source',v['source_set_sha256'],len(v['files']))
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
active=[p for p in rows if Path(p.strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt')]
assert not active,active
PY
}
observe_state() {
 "$platform_python" -I -B - <<'PY'
import json,os,subprocess
from pathlib import Path
root=Path.cwd().resolve();target=root/'artifacts/v190/p10-target';rows=subprocess.check_output(['/bin/ps','-axo','comm='],text=True).splitlines()
state={'physical_root':str(root),'target':str(target),'target_exists':target.exists(),'target_symlink':target.is_symlink(),'native_command_entered':os.environ.get('AIRK_MAC_CLI_NATIVE_COMMAND_ENTERED'),'active_compilers':[p.strip() for p in rows if Path(p.strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt')]}
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
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --fixture-base "$TMPDIR" --target-dir "$platform_target" mac-cli-await-clippy "$platform_python" scripts/dev-check.py -p airk-cli -p airkctl -p airk-inference -p airk-supervisor --target-dir "$platform_target" --incremental off
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --fixture-base "$TMPDIR" --target-dir "$platform_target" mac-cli-await-lib cargo nextest run --profile ci -p airk-cli -p airkctl --lib --no-default-features --locked --timings --target-dir "$platform_target" -E 'package(airk-cli) | (package(airkctl) & (test(config_retained_available_bytes_widens_native_fields_and_refuses_overflow) | test(config_retained_available_bytes_observes_actual_pinned_apfs_directory)))' --test-threads 2
# Complete lib execution includes the exact required readonly/role regressions;
# verify independently listed cases in actual matching UUID JUnit, not a second run.
"$platform_python" -I -B - "$platform_target" "$platform_receipts" <<'PY'
import hashlib,json,re,sys,xml.etree.ElementTree as ET
from pathlib import Path
root=Path.cwd().resolve();target=Path(sys.argv[1]);out=Path(sys.argv[2]);text=(out/'mac-cli-await-lib.log').read_text()
run=re.search(r'Nextest run ID ([0-9a-f-]+)',text).group(1);selected=int(re.search(r'Starting (\d+) tests',text).group(1));assert selected==37,('Unexpected no-default CLI lib test population',selected)
required={
'role_admission::tests::ordinary_arguments_select_no_role','role_admission::tests::bare_unknown_and_pending_roles_are_refused','role_admission::tests::service_detached_requires_its_authority_arguments','role_admission::tests::a_valid_request_is_consumed_once_with_its_mode','role_admission::tests::an_invalid_request_is_refused_and_still_consumed','role_admission::tests::a_request_readable_by_others_is_refused','role_admission::tests::listed_roles_match_the_reviewed_table','role_admission::tests::record_error_requires_an_absolute_root_and_an_operation','role_admission::tests::setup_takes_an_absolute_operation_and_one_mode','role_admission::tests::service_login_takes_one_install_id','role_admission::tests::service_foreground_takes_an_absolute_root_and_a_bounded_idle_grace','role_admission::tests::static_mac_login_has_no_embedded_per_install_identity','role_admission::tests::removal_admission_and_inno_sealing_are_exclusive_private_setup_modes','role_admission::tests::recovery_fields_and_wrapper_lease_are_closed_exclusive_private_modes',
'runtime_commands::connection_observation_tests::source_scope_rejected_metadata_never_creates_install_lock_or_state','runtime_commands::connection_observation_tests::source_scope_authenticates_digest_control_and_missing_state_without_creation','runtime_commands::connection_observation_tests::source_scope_preserves_pending_stop_and_activation_without_election','runtime_commands::connection_observation_tests::source_scope_managed_metadata_authenticates_both_modes_without_starting','runtime_commands::connection_observation_tests::current_pending_or_malformed_activation_cannot_report_settled','runtime_commands::connection_observation_tests::dispatcher_future_inline_state_stays_within_the_readonly_command_stack_budget','runtime_commands::connection_observation_tests::captured_dispatch_allocates_before_poll_without_observing_or_creating_roots','runtime_commands::managed_service::tests::only_a_console_owned_by_the_task_alone_is_released'}
retention_required={'agents::config::retention::tests::config_retained_available_bytes_widens_native_fields_and_refuses_overflow','agents::config::retention::tests::config_retained_available_bytes_observes_actual_pinned_apfs_directory'}
required|=retention_required
for candidate in [target/'nextest/ci/junit.xml' ,root/'target/nextest/ci/junit.xml']:
 if not candidate.exists():continue
 raw=candidate.read_bytes();assert len(raw)<=4*1024*1024;xml=ET.fromstring(raw)
 if xml.attrib.get('uuid')!=run:continue
 assert int(xml.attrib['tests'])==selected and int(xml.attrib['failures'])==0 and int(xml.attrib['errors'])==0
 cases={case.attrib['name'] for case in xml.iter('testcase')};assert required<=cases,('Missing exact readonly/role/retention regression',sorted(required-cases))
 cli_cases={name for name in cases if name.startswith(('role_admission::','runtime_commands::','standalone_setup::'))};assert len(cli_cases)==35 and cases-cli_cases==retention_required,('Unexpected CLI/retention populations',len(cli_cases),sorted(cases-cli_cases))
 (out/'mac-cli-await-lib-observed-junit.xml').write_bytes(raw)
 (out/'exact-regressions.json').write_text(json.dumps({'uuid':run,'selected':selected,'required_cases':sorted(required),'required_count':len(required),'all_passed':True,'junit_sha256':hashlib.sha256(raw).hexdigest(),'source_path':str(candidate),'cli_count':35,'retention_count':2,'scope':'Mac no-default CLI35+numeric/statvfs2; no model/inference/installed qualification'},indent=2)+'\n');break
else:raise ValueError('No actual matching Nextest UUID JUnit found; no success claim')
PY
source_check
