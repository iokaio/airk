# SPDX-License-Identifier: Apache-2.0
"""Prepared FINAL-only native owner. Execute only after primary phase/lifetime approval."""
import datetime,hashlib,importlib.util,json,os
from pathlib import Path
import shutil
root=Path.cwd();source=root/'artifacts/advanced-split/native-harnesses/goal-fixture-cpu24-r8/airk_service-7285a460d5f02399.exe';expected='d07d63d25387a9d984df541787925cad01bd9a89caddaee7fca9b0ce05942bbe'
home=root/'artifacts/advanced-split/native-harnesses/goal-fixture-cpu24-final-r1';work=root/'artifacts/advanced-split/models/native-goal-private-anchor-final-r1/work';inputs=root/'artifacts/advanced-split/models/native-hybrid-next-read-profiles-cpu24-r2/materialized/evaluation-inputs.json';observer=root/'scripts/provision-static-onnx-runtime.py'
def sha(p):
 with p.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
assert sha(source)==expected and sha(inputs)=='868810174d6c287502653d49ded5bddbfe3d1b8c43519bd97ae023c4797fd3d4';assert sha(observer)=='304ff3392aec7a97cd57802e8a91bb2b0d7b1102e0452f08662627b0d1f667a4';assert not work.exists()
home.mkdir(parents=True,exist_ok=False);binary=home/source.name;shutil.copy2(source,binary);assert sha(binary)==sha(source)==expected
record={'scope':'Prepared new final phase, all six final conditions must bind ONE original Ready/incarnation/namespace/actual native bindings before first score. No promotion of failed A1 encoder.','started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'binary':str(binary),'binary_sha256':expected,'input':str(inputs),'input_sha256':sha(inputs),'observer':str(observer),'observer_sha256':sha(observer),'work':str(work),'fixture_lifetime_seconds':18000,'owned_observer_seconds':19200,'read_request_seconds_unchanged':30,'release_qualified':False}
(home/'run.json').write_text(json.dumps(record,indent=2)+'\n');spec=importlib.util.spec_from_file_location('final_fixture_owned_observer',observer);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
env=dict(os.environ,AIRK_ARBITER_EVALUATION_INPUTS=str(inputs),AIRK_ARBITER_EVALUATION_INPUTS_SHA256=record['input_sha256'],AIRK_ARBITER_EVALUATION_WORK=str(work),AIRK_ARBITER_EVALUATION_SECONDS='18000');result=owner.run_evaluation_fixture([str(binary),'service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture','--ignored','--exact','--nocapture'],root,home/'native.log',19200,env)
record.update(owned_result=result,finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),binary_unchanged=sha(binary)==expected,input_unchanged=sha(inputs)==record['input_sha256'],observer_unchanged=sha(observer)==record['observer_sha256'])
ready_path=work/'ready.json';closed_path=work/'closed.json';record['ready_sha256']=sha(ready_path) if ready_path.exists() else None;record['closed_sha256']=sha(closed_path) if closed_path.exists() else None
ready=json.loads(ready_path.read_bytes()) if ready_path.exists() else {};closed=json.loads(closed_path.read_bytes()) if closed_path.exists() else {};record['actual_original_ready_health']=ready.get('health');record['native_closed']=closed
record['normal_closure_verified']=owner.successful(result) and result.get('owned_job_forced_termination') is False and result.get('child_assigned_before_resume') is True and closed.get('shutdown_completed') is True and closed.get('service_lease_reacquired') is True and closed.get('instance')==ready.get('health',{}).get('instance')
(home/'run.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record,indent=2));raise SystemExit(0 if record['normal_closure_verified'] and record['binary_unchanged'] and record['input_unchanged'] and record['observer_unchanged'] else 1)
