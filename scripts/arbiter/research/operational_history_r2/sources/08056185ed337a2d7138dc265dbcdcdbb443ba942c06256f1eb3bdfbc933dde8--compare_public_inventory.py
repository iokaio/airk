# SPDX-License-Identifier: Apache-2.0
"""Compare exact native emitted asset pins to complete public file inventories."""
import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
REPO=ROOT.parents[2]

def matches(asset, remote, local_small=None):
 if type(remote.get('size')) is not int or remote['size']!=asset['bytes']:return False
 lfs=remote.get('lfs')
 if lfs is not None:return lfs.get('size')==asset['bytes'] and lfs.get('sha256')==asset['sha256']
 if local_small is None:return False
 if len(local_small)!=asset['bytes'] or hashlib.sha256(local_small).hexdigest()!=asset['sha256']:return False
 return hashlib.sha1(b'blob '+str(len(local_small)).encode()+b'\0'+local_small).hexdigest()==remote.get('blobId')

def main():
 facts_path=REPO/'artifacts/advanced-split/receipts/stage-integration-r98/candidate-facts.native-r2.json'
 raw=facts_path.read_bytes();assert hashlib.sha256(raw).hexdigest()=='bc9db666d9c2636873529a65e07ec103a5d35ea8de7b9621274f4582df34cd0b'
 facts=json.loads(raw);assert facts['schema']=='airk.private-final-distribution-candidate-facts/1'
 masters={'arbiter-focus-qwen3-reranker-06b':'artifacts/advanced-split/focus-native-qualification-preparation-r1/selected-focus-profile-r3/selected-profiles.private.json','arbiter-context-qwen3-17b':'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r1/selected-context-17-profile-r1/selected-profiles.private.json','arbiter-context-qwen3-4b':'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r2/selected-context-4-profile-r2/selected-profiles.private.json'}
 canonical={'arbiter-focus-qwen3-reranker-06b':'Qwen/Qwen3-Reranker-0.6B','arbiter-context-qwen3-17b':'Qwen/Qwen3-1.7B','arbiter-context-qwen3-4b':'Qwen/Qwen3-4B'}
 inventories=json.loads((ROOT/'inventories.json').read_bytes());sources=[]
 for pin in inventories['sources']:
  raw=Path(pin['path']).read_bytes();assert len(raw)==pin['bytes'] and hashlib.sha256(raw).hexdigest()==pin['sha256'];sources.append(json.loads(raw))
 matrix=[]
 for profile in facts['profiles']:
  master=json.loads((REPO/masters[profile['model']]).read_bytes())['profiles'][0]
  assert master['manifest']==profile['manifest'] and master['profile_sha256']==profile['profile_sha256']
  for asset in profile['assets']:
   small=None
   if asset['role'] in ('notice','compiler','calibration','fixture'):
    assert asset['bytes']<=1<<20
    small=Path(master['files'][asset['name']]).read_bytes()
    assert len(small)==asset['bytes'] and hashlib.sha256(small).hexdigest()==asset['sha256']
   found=[]
   for inventory in sources:
    for remote in inventory['siblings']:
     if matches(asset,remote,small):found.append({'repository':inventory['id'],'revision':inventory['sha'],'filename':remote['rfilename'],'url':f"https://huggingface.co/{inventory['id']}/resolve/{inventory['sha']}/{remote['rfilename']}",'evidence':'publisher LFS SHA256/size' if remote.get('lfs') else 'publisher Git blob plus exact local small-asset SHA256/size'})
   found.sort(key=lambda value:(value['repository']!=canonical[profile['model']],not value['repository'].startswith('Qwen/'),value['repository'],value['filename']))
   # No same-repo Reranker LICENSE exists: distinct Base source is explicit.
   row={'model':profile['model'],'asset':asset,'canonical_primary':found[0] if found else None,'verified_byte_identical_backups':found[1:],'status':'EXACT_PUBLIC_BYTES_DECLARED' if found else 'NO_MATCH_IN_CHECKED_PUBLISHER_AND_MAINTAINED_ONNX_REPOS_PUBLICATION_REQUIRED','runtime_source_approved':False,'actual_download_verified':False}
   matrix.append(row)
 result={'schema':'airk.private-selected-public-origin-matrix/1','scope':'Exact R98 raw assets only; future fitted calibration requires a new additive row. Public metadata/byte identity is not final qualification or a claimed transfer. Search is bounded to four official repositories and three maintained ONNX repositories, not the entire internet.','native_facts_sha256':'bc9db666d9c2636873529a65e07ec103a5d35ea8de7b9621274f4582df34cd0b','assets':matrix,'public_exact_rows':sum(row['canonical_primary'] is not None for row in matrix),'unpublished_rows':sum(row['canonical_primary'] is None for row in matrix),'model_or_tokenizer_bytes_read':False,'uploaded':False,'qualification_granted':False}
 with (ROOT/'asset-origin-matrix.raw-r1.json').open('x',encoding='utf-8') as stream:json.dump(result,stream,indent=2);stream.write('\n')
 print(json.dumps({'assets':len(matrix),'public_exact_rows':result['public_exact_rows'],'unpublished_rows':result['unpublished_rows'],'missing':[(r['model'],r['asset']['name']) for r in matrix if not r['canonical_primary']]}))
if __name__=='__main__':main()
