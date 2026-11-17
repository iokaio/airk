# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
root=Path(__file__).resolve().parents[2]
old=root/'artifacts/v190/pr60-root-command-r1.py'
new=Path(__file__).resolve().parent/'command-r1.py'
s=old.read_text(encoding='utf-8')
s=s.replace("log=base/'artifacts/v190'/sys.argv[1]", "log=base/'artifacts/branch-review-r1/windows-r1'/sys.argv[1]")
s=s.replace("env['PATH']='C:/Users/tdotn/.cargo/bin;'+env['PATH']", """tool_paths=[base/'artifacts/tools/node-v24.15.0-win-x64',base/'artifacts/tools/nextest',Path('C:/Users/tdotn/.cargo/bin')]
node_pin=json.loads((base/'artifacts/tools/node-v24.15.0-win-x64.receipt.json').read_text(encoding='utf-8'))
assert hashlib.sha256((tool_paths[0]/'node.exe').read_bytes()).hexdigest()==node_pin['node_sha256']
env['PATH']=';'.join(map(str,tool_paths))+';'+env['PATH']""")
s=s.replace("'command':sys.argv[2:]", "'tool_paths':list(map(str,tool_paths)),'node_sha256':node_pin['node_sha256'],'nextest_sha256':hashlib.sha256((tool_paths[1]/'cargo-nextest.exe').read_bytes()).hexdigest(),'command':sys.argv[2:]")
assert s!=old.read_text(encoding='utf-8')
with new.open('x',encoding='utf-8') as f:f.write(s)
(new.parent/'windows-r1').mkdir(exist_ok=False)
print(new)
