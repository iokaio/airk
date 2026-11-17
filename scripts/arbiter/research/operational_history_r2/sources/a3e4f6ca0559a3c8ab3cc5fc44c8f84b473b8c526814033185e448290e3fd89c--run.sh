# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
# Prepared only. Root owns the sole Cargo baton and invocation.
set -eu
umask 077
[ "${AIRK_CARGO_OWNER_BATON:-}" = "mac-cli-await-c6a3e93de5e0e4c0" ] || { echo 'Root Cargo owner baton required' >&2; exit 29; }
cd "$HOME/code/airk"
platform_python="$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13"
platform_target="$PWD/artifacts/v190/p10-target"
platform_receipts="$PWD/artifacts/advanced-split/receipts/mac-cli-await-r1"
PATH="$PWD/artifacts/tools/neo-nextest-0.9.148:$HOME/.cargo/bin:$PATH"
export PATH CARGO_TARGET_DIR="$platform_target" CARGO_INCREMENTAL=0 PYTHONDONTWRITEBYTECODE=1
unset AIRK_BUNDLED_MODEL_DIR AIRK_VECTOR_MODEL AIRK_VECTOR_MODEL_MANIFEST AIRK_VECTOR_TOKENIZER ORT_LIB_LOCATION ORT_LIB_PROFILE ORT_PREFER_DYNAMIC_LINK
mkdir "$platform_receipts"
mkdir "$platform_receipts/native-fixtures"
TMPDIR=$(cd "$platform_receipts/native-fixtures" && pwd -P)
export TMPDIR TEMP="$TMPDIR" TMP="$TMPDIR"
source_check() {
 "$platform_python" -I -B - <<'PY'
import hashlib,json,sys
from pathlib import Path
assert sys.version_info[:3]==(3,13,18)
p=Path('artifacts/advanced-split/receipts/mac-cli-await-source-r1.json');raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()=='d795f39302e0abe432a9a3b5086ea78e0e69e10f46521d3f18a0feec5dd4258d'
v=json.loads(raw)
for name,row in v['files'].items():
 path=Path(name);assert path.is_file() and not path.is_symlink() and hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256'],name
for name in v['retired_auto_discovery']:assert not Path(name).exists(),name
print('Current sealed public source',v['source_set_sha256'],len(v['files']))
PY
}
source_check
"$platform_python" -I -B - <<'PY'
import os
from pathlib import Path
root=Path.cwd().resolve();expected=root/'artifacts/v190/p10-target';actual=Path(os.environ['CARGO_TARGET_DIR']).resolve()
for path in (expected,*expected.parents):
 assert not path.is_symlink(),path
 if path==root:break
assert actual==expected and actual.is_relative_to(root/'artifacts/v190'),actual
PY
"$platform_python" -I -B -c 'import os,pathlib,tempfile; p=pathlib.Path(os.environ["TMPDIR"]); assert p.resolve()==p and p.stat().st_mode & 0o077==0; assert pathlib.Path(tempfile.gettempdir())==p; print("resolved private TMPDIR",p)'
cargo nextest --version | tee "$platform_receipts/nextest-version.txt"
grep -q '0.9.148' "$platform_receipts/nextest-version.txt"
compiler_idle() {
 "$platform_python" -I -B - <<'PY'
import subprocess
from pathlib import Path
rows=subprocess.check_output(['/bin/ps','-axo','comm='],text=True).splitlines()
active=[p for p in rows if Path(p.strip()).name in ('cargo','rustc','clippy-driver','rustfmt')]
assert not active,active
PY
}
compiler_idle
finish() {
 status=$?
 set +e
 trap - EXIT HUP INT TERM
 source_check || status=3
 # Retain native receipts/logs plus actual timing/JUnit bytes outside target
 # BEFORE the mandatory clean, even if compilation or a test failed.
 "$platform_python" -I -B - "$platform_target" "$platform_receipts" <<'PY'
import hashlib,json,shutil,sys
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
trap finish EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --fixture-base "$TMPDIR" --target-dir "$platform_target" mac-cli-await-clippy "$platform_python" scripts/dev-check.py -p airk-cli --target-dir "$platform_target" --incremental off
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --fixture-base "$TMPDIR" --target-dir "$platform_target" mac-cli-await-lib cargo nextest run --profile ci -p airk-cli --lib --no-default-features --locked --timings --target-dir "$platform_target" --test-threads 2
# Complete lib execution includes the exact required readonly/role regressions;
# verify independently listed cases in actual matching UUID JUnit, not a second run.
"$platform_python" -I -B - "$platform_target" "$platform_receipts" <<'PY'
import hashlib,json,re,sys,xml.etree.ElementTree as ET
from pathlib import Path
root=Path.cwd().resolve();target=Path(sys.argv[1]);out=Path(sys.argv[2]);text=(out/'mac-cli-await-lib.log').read_text()
run=re.search(r'Nextest run ID ([0-9a-f-]+)',text).group(1);selected=int(re.search(r'Starting (\d+) tests',text).group(1));assert selected==35,('Unexpected no-default CLI lib test population',selected)
required={
'role_admission::tests::ordinary_arguments_select_no_role','role_admission::tests::bare_unknown_and_pending_roles_are_refused','role_admission::tests::service_detached_requires_its_authority_arguments','role_admission::tests::a_valid_request_is_consumed_once_with_its_mode','role_admission::tests::an_invalid_request_is_refused_and_still_consumed','role_admission::tests::a_request_readable_by_others_is_refused','role_admission::tests::listed_roles_match_the_reviewed_table','role_admission::tests::record_error_requires_an_absolute_root_and_an_operation','role_admission::tests::setup_takes_an_absolute_operation_and_one_mode','role_admission::tests::service_login_takes_one_install_id','role_admission::tests::service_foreground_takes_an_absolute_root_and_a_bounded_idle_grace','role_admission::tests::static_mac_login_has_no_embedded_per_install_identity','role_admission::tests::removal_admission_and_inno_sealing_are_exclusive_private_setup_modes','role_admission::tests::recovery_fields_and_wrapper_lease_are_closed_exclusive_private_modes',
'runtime_commands::connection_observation_tests::source_scope_rejected_metadata_never_creates_install_lock_or_state','runtime_commands::connection_observation_tests::source_scope_authenticates_digest_control_and_missing_state_without_creation','runtime_commands::connection_observation_tests::source_scope_preserves_pending_stop_and_activation_without_election','runtime_commands::connection_observation_tests::source_scope_managed_metadata_authenticates_both_modes_without_starting','runtime_commands::connection_observation_tests::current_pending_or_malformed_activation_cannot_report_settled','runtime_commands::connection_observation_tests::dispatcher_future_inline_state_stays_within_the_readonly_command_stack_budget','runtime_commands::connection_observation_tests::captured_dispatch_allocates_before_poll_without_observing_or_creating_roots','runtime_commands::managed_service::tests::only_a_console_owned_by_the_task_alone_is_released'}
for candidate in [target/'nextest/ci/junit.xml',root/'target/nextest/ci/junit.xml']:
 if not candidate.exists():continue
 raw=candidate.read_bytes();assert len(raw)<=4*1024*1024;xml=ET.fromstring(raw)
 if xml.attrib.get('uuid')!=run:continue
 assert int(xml.attrib['tests'])==selected and int(xml.attrib['failures'])==0 and int(xml.attrib['errors'])==0
 cases={case.attrib['name'] for case in xml.iter('testcase')};assert required<=cases,('Missing exact readonly/role regression',sorted(required-cases))
 (out/'mac-cli-await-lib-observed-junit.xml').write_bytes(raw)
 (out/'exact-regressions.json').write_text(json.dumps({'uuid':run,'selected':selected,'required_cases':sorted(required),'required_count':len(required),'all_passed':True,'junit_sha256':hashlib.sha256(raw).hexdigest(),'source_path':str(candidate),'scope':'Mac no-default-feature CLI only; no model/inference/installed qualification'},indent=2)+'\n');break
else:raise ValueError('No actual matching Nextest UUID JUnit found; no success claim')
PY
source_check
