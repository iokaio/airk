# SPDX-License-Identifier: Apache-2.0
import subprocess,sys,json
from pathlib import Path
root=Path.cwd();base=root/'artifacts/v190';out=base/'p9-fixture-qualification-r1.json';assert not out.exists()
steps=[('minimal',['cargo','test','-p','airk-kernel','--no-default-features','--lib','--locked','shared_calibration','--','--test-threads=1']),('helper',['cargo','test','-p','airk-kernel','--all-features','--lib','--locked','shared_calibration','--','--test-threads=1']),('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--','-D','warnings']),('fmt',['cargo','fmt','--all','--check'])]
record={'status':'running','scope':'Canonical Mac temporary fixture only; runtime production code unchanged from42224990.','steps':[]}
for name,command in steps:
 log='artifacts/v190/p9-fixture-'+name+'-r1.log';p=subprocess.run([sys.executable,str(base/'p9-local-command.py'),log,*command],cwd=root)
 record['steps'].append({'name':name,'exit_code':p.returncode,'log':log});out.write_text(json.dumps(record,indent=2)+'\n')
 if p.returncode:record['status']='failed';out.write_text(json.dumps(record,indent=2)+'\n');sys.exit(p.returncode)
record['status']='passed';out.write_text(json.dumps(record,indent=2)+'\n')
