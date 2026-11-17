# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,importlib.util,subprocess,datetime
root=Path.cwd();out=root/'artifacts/v190/final-plan/exec-20261116T052942Z/candidate-overlay-preparation-r3';spec=importlib.util.spec_from_file_location('candidate_index_metadata_proof',out/'candidate_overlay.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);m.identity();plan=json.loads((out/'postcommit-plan.json').read_bytes());candidate=Path(plan['candidate']);tree=m.tree_rows(candidate);assert m.tree_rows(root)==tree and m.revision(root)==m.revision(candidate)==plan['commit'];root_index=Path(m.git(root,'rev-parse','--git-path','index').decode().strip());root_index=root_index if root_index.is_absolute() else root/root_index;root_index_before=m.sha(root_index)
def catalog(base):return m.git(base,'ls-files','--stage','-z')
before_catalog=catalog(candidate);root_catalog=catalog(root);assert before_catalog==root_catalog
status=m.git(candidate,'status','--porcelain=v1','-z');dirty=[]
for row in status.split(bytes([0])):
 if not row:continue
 name=row[3:].decode('utf-8');assert row[:3] in [b' M ',b'M  '],row;assert name in tree and m.public_name(name),name;dirty.append(name)
normalized=m.normalized_blobs(candidate,tree);assert all(normalized[name]==entry['blob'] for name,entry in tree.items())
for row in plan['source_files']:assert m.sha(candidate/row['path'])==m.sha(root/row['path'])==row['sha256']
index=Path(m.git(candidate,'rev-parse','--git-path','index').decode().strip());index=index if index.is_absolute() else candidate/index;before_index=m.sha(index);refresh=subprocess.run(['git','--no-optional-locks','update-index','--refresh'],cwd=candidate,text=True,encoding='utf-8',capture_output=True);after_refresh=m.git(candidate,'status','--porcelain=v1','-z');proof={'schema':'airk.private.candidate-r3-index-normalization/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'commit':plan['commit'],'candidate':str(candidate),'actual_git_filtered_blobs_equal_head_and_index_before_write':3508,'full_index_catalog_sha256_before':hashlib.sha256(before_catalog).hexdigest(),'index_sha256_before':before_index,'plain_refresh':{'argv':['git','--no-optional-locks','update-index','--refresh'],'exit_code':refresh.returncode,'remaining_status_paths':len([x for x in after_refresh.split(bytes([0])) if x])},'initial_dirty_count':len(dirty),'content_reconciliation':None}
if after_refresh:
 current=[]
 for row in after_refresh.split(bytes([0])):
  if not row:continue
  name=row[3:].decode('utf-8');assert name in dirty and name in tree;current.append(name)
 checked=m.normalized_blobs(candidate,current);assert all(checked[name]==tree[name]['blob'] for name in current)
 pathspec=out/'proven-stat-paths.nul';pathspec.write_bytes(b''.join(name.encode('utf-8')+bytes([0]) for name in current));command=['git','--no-optional-locks','add','--pathspec-from-file='+str(pathspec),'--pathspec-file-nul'];added=subprocess.run(command,cwd=candidate,text=True,encoding='utf-8',capture_output=True);proof['content_reconciliation']={'argv':command,'exact_proven_paths':len(current),'pathspec_sha256':m.sha(pathspec),'exit_code':added.returncode,'stdout':added.stdout,'stderr':added.stderr};assert added.returncode==0,added.stderr
assert catalog(candidate)==before_catalog==catalog(root);assert m.git(candidate,'status','--porcelain=v1','-z')==b'';assert m.sha(root_index)==root_index_before
for row in plan['source_files']:assert m.sha(candidate/row['path'])==m.sha(root/row['path'])==row['sha256']
assert m.native_seal(root)==m.native_seal(candidate)==plan['native_root_source_identity'];proof.update(index_sha256_after=m.sha(index),root_index_sha256=root_index_before,root_index_unchanged=True,full_index_blob_mode_catalog_unchanged=True,source_raw_bytes_unchanged=True,candidate_status_clean=True,no_assume_unchanged_skip_worktree_reset_filter_acl_override=True,passed=True);(out/'index-normalization.json').write_text(json.dumps(proof,indent=2)+chr(10),encoding='utf-8');print(json.dumps({'passed':True,'initial_dirty':len(dirty),'plain_refresh_exit':refresh.returncode,'reconciled_paths':proof['content_reconciliation']['exact_proven_paths'] if proof['content_reconciliation'] else 0,'receipt_sha256':m.sha(out/'index-normalization.json')},indent=2))
