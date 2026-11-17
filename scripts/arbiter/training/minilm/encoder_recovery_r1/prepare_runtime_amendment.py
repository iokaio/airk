# SPDX-License-Identifier: Apache-2.0
"""Prepare one additive CUDA retry seal after exact primary runtime amendment review.
Never executes a child or removes a file/pin; preserves failed original seal/attempt.
"""
from pathlib import Path
import argparse,copy,hashlib,json,os
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'encoder-recovery-preparation-r1'

def sha(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def key(path):return os.path.normcase(str(Path(path).resolve()))
def amend_files(files,amendment,actual):
 old={key(f['path']):f for f in files};base={k:v for k,v in old.items() if Path(k).is_relative_to(Path(amendment['root']))};added={key(f['path']):f for f in amendment['added']};changed={key(f['path']):f for f in amendment['changed']}
 if amendment['removed'] or len(base)!=amendment['expected_before_count']:raise ValueError('old roster/removal differs')
 if set(base)-set(actual):raise ValueError('missing current file; never remove stale pin')
 if set(actual)-set(base)!=set(added) or len(actual)!=amendment['expected_after_count']:raise ValueError('unreviewed added file or count')
 differing={k for k,v in base.items() if actual[k]['sha256']!=v['sha256']}
 if differing!=set(changed):raise ValueError('unreviewed changed file')
 result=copy.deepcopy(files)
 for k,v in changed.items():
  if base[k]['sha256']!=v['expected'] or actual[k]['sha256']!=v['actual']:raise ValueError('old/current reviewed changed hash differs')
  target=next(f for f in result if key(f['path'])==k);target.update(sha256=v['actual'],bytes=actual[k]['bytes'])
 for k,v in added.items():
  if actual[k]['sha256']!=v['sha256'] or actual[k]['bytes']!=v['bytes']:raise ValueError('reviewed added file hash differs')
  result.append(copy.deepcopy(v))
 if len(result)!=len(files)+len(added) or {key(f['path']) for f in files}-{key(f['path']) for f in result}:raise ValueError('no original pin may disappear')
 return result

def main():
 require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
 for name in ('HERE','PREP','H','MODELS'):
  if name in globals():require_private_root(globals()[name])
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root-reviewed-amendment-sha256',required=True);a=p.parse_args();amend=HERE/'runtime-amendment-r2.json'
 if sha(amend)!=a.root_reviewed_amendment_sha256:raise ValueError('primary reviewed amendment changed')
 spec=json.loads(amend.read_bytes());old=HERE/'execution-seal-cuda.json'
 if sha(old)!=spec['base_execution_seal_sha256']:raise ValueError('original execution seal changed')
 base=json.loads(old.read_bytes());owner_path=Path(next(f['path'] for f in base['files'] if Path(f['path']).name=='training_execution.py'));expected=next(f['sha256'] for f in base['files'] if f['path']==str(owner_path))
 if sha(owner_path)!=expected:raise ValueError('original runtime owner source changed')
 import types
 owner=types.ModuleType('_unchanged_runtime_owner');exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__)
 actual={key(f):{'path':str(Path(f).resolve()),'sha256':sha(f),'bytes':Path(f).stat().st_size} for f in owner.runtime_paths(spec['root'])}
 base['files']=amend_files(base['files'],spec,actual);target=HERE/'execution-seal-cuda-r2.json';output=HERE/'cuda-equivalence-r2.json';log=HERE/'owned-cuda-r2';cache=HERE/'disabled-pycache-cuda-r2'
 if any(p.exists() for p in [target,output,log,cache]):raise ValueError('fresh r2 paths required')
 cmd=base['command'];pairs={'--output':str(output),'--execution-seal':str(target)}
 for flag,value in pairs.items():cmd[cmd.index(flag)+1]=value
 for n,value in enumerate(cmd):
  if value.startswith('pycache_prefix='):cmd[n]='pycache_prefix='+str(cache)
 base['environment']['PYTHONPYCACHEPREFIX']=str(cache);base['launch_log_dir']=str(log);base['source_closure_amendment']={'path':str(amend),'sha256':a.root_reviewed_amendment_sha256,'original_seal_sha256':spec['base_execution_seal_sha256'],'no_removed_pins':True,'synthetic_cuda_only':True}
 for path in [amend,Path(__file__).resolve()]:base['files'].append({'path':str(path),'sha256':sha(path),'bytes':path.stat().st_size})
 base['environment']['AIRK_RESEARCH_MODELS']=str(require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS')));target.write_text(json.dumps(base,indent=2)+'\n',encoding='utf-8');print(json.dumps({'seal':str(target),'sha256':sha(target),'log_dir':str(log),'files':len(base['files']),'limits':base['limits'],'executed':False}))
if __name__=='__main__':main()
