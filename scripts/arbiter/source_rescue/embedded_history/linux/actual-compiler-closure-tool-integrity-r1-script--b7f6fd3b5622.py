# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,hashlib,urllib.request,re,tarfile,stat
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'toolchain/sysroot';assert B.resolve()==B and B.stat().st_uid==1000
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
r4=B/'evidence/compiler-smoke-r4.json';r=json.loads(r4.read_text());assert r['passed'] is False and r['failure']['type']=='FileNotFoundError';assert len(r['commands'])==6 and all(x['exit_code']==0 for x in r['commands']);assert r['commands'][2]['stdout']=='C:42\n' and r['commands'][3]['stdout']=='CPP:6\n'
invpath=pathlib.Path(r['inventory']['path']);assert sha(invpath)==r['inventory']['sha256'];inv=json.loads(invpath.read_text());byname={p['path']:p for p in inv}
for row in inv:
 p=S/row['path'];assert p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(S)
 if row['kind']=='file':assert sha(p)==row['sha256']
 if row['kind']=='link':assert os.readlink(p)==row['target']
cc1=list(S.glob('usr/libexec/gcc/*/*/cc1'));cc1plus=list(S.glob('usr/libexec/gcc/*/*/cc1plus'));assert len(cc1)==len(cc1plus)==1
paths=[B/'toolchain/bin/cc',B/'toolchain/bin/cxx',S/'usr/bin/x86_64-linux-gnu-gcc-13',S/'usr/bin/x86_64-linux-gnu-g++-13',*cc1,*cc1plus,pathlib.Path('/usr/bin/as'),pathlib.Path('/usr/bin/ld'),pathlib.Path('/usr/bin/ar')]
closure={'schema':'airk.private.rootless-compiler-readiness/1','passed':True,'uid':os.getuid(),'source':'Actual preserved r4 compile/run/readelf commands; no re-execution or outcome retag','prior_r4':{'path':str(r4),'sha256':sha(r4),'passed':False,'failure':r['failure']},'actual_compile_run_readelf_count':6,'actual_c_stdout':r['commands'][2]['stdout'],'actual_cpp_stdout':r['commands'][3]['stdout'],'post_inventory_reopened':len(inv),'sysroot_inventory':r['inventory'],'tool_pins':[{'path':str(p),'sha256':sha(p)} for p in paths],'layout':'Actual Debian gcc13 compiler phases are usr/libexec/gcc/...; final pin collector now observes existing signed-package/inventory-owned paths','no_compiler_or_test_replay':True}
cp=B/'evidence/compiler-readiness-r5.json';cp.write_text(json.dumps(closure,indent=2)+'\n');print('COMPILER',json.dumps({'passed':True,'receipt_sha256':sha(cp),'entries_reopened':len(inv),'cc1':str(cc1[0]),'cc1plus':str(cc1plus[0])}))
# Source-authenticated official Node keyring; never use the account's global GPG home.
req=urllib.request.Request('https://api.github.com/repos/nodejs/release-keys/commits/main',headers={'User-Agent':'AIRK-local-qualification'})
with urllib.request.urlopen(req,timeout=30) as f:commit=json.load(f)['sha']
req=urllib.request.Request('https://api.github.com/repos/nodejs/release-keys/contents/gpg?ref='+commit,headers={'User-Agent':'AIRK-local-qualification'})
with urllib.request.urlopen(req,timeout=30) as f:entries=json.load(f)
row=next(x for x in entries if x['name'] in ('pubring.kbx','pubring.gpg'))
with urllib.request.urlopen(row['download_url'],timeout=30) as f:keyring=f.read(1048577)
assert len(keyring)<1048576;kp=B/'downloads/node-release-pubring';kp.write_bytes(keyring);gh=B/'downloads/node-gpgv-home';gh.mkdir(mode=0o700)
verified=B/'downloads/node-verified-SHASUMS256.txt';cmd=['/usr/bin/gpgv','--homedir',str(gh),'--status-fd','1','--keyring',str(kp),'--output',str(verified),str(B/'downloads/node-SHASUMS256.txt.asc')];p=subprocess.run(cmd,capture_output=True,text=True,timeout=20);assert p.returncode==0 and '[GNUPG:] VALIDSIG ' in p.stdout,(p.returncode,p.stdout,p.stderr)
assert verified.read_bytes()==(B/'downloads/node-SHASUMS256.txt').read_bytes()
checksums=verified.read_text();assert next(l.split()[0] for l in checksums.splitlines() if l.endswith(' node-v24.15.0-win-x64.zip'))=='cc5149eabd53779ce1e7bdc5401643622d0c7e6800ade18928a767e940bb0e62'
record={'schema':'airk.private.linux-tool-integrity-pins/1','official_Node_key_source':'https://github.com/nodejs/release-keys','key_source_commit':commit,'keyring_url':row['download_url'],'keyring_sha256':sha(kp),'node_signature_command':cmd,'signature_exit_code':p.returncode,'signature_status':p.stdout,'signature_stderr':p.stderr,'node_verified_checksums_sha256':sha(verified),'known_retained_Windows_Node_hash_matches':True,'rustup_expected_sha256':(B/'downloads/rustup-init.sha256').read_text().split()[0],'node_expected_sha256':next(l.split()[0] for l in checksums.splitlines() if l.endswith(' node-v24.15.0-linux-x64.tar.xz')),'nextest_expected_sha256':'223ba936714cc861a9cdedad5a8eb664dfdd69d6fbdf6cea6aa8184f523290dc','executable_download_or_run':False};mp=B/'evidence/tool-integrity-pins-r1.json';mp.write_text(json.dumps(record,indent=2)+'\n');print('TOOL_PINS',json.dumps(record));print('TOOL_RECEIPT_SHA256',sha(mp))
