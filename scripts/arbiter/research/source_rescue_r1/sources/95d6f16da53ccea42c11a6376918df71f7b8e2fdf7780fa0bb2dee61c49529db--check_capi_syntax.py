# SPDX-License-Identifier: Apache-2.0
"""Root-owned syntax check only, explicit execution; no full native build."""
from pathlib import Path
import json,hashlib,importlib.util,argparse,time
HERE=Path(__file__).resolve().parent
def digest(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(path,name):
 s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def main():
 a=argparse.ArgumentParser();a.add_argument('--execute-syntax',action='store_true');v=a.parse_args()
 if not v.execute_syntax:raise ValueError('explicit Root syntax execution required')
 req=json.loads((HERE/'continuation-request.json').read_bytes());recipe=load(req['recipe']['path'],'reviewed_r8');lock=json.loads(Path(req['recipe_lock']['path']).read_bytes());o=recipe.cpu_owner(lock)
 for key in ['recipe','recipe_lock','before','after','patch','syntax_card','syntax_response','parent_receipt','parent_failed_log','parent_source_inventory']:
  row=req[key];o.verify_file(o.plain(row['path'],regular=True),row)
 card=json.loads(Path(req['syntax_card']['path']).read_bytes());parent=json.loads(Path(req['parent_receipt']['path']).read_bytes())
 output=recipe.PARENT/'capi-syntax-r9'
 # Reuse the exact same-user/protected-parent guard with this fresh declared child.
 identity_lock=dict(lock);identity_lock['build_path']={'fresh_child':output.name};policy=recipe.identity_and_parent(o,output,identity_lock);output.mkdir()
 receipt={'schema':'airk.private.cuda-capi-syntax-result/1','status':'RUNNING','request_sha256':digest(HERE/'continuation-request.json'),'card_sha256':digest(req['syntax_card']['path']),'source_sha256':req['after']['sha256'],'policy':policy}
 start=time.monotonic()
 try:
  for row in parent['toolchain'].values():
   if isinstance(row,dict) and {'path','bytes','sha256'}.issubset(row):o.verify_file(o.plain(row['path'],regular=True),row)
  if digest(card['argv'][0])!=card['compiler']['sha256']:raise ValueError('captured compiler spelling is a different image')
  actual=o.windows_command(card['argv'],parent['toolchain'],output,'capi-syntax-r9')
  receipt['command']=o.run_owned(actual,Path(card['cwd']),output/'logs/capi-syntax-r9.log',120)
  if not o.successful(receipt['command']):raise ValueError('actual CAPI /Zs failed or unsettled')
  for key in ['after','syntax_response','syntax_card','parent_receipt','parent_source_inventory']:
   row=req[key];o.verify_file(o.plain(row['path'],regular=True),row)
  receipt['status']='PASS_SYNTAX_ONLY'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:receipt['elapsed_seconds']=time.monotonic()-start;recipe.write(output/'receipt.json',receipt)
 print(json.dumps({'output':str(output),'status':receipt['status']}))
if __name__=='__main__':main()
