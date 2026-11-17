# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,subprocess,os,time,traceback,sys
out=pathlib.Path(r'D:/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/ui-native-r3-cards');root=pathlib.Path.cwd();cardpath=out/'cards.json';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(cardpath)=='8bd6997a5a80bca2ca427b89530de0b81e8339a9aefd2871f78afd3ab62029a4'
card=json.loads(cardpath.read_bytes());assert root.samefile(card['candidate'])
assert not (out/'group-execution.json').exists()
for n,h in card['private_inputs'].items():assert sha(out/n)==h,n
for d in ['workbench-r3','admin-r4']:assert not (out/d/'execution.json').exists()
env=os.environ.copy();env.update(card['environment']);(out/'group-temp').mkdir();env['TEMP']=env['TMP']=str(out/'group-temp')
result={'schema':'airk.private.native-ui-r3-group/1','card_sha256':sha(cardpath),'argv':{},'cwd':str(root),'steps':[],'passed':False};start=time.monotonic()
try:
 for name in ['workbench','admin']:
  argv=card[name]['argv'];result['argv'][name]=argv;step={'name':name,'argv':argv,'passed':False};result['steps'].append(step);beg=time.monotonic()
  with (out/(name+'.log')).open('x',encoding='utf-8') as log:child=subprocess.run(argv,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
  step.update(exit_code=child.returncode,elapsed_seconds=time.monotonic()-beg,passed=child.returncode==0)
  print(json.dumps(step),flush=True)
  assert child.returncode==0,'Stop first failure: '+name+'; retained diagnostics '+str(out/(name+'.log'))
 result['passed']=True
except BaseException as error:
 result['failure']={'type':type(error).__name__,'message':str(error)};traceback.print_exc();raise
finally:
 result['elapsed_seconds']=time.monotonic()-start;(out/'group-execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
