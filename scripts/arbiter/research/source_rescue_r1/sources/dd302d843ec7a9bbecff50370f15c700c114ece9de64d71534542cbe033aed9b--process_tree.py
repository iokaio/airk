# SPDX-License-Identifier: Apache-2.0
"""Owned process-tree wrapper: never exempts descendants or extends cleanup."""
import json,subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent
POWERSHELL=Path('C:/Program Files/PowerShell/7/pwsh.exe')
def command_request(argv,cwd,out,stage,events=None):
 if not argv or not Path(argv[0]).is_absolute():raise ValueError('absolute original command executable required')
 request={'schema':'airk.private.process-tree-command/1','executable':str(argv[0]),'arguments':subprocess.list2cmdline([str(x) for x in argv[1:]]),'cwd':str(cwd),'stdout':str(out/'logs'/(stage+'-tree.stdout.log')),'stderr':str(out/'logs'/(stage+'-tree.stderr.log')),'result':str(out/(stage+'-tree-result.json')),'started':str(out/(stage+'-tree-started.json')),'started_event':None,'done_event':None}
 if events:request.update(started_event=events['started'],done_event=events['done'])
 return request
def run_tree(o,argv,cwd,out,stage,timeout,environment,events=None):
 out=Path(out);(out/'logs').mkdir(parents=True,exist_ok=True)
 request=command_request(argv,cwd,out,stage,events);path=out/(stage+'-tree-request.json')
 with path.open('xb') as f:f.write((json.dumps(request,indent=2)+'\n').encode())
 wrapped=[str(POWERSHELL),'-NoLogo','-NoProfile','-NonInteractive','-File',str(HERE/'wait_process_tree.ps1'),'-Request',str(path)]
 result=o.run_owned(wrapped,cwd,out/'logs'/(stage+'.log'),timeout,environment)
 result['tree_request']=str(path)
 # Canonical Job ownership and its existing5s cleanup remain authoritative.
 if o.successful(result):
  tree=o.read_json(request['result'])
  if tree.get('schema')!='airk.private.process-tree-result/1' or tree.get('exit_code')!=0 or tree.get('wait')!='Start-Process-Wait-process-and-descendants':raise ValueError('actual complete tree result missing')
  result['actual_tree_result']=tree
 return result
