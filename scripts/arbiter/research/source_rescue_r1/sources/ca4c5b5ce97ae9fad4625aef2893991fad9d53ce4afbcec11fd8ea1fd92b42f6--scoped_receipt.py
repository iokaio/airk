# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import importlib.util,sys,hashlib,json,subprocess,time,datetime
root=Path.cwd(); tag=sys.argv[1]; scopes=sys.argv[2].split(','); args=sys.argv[3:]
def snap():
 p=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard','--',*scopes]);return {s.decode():hashlib.sha256((root/s.decode()).read_bytes()).hexdigest() for s in p.split(b'\0') if s and (root/s.decode()).is_file()}
before=snap();start=time.monotonic();cmd=[sys.executable,'-B','scripts/dev/receipt_runner.py','--groups','',*args];r=subprocess.run(cmd);after=snap();out=root/'artifacts/v190/final-plan/exec-20261116T052942Z';(out/(tag+'-source.json')).write_text(json.dumps({'command':cmd,'elapsed_seconds':time.monotonic()-start,'before':before,'after':after,'source_unchanged':before==after,'exit_code':r.returncode},indent=2)+'\n',encoding='utf-8');sys.exit(r.returncode or (0 if before==after else 3))
