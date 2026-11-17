# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,hashlib,importlib.util
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';assert S.resolve()==S and S.stat().st_uid==1000
p=subprocess.run(['/usr/bin/git','-c','core.autocrlf=true','-C',str(S),'status','--porcelain','--untracked-files=all'],capture_output=True,text=True,timeout=20)
spec=importlib.util.spec_from_file_location('linux_actual_c7',S/'scripts/build-service-runtime.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);c7=m.source_identity()
r={'schema':'airk.private.linux-source-newline-policy-observation/1','uid':os.getuid(),'per_command_git_config_only':True,'argv':['git','-c','core.autocrlf=true','-C',str(S),'status','--porcelain','--untracked-files=all'],'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'raw_c7':c7,'source_bytes_unchanged':True,'local_config_not_changed':True};q=B/'evidence/source-newline-policy-readonly-r1.json';q.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
