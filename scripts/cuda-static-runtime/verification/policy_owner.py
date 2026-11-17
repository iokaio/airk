# SPDX-License-Identifier: Apache-2.0
"""Maintained same-human policy for fresh CUDA execution children; no ACL changes."""
import hashlib,importlib.util,json,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
PARENT=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z')
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
GUARD_SHA='57592dbbbefc63030c97a1d3485bc5c4fda7c2243d792b7399432b129ea4c57f'
POWERSHELL=Path('C:/Program Files/PowerShell/7/pwsh.exe')
def digest(path):
 with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def check_deadline(deadline):
 if deadline is not None and time.monotonic()>=deadline:raise TimeoutError('original CUDA parent deadline expired during output policy')
def cpu_owner(lock,*,deadline=None,loader=None):
 check_deadline(deadline)
 if digest(ROOT/'scripts/provision-static-onnx-runtime.py')!=lock['cpu_owner_sha256'] or digest(ROOT/'scripts/static-onnx-runtime-lock.json')!=lock['cpu_lock_sha256']:raise ValueError('canonical CPU owner drift')
 path=ROOT/'scripts/provision-static-onnx-runtime.py'
 if loader is None:
  spec=importlib.util.spec_from_file_location('airk_cpu_owner',path);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
 else:owner=loader(path,'airk_cpu_owner')
 owner.validate_lock();check_deadline(deadline);return owner
def expected_parent_sddl():
 return 'O:'+SID+'G:'+SID+'D:AI(A;OICIID;0x1200a9;;;S-1-5-21-3678370870-3858642417-826968105-1011)(A;OICIID;FA;;;SY)(A;OICIID;FA;;;BA)(A;OICIID;FA;;;'+SID+')'
def identity_and_parent(owner,destination,lock,*,deadline=None):
 check_deadline(deadline)
 destination=Path(destination);owner.plain(PARENT);owner.plain(destination)
 if destination.parent!=PARENT or destination.name!=lock['build_path']['fresh_child'] or destination.exists():raise ValueError('exact fresh child of approved protected parent required')
 guard=ROOT/'scripts/dev/assert-human-execution.ps1'
 if digest(guard)!=GUARD_SHA:raise ValueError('reviewed maintained human guard drift')
 def quote(value):return "'"+str(value).replace("'","''")+"'"
 code="$ErrorActionPreference='Stop'; . "+quote(guard)+"; Assert-AirkOutputParent -Path "+quote(destination)+" -Root "+quote(PARENT)+" -ExpectedSid "+quote(SID)+" | Out-Null; $a=Get-Acl -LiteralPath "+quote(PARENT)+"; @{owner=$a.Owner;sddl=$a.Sddl}|ConvertTo-Json -Compress"
 result=subprocess.run([str(POWERSHELL),'-NoProfile','-Command',code],check=True,capture_output=True,text=True,timeout=min(10,deadline-time.monotonic()) if deadline is not None else 10)
 check_deadline(deadline)
 policy=json.loads(result.stdout)
 if policy.get('owner')!='TYAI\\tdotn' or policy.get('sddl')!=expected_parent_sddl():raise ValueError('protected output-parent exact historical policy drift')
 return policy
