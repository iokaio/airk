# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,hashlib
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';manifest=json.loads((B/'source-transfer/source-manifest.json').read_text());rows={r['path']:r for r in manifest['files']};observed=[]
for name in ['.gitattributes','crates/airk-core/Cargo.toml','crates/airk-host/src/bundled_assets.rs','AGENTS.md']:
 data=(S/name).read_bytes();normalized=data.replace(b'\r\n',b'\n')
 rawblob=hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest();normblob=hashlib.sha1(b'blob '+str(len(normalized)).encode()+b'\0'+normalized).hexdigest()
 p=subprocess.run(['git','-C',str(S),'rev-parse','HEAD:'+name],capture_output=True,text=True)
 observed.append({'path':name,'raw_sha256':hashlib.sha256(data).hexdigest(),'manifest_sha256':rows[name]['sha256'],'manifest_git_blob':rows[name]['blob'],'HEAD_git_blob':p.stdout.strip(),'raw_git_blob':rawblob,'LF_normalized_git_blob':normblob,'CRLF_count':data.count(b'\r\n'),'LF_count':data.count(b'\n')})
print(json.dumps(observed))
