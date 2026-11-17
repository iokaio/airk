# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,subprocess,json
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';S='/home/tyler/airk-builds/arbiter-final-linux-r1/source'
rows=[]
for argv in [['git','-C',S,'diff','--','.gitattributes'],['git','-C',S,'check-attr','-a','--','.gitattributes'],['git','-C',S,'config','--local','--list'],['git','-C',S,'hash-object','--path=.gitattributes',S+'/.gitattributes'],['git','-c','core.autocrlf=true','-C',S,'hash-object','--path=.gitattributes',S+'/.gitattributes']]:
 p=subprocess.run(argv,capture_output=True,text=True);rows.append({'argv':argv,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})
print(json.dumps(rows))
