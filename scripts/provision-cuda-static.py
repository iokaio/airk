# SPDX-License-Identifier: Apache-2.0
"""Explicit separately-owned GPU producer/aggregate/handoff command. No model/publish.
Requires an actual Root-reviewed completed receipt; missing evidence never falls back
through the CPU owner. This is a private proposal until reviewed/applied.
"""
import argparse,importlib.util,json
from pathlib import Path

def load(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--repo-root',type=Path,default=Path(__file__).resolve().parents[1]);ap.add_argument('--destination',type=Path,required=True);ap.add_argument('--producer-file',type=Path);ap.add_argument('--lock-file',type=Path);ap.add_argument('--continuation-root',type=Path);ap.add_argument('--continuation-proposal',type=Path);ap.add_argument('--r12-continuation-root',type=Path);ap.add_argument('--r12-proposal',type=Path);ap.add_argument('--checkpointed-root',type=Path);ap.add_argument('--checkpointed-proposal',type=Path);ap.add_argument('--stage-providers',type=Path);ap.add_argument('--receipt-sha256',required=True);ap.add_argument('--aggregate',action='store_true');ap.add_argument('--write-environment',type=Path);a=ap.parse_args();runtime=load('cuda-static-runtime');aggregate=load('cuda-static-aggregate');env=load('cuda-runtime-environment');o=runtime.load(a.repo_root/'scripts/provision-static-onnx-runtime.py','cpu_acquisition_owner');destination=o.plain(a.destination)
 if a.checkpointed_root is not None or a.checkpointed_proposal is not None:
  if a.checkpointed_root is None or a.checkpointed_proposal is None or any(x is not None for x in [a.r12_continuation_root,a.r12_proposal,a.continuation_root,a.continuation_proposal,a.producer_file,a.lock_file]):raise ValueError('explicit complete checkpointed mode required')
  build=load('cuda-static-checkpointed').verify_checkpointed(destination,a.checkpointed_root,a.checkpointed_proposal,a.receipt_sha256,a.repo_root)
 elif a.r12_continuation_root is not None or a.r12_proposal is not None:
  if a.r12_continuation_root is None or a.r12_proposal is None or any(x is not None for x in [a.continuation_root,a.continuation_proposal,a.producer_file,a.lock_file]):raise ValueError('explicit complete R12 mode required')
  build=load('cuda-static-continuation-r12').verify_continuation(destination,a.r12_continuation_root,a.r12_proposal,a.receipt_sha256,a.repo_root)
 elif a.continuation_root is not None or a.continuation_proposal is not None:
  if a.continuation_root is None or a.continuation_proposal is None or a.producer_file is not None or a.lock_file is not None:raise ValueError('explicit complete continuation mode required')
  build=load('cuda-static-continuation').verify_continuation(destination,a.continuation_root,a.continuation_proposal,a.receipt_sha256,a.repo_root)
 else:
  if a.producer_file is None or a.lock_file is None:raise ValueError('explicit original GPU producer inputs required')
  build=runtime.verify_build(destination,a.producer_file,a.lock_file,a.receipt_sha256,a.repo_root)
 if a.aggregate:
  if (destination/'gpu-static-aggregate-receipt.json').exists():proof=aggregate.verify_aggregate(destination,build,o)
  else:proof=aggregate.aggregate(destination,build,o)
 else:proof=aggregate.verify_aggregate(destination,build,o)
 result=env.checkpoint_handoff_value(destination,a.checkpointed_root,a.checkpointed_proposal,a.receipt_sha256,build,proof) if a.checkpointed_root is not None else env.r12_handoff_value(destination,a.r12_continuation_root,a.r12_proposal,a.receipt_sha256,build,proof) if a.r12_continuation_root is not None else env.continuation_handoff_value(destination,a.continuation_root,a.continuation_proposal,a.receipt_sha256,build,proof) if a.continuation_root is not None else env.handoff_value(destination,a.producer_file,a.lock_file,a.receipt_sha256,build,proof)
 if a.stage_providers:
  if a.checkpointed_root is not None:load('cuda-provider-staging').stage_checkpointed_providers(destination,build,proof,a.stage_providers,a.checkpointed_root,a.checkpointed_proposal,a.receipt_sha256,a.repo_root)
  elif a.r12_continuation_root is None:raise ValueError('provider staging only enabled for exact R12 proof')
  else:load('cuda-provider-staging').stage_providers(destination,build,proof,a.stage_providers,a.r12_continuation_root,a.r12_proposal,a.receipt_sha256,a.repo_root)
 if a.write_environment:
  target=o.plain(a.write_environment)
  if target.exists():raise ValueError('existing GPU environment retained; no overwrite')
  target.parent.mkdir(parents=True,exist_ok=True)
  with target.open('xb') as f:f.write((json.dumps(result,indent=2)+'\n').encode())
 print(json.dumps(result,indent=2))
if __name__=='__main__':main()
