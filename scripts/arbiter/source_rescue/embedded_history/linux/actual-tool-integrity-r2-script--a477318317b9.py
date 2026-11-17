# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,urllib.request,subprocess,re
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
record={'schema':'airk.private.linux-tool-integrity-pins/2','passed':False,'uid':os.getuid(),'prior_failure':'official full keyring exceeded arbitrary1MiB staging read; prior mirror retained; now exact published immutable blob size/hash verified before GPG','executable_run':False};out=B/'evidence/tool-integrity-pins-r2.json'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def getjson(url):
 req=urllib.request.Request(url,headers={'User-Agent':'AIRK-local-qualification'})
 with urllib.request.urlopen(req,timeout=30) as f:return json.load(f)
try:
 commit=getjson('https://api.github.com/repos/nodejs/release-keys/commits/main')['sha'];entries=getjson('https://api.github.com/repos/nodejs/release-keys/contents/gpg?ref='+commit);row=next(x for x in entries if x['name'] in ('pubring.kbx','pubring.gpg'));assert 0<row['size']<=32*1024*1024
 record.update(key_source_commit=commit,keyring_url=row['download_url'],keyring_declared_size=row['size'],keyring_declared_git_blob=row['sha'])
 with urllib.request.urlopen(row['download_url'],timeout=30) as f:data=f.read(row['size']+1)
 assert len(data)==row['size'] and hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()==row['sha']
 kp=B/'downloads/node-release-pubring-r2';kp.write_bytes(data);gh=B/'downloads/node-gpgv-home-r2';gh.mkdir(mode=0o700)
 verified=B/'downloads/node-verified-SHASUMS256-r2.txt';cmd=['/usr/bin/gpgv','--homedir',str(gh),'--status-fd','1','--keyring',str(kp),'--output',str(verified),str(B/'downloads/node-SHASUMS256.txt.asc')];p=subprocess.run(cmd,capture_output=True,text=True,timeout=20);record.update(signature_command=cmd,signature_exit_code=p.returncode,signature_status=p.stdout,signature_stderr=p.stderr,keyring_sha256=sha(kp));assert p.returncode==0 and '[GNUPG:] VALIDSIG ' in p.stdout
 assert verified.read_bytes()==(B/'downloads/node-SHASUMS256.txt').read_bytes();checksums=verified.read_text();assert next(l.split()[0] for l in checksums.splitlines() if l.endswith(' node-v24.15.0-win-x64.zip'))=='cc5149eabd53779ce1e7bdc5401643622d0c7e6800ade18928a767e940bb0e62'
 record.update(node_verified_checksums_sha256=sha(verified),known_retained_Windows_Node_hash_matches=True,rustup_expected_sha256=(B/'downloads/rustup-init.sha256').read_text().split()[0],node_expected_sha256=next(l.split()[0] for l in checksums.splitlines() if l.endswith(' node-v24.15.0-linux-x64.tar.xz')),nextest_expected_sha256='223ba936714cc861a9cdedad5a8eb664dfdd69d6fbdf6cea6aa8184f523290dc',passed=True)
except BaseException as e:record['failure']={'type':type(e).__name__,'message':str(e)};raise
finally:out.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record));print('RECEIPT_SHA256',sha(out))
