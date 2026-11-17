# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,os,tarfile
base=pathlib.Path('/Users/tylerjensen/airk-builds/old-standalone-fixtures-20261107-1814')
r=json.loads((base/'campaign.json').read_text())
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
for c in r['candidates']:
 assert c['outcome']=='created'
 case=base/c['tag'];private=pathlib.Path(c['data_root'])
 for f in c['saved_data_inventory']:assert digest(private/f['path'])==f['sha256']
 for f in c['source_after']:assert digest(pathlib.Path(c['source'])/f['path'])==f['sha256']
 config=pathlib.Path(c['original_config']['path']);argv=c['original_config']['actual_stdio_argv']
 c['original_config']['previous_fixture_whitespace_sha256']=digest(config)
 # Match JavaScript JSON.stringify, as used by the original model.ts renderer.
 config.write_text('[mcp_servers.airk-'+c['collection']+']\ncommand = '+json.dumps(argv[0],separators=(',',':'))+'\nargs = '+json.dumps(argv[1:],separators=(',',':'))+'\n')
 c['original_config']['sha256']=digest(config);c['original_config']['bytes']=config.stat().st_size
 body=json.loads(c['mcp_status']['result']['content'][0]['text'])
 assert body['publication']==c['status']['publication']
 c['native_and_stdio_publication_equal']=True
 pids=[x['pid'] for x in c['commands']]+[c['mcp_process']['pid']]
 observations=[]
 for pid in pids:
  try:os.kill(pid,0);exists=True
  except ProcessLookupError:exists=False
  observations.append({'pid':pid,'exists_after_owned_wait':exists})
 assert not any(x['exists_after_owned_wait'] for x in observations),observations
 c['process_exit_observations']=observations
 archive=case/'saved-index-original.tar'
 with tarfile.open(archive,'x') as tar:
  for name in ['private-data','workspace','User/config.toml']:tar.add(case/name,arcname=name,recursive=True)
 os.chmod(archive,0o600)
 c['saved_index_archive']={'path':str(archive),'sha256':digest(archive),'bytes':archive.stat().st_size}
 (case/'receipt.json').write_text(json.dumps(c,indent=2)+'\n')
 print(json.dumps({'tag':c['tag'],'config_sha256':c['original_config']['sha256'],'saved_archive':c['saved_index_archive'],'all_children_exited':True,'publication_equal':True}))
(base/'campaign.json').write_text(json.dumps(r,indent=2)+'\n')
