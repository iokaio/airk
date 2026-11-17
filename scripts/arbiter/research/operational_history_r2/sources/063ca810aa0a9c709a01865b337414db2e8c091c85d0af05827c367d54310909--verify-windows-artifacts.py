# SPDX-License-Identifier: Apache-2.0
import hashlib,json,pathlib,struct,zipfile,stat
BASE=pathlib.Path(__file__).resolve().parent
releases=json.loads((BASE.parent/'historical-macos/releases.json').read_text())
report={'schema':1,'repository':'iokaio/airk','executed':False,'candidates':[]}
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
for tag,version,layout in [('v1.3.1','1.3.1','collections-1.2'),('v1.0.0-build.7','1.0.0','collections')]:
 release=next(r for r in releases if r['tag_name']==tag);assert not release['draft']
 case=BASE/tag;names=[f'airk-{version}-win32-x64.vsix',f'airk-{version}-win32-x64.vsix-manifest.json','release.json','SHA256SUMS']
 assets=[]
 for name in names:
  a=next(a for a in release['assets'] if a['name']==name);p=case/name;sha=digest(p)
  assert p.stat().st_size==a['size'] and 'sha256:'+sha==a['digest']
  assets.append({'name':name,'asset_id':a['id'],'url':a['browser_download_url'],'bytes':p.stat().st_size,'sha256':sha,'github_digest_matched':True})
 sums={line.split()[1].lstrip('*'):line.split()[0] for line in (case/'SHA256SUMS').read_text().splitlines() if line.strip()}
 for asset in assets:
  if asset['name']!='SHA256SUMS':assert sums[asset['name']]==asset['sha256']
 vm=json.loads((case/names[1]).read_text());assert vm['archive_sha256']==assets[0]['sha256'] and vm['platform']=='win32-x64'
 with zipfile.ZipFile(case/names[0]) as z:
  for info in z.infolist():
   path=pathlib.PurePosixPath(info.filename)
   assert not path.is_absolute() and '..' not in path.parts and not stat.S_ISLNK(info.external_attr>>16)
  for name,sha in vm['payload'].items():
   entry={'LICENSE':'LICENSE.txt','README.md':'readme.md'}.get(name,name)
   assert hashlib.sha256(z.read('extension/'+entry)).hexdigest()==sha,(tag,name)
  native=json.loads(z.read('extension/native/manifest.json'));assert native['schema']=='airk-native-candidate/1' and native['target']=='x86_64-pc-windows-msvc' and native['vsix_target']=='win32-x64' and native['commit']==release['target_commitish']
  assert hashlib.sha256(z.read('extension/native/manifest.json')).hexdigest()==vm['native_manifest_sha256']
  for name,sha in native['payload'].items():assert hashlib.sha256(z.read('extension/native/'+name)).hexdigest()==sha,(tag,name)
  exe=z.read('extension/native/airk.exe');offset=struct.unpack_from('<I',exe,0x3c)[0]
  assert exe[:2]==b'MZ' and exe[offset:offset+4]==b'PE\0\0' and struct.unpack_from('<H',exe,offset+4)[0]==0x8664
 report['candidates'].append({'tag':tag,'package_version':version,'layout':layout,'commit':release['target_commitish'],'published_at':release['published_at'],'release_url':release['html_url'],'assets':assets,'native':{'entry':'extension/native/airk.exe','schema':native['schema'],'sha256':hashlib.sha256(exe).hexdigest(),'bytes':len(exe),'pe_machine':'AMD64/0x8664','payload_entries_verified':len(native['payload']),'vsix_payload_entries_verified':len(vm['payload']),'manifest_sha256':vm['native_manifest_sha256']}})
(BASE/'verified-artifacts.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'verified':len(report['candidates']),'native':[(c['tag'],c['native']['sha256']) for c in report['candidates']]}))
