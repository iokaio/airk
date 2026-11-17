# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess
from pathlib import Path
run=Path(__file__).resolve().parent
record=json.loads((run/'pre-policy-probe.json').read_text())
binary=Path(record['executable']).resolve(strict=True)
assert binary.is_relative_to((run/'target').resolve())
def digest():return hashlib.sha256(binary.read_bytes()).hexdigest()
assert digest()==record['executable_sha256']
environment=dict(os.environ,AIRK_PRE_POLICY_STORE_HELPER=str(binary))
command=['cargo','test','-p','airk-cli','--test','combined_reads','--all-features','--locked','--offline','permanent_trim_rejects_actual_pre_policy_store_handlers_after_retirement','--','--ignored','--test-threads=1']
result=subprocess.run(command,cwd=run/'source',env=environment)
assert digest()==record['executable_sha256']
(run/'pre-policy-execution.json').write_text(json.dumps({'command':command,'exit_code':result.returncode,'helper_sha256':record['executable_sha256'],'helper_unchanged':True,'origin_commit':record['origin_commit']},indent=2)+'\n')
raise SystemExit(result.returncode)
