# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import re
p=Path('artifacts/optimizations/f06-draft/rebased-f05/source/crates/airk-cli/src')
for name in ['project_client.rs','unified_mcp.rs']:
 path=p/name;s=path.read_text(); blocks=[]
 pattern=r'<<<<<<< [^\n]*\n(.*?)\|\|\|\|\|\|\| [^\n]*\n(.*?)=======\n(.*?)>>>>>>> [^\n]*\n'
 def resolve(m):
  blocks.append({'live':m[1],'base':m[2],'draft':m[3]})
  return m[3]
 s=re.sub(pattern,resolve,s,flags=re.S)
 path.write_text(s,newline='\n')
 import json
 (p.parent.parent.parent/'CONFLICTS-'+Path(name).stem if False else p/(name+'.conflicts.json')).write_text(json.dumps(blocks,indent=2))
 print(name,len(blocks))
