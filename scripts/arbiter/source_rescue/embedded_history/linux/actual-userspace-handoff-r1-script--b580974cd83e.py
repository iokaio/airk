# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,hashlib
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';R=B/'toolchain/compiler-runtime';P=B/'toolchain/sysroot'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
# Complete the same filtered helper-library rule used for GCC: include pkgconf,
# avoid injecting a target sysroot libc into host executable loader processes.
for src in (P/'usr/lib/x86_64-linux-gnu').glob('libpkgconf.so*'):
 d=R/src.name
 if not d.exists():d.symlink_to(os.path.relpath(src,R))
 assert d.resolve().is_relative_to(B)
wrapper=B/'toolchain/bin/pkg-config';wrapper.write_text('#!/bin/sh\nexport LD_LIBRARY_PATH='+str(R)+'\nexport PKG_CONFIG_SYSROOT_DIR='+str(P)+'\nexport PKG_CONFIG_LIBDIR='+str(P/'usr/lib/x86_64-linux-gnu/pkgconfig')+':'+str(P/'usr/share/pkgconfig')+'\nexec '+str(P/'usr/bin/pkgconf')+' "$@"\n');wrapper.chmod(0o700)
T=B/'rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin';env={**os.environ,'PATH':str(T)+':'+str(B/'nextest')+':'+str(B/'toolchain/bin')+':'+str(B/'node/node-v24.15.0-linux-x64/bin')+':/usr/bin:/bin','CARGO_HOME':str(B/'cargo'),'RUSTUP_HOME':str(B/'rustup'),'TMPDIR':str(B/'tmp'),'AIRK_BUNDLED_MODEL_DIR':str(B/'inputs/bundled-model'),'CC':str(B/'toolchain/bin/cc'),'CXX':str(B/'toolchain/bin/cxx'),'AR':'/usr/bin/ar','PKG_CONFIG':str(wrapper),'PYTHONDONTWRITEBYTECODE':'1'}
versions=[]
for argv in [[str(B/'toolchain/bin/make'),'--version'],[str(wrapper),'--version']]:
 p=subprocess.run(argv,env=env,cwd=B,capture_output=True,text=True,timeout=15);versions.append({'argv':argv,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr});assert p.returncode==0
selected=['compiler-readiness-r5.json','user-toolchain-readiness-r2.json','source-and-native-inputs-r3.json','user-executable-acquisition-r1.json','signed-package-roster-r1.json']
receipts=[{'path':str(B/'evidence'/n),'sha256':sha(B/'evidence'/n)} for n in selected]
original_install=B/'evidence/user-toolchain-install-r1.json';manifest=json.loads((B/'source-transfer/source-manifest.json').read_text());assert subprocess.check_output(['git','-C',str(S),'status','--porcelain']).strip()==b''
assert all((S/r['path']).stat().st_size==r['bytes'] and sha(S/r['path'])==r['sha256'] for r in manifest['files'])
r={'schema':'airk.private.linux-userspace-handoff/1','passed':True,'uid':os.getuid(),'user':'tyler','distro_guid':'{b504d8ef-f4f8-4c59-aeb2-b0c91302bf19}','source_root':str(S),'head':manifest['commit'],'tree':manifest['normalized_tree'],'raw_c7':manifest['raw_c7'],'all3511_source_hashes_still_match':True,'git_clean':True,'readiness_receipts':receipts,'original_install_correct_path':{'path':str(original_install),'sha256':sha(original_install)},'prior_readiness_path_label_correction':'r2 prior_install_receipt.path accidentally retained loop variable; its SHA and actual install/source observations are correct. This field binds the real original install receipt without retagging r2.','additional_tool_versions':versions,'tool_pins':[{'path':str(p),'sha256':sha(p)} for p in [B/'toolchain/bin/cc',B/'toolchain/bin/cxx',wrapper,P/'usr/bin/make']], 'environment_for_future_owned_child':{k:env[k] for k in ['PATH','CARGO_HOME','RUSTUP_HOME','TMPDIR','AIRK_BUNDLED_MODEL_DIR','CC','CXX','AR','PKG_CONFIG','PYTHONDONTWRITEBYTECODE']},'make_alias':{'path':str(B/'toolchain/bin/make'),'target':os.readlink(B/'toolchain/bin/make')},'component_boundary':'userspace toolchain/source/models/static archives ready; zero Cargo/build/check/test, ORT/CMake/Ninja builds not run'}
p=B/'evidence/userspace-handoff-r1.json';p.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));print('RECEIPT_SHA256',sha(p))
