# SPDX-License-Identifier: Apache-2.0
"""Root-only final native gates; default prints commands and NEVER invokes Cargo.
Requires sole Cargo ownership and no concurrent native model/resource window.
"""
import argparse,hashlib,json,os,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
CARGO=Path('C:/Users/tdotn/.cargo/bin/cargo.exe')
TARGET=ROOT/'artifacts/v190/p10-target'
MODEL=ROOT/'artifacts/advanced-split/bundled-model-v3'
TEST_MODEL=MODEL/'minilm-l6-v2-u8-v3'
TEST_MODEL_METADATA_SHA256='21da4db318fa063450d55a40f3e2dc9c6097b2950d3510e7ff6055f0e5eb8924'
ORT=ROOT/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json'
COMMANDS={
 'workspace-clippy':[str(CARGO),'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings'],
 'lightweight-main':[str(CARGO),'clippy','-p','airk-cli','--no-default-features','--all-targets','--locked','--timings','--','-D','warnings'],
 'workspace-tests-restored':[str(CARGO),'nextest','run','--profile','ci','--workspace','--all-features','--locked','--timings','--ignore-default-filter','--fail-fast'],
 'workspace-doctests':[str(CARGO),'test','--doc','--workspace','--all-features','--locked','--timings'],
 'ignored-vector-performance':[str(CARGO),'test','-p','airk-kernel','--no-default-features','--features','local-inference','--test','vector_performance','--locked','--timings','--','--ignored','--test-threads=1'],
}
def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--only',choices=COMMANDS,action='append',required=True)
 p.add_argument('--out',type=Path,required=True,help='Fresh receipt directory, outside target')
 p.add_argument('--execute',action='store_true')
 a=p.parse_args()
 env=dict(os.environ)
 values=json.loads(ORT.read_bytes())['environment']
 assert set(values)=={'LIBONNXRUNTIME_NO_PKG_CONFIG','ORT_SKIP_DOWNLOAD','ORT_PREFER_DYNAMIC_LINK','ORT_LIB_LOCATION','ORT_LIB_PROFILE'}
 assert values['ORT_PREFER_DYNAMIC_LINK']=='0' and values['ORT_SKIP_DOWNLOAD']=='1'
 assert TARGET.is_dir() and MODEL.is_dir() and Path(values['ORT_LIB_LOCATION']).is_dir()
 assert TEST_MODEL.is_dir() and (TEST_MODEL/'model.json').is_file()
 assert hashlib.sha256((TEST_MODEL/'model.json').read_bytes()).hexdigest()==TEST_MODEL_METADATA_SHA256, 'CPU U8-v3 metadata differs from reviewed actual profile'
 assert all((TEST_MODEL/name).is_file() for name in ['model.onnx','tokenizer.json','LICENSE'])
 env.update(values)
 env.update(CARGO_TARGET_DIR=str(TARGET),CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='1',CARGO_BUILD_JOBS='8',AIRK_BUNDLED_MODEL_DIR=str(MODEL),AIRK_TEST_MODEL_DIR=str(TEST_MODEL))
 selected=list(dict.fromkeys(a.only))
 for name in selected:
  cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures,harness','--tier','T3','--out',str(a.out),'--target-dir',str(TARGET)]
  # Whole unfiltered doctest compilation may legitimately have no authored examples.
  # This exception does not apply to any filtered or ordinary test selection.
  if name in {'workspace-clippy','lightweight-main','workspace-doctests'}:cmd+=['--allow-no-tests']
  cmd += [name,*COMMANDS[name]]
  print(json.dumps({'name':name,'command':cmd,'target':str(TARGET),'incremental':'1','ort_env_source':str(ORT),'model':str(MODEL),'test_model':str(TEST_MODEL),'test_model_metadata_sha256':TEST_MODEL_METADATA_SHA256,'execute':a.execute}),flush=True)
  if a.execute:
   result=subprocess.run(cmd,cwd=ROOT,env=env)
   if result.returncode:return result.returncode
 return 0
if __name__=='__main__':raise SystemExit(main())
