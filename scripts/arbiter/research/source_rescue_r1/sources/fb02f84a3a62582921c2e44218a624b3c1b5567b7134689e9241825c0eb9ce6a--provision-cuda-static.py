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
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--repo-root',type=Path,default=Path(__file__).resolve().parents[1]);ap.add_argument('--destination',type=Path,required=True);ap.add_argument('--producer-file',type=Path,required=True);ap.add_argument('--lock-file',type=Path,required=True);ap.add_argument('--receipt-sha256',required=True);ap.add_argument('--aggregate',action='store_true');ap.add_argument('--write-environment',type=Path);a=ap.parse_args();runtime=load('cuda-static-runtime');aggregate=load('cuda-static-aggregate');env=load('cuda-runtime-environment');o=runtime.load(a.repo_root/'scripts/provision-static-onnx-runtime.py','cpu_acquisition_owner');destination=o.plain(a.destination)
 build=runtime.verify_build(destination,a.producer_file,a.lock_file,a.receipt_sha256,a.repo_root)
 if a.aggregate:
  if (destination/'gpu-static-aggregate-receipt.json').exists():proof=aggregate.verify_aggregate(destination,build,o)
  else:proof=aggregate.aggregate(destination,build,o)
 else:proof=aggregate.verify_aggregate(destination,build,o)
 result=env.handoff_value(destination,a.producer_file,a.lock_file,a.receipt_sha256,build,proof)
 if a.write_environment:
  target=o.plain(a.write_environment)
  if target.exists():raise ValueError('existing GPU environment retained; no overwrite')
  target.parent.mkdir(parents=True,exist_ok=True)
  with target.open('xb') as f:f.write((json.dumps(result,indent=2)+'\n').encode())
 print(json.dumps(result,indent=2))
if __name__=='__main__':main()
