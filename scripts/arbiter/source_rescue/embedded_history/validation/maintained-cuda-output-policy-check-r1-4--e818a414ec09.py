# SPDX-License-Identifier: Apache-2.0

import importlib.util,json,time,hashlib
from pathlib import Path
root=Path('D:/code/Github/airk');p=root/'scripts/cuda-static-runtime/verification/policy_owner.py'
assert hashlib.sha256(p.read_bytes()).hexdigest()=='3d2c63a238cd85952208dc572794cf9dba4adeae5f4d820fc10ab5f8c0aa4183'
spec=importlib.util.spec_from_file_location('policy',p);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
lockpath=root/'artifacts/v190/final-plan/exec-20261116T052942Z/gpu-runtime-strategy-proposal-r1/gpu-provision-owner-proposal-r8/cuda-runtime-producer-lock.json'
raw=lockpath.read_bytes();assert hashlib.sha256(raw).hexdigest()=='549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d'
lock=json.loads(raw);target=owner.PARENT/'gpu-maintained-policy-check-r1';assert not target.exists();lock['build_path']['fresh_child']=target.name
start=time.monotonic();deadline=start+30;cpu=owner.cpu_owner(lock,deadline=deadline);policy=owner.identity_and_parent(cpu,target,lock,deadline=deadline);assert not target.exists()
print(json.dumps({'status':'PASS_READONLY_POLICY_ONLY','target':str(target),'target_created':False,'elapsed_seconds':time.monotonic()-start,'policy':policy,'policy_owner_sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'historical_lock_sha256':hashlib.sha256(raw).hexdigest(),'native_execution':False,'qualification':False}))
