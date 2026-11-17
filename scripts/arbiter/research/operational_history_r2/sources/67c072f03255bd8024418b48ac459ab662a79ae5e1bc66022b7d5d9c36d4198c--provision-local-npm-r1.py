# SPDX-License-Identifier: Apache-2.0
import hashlib,os,subprocess,sys
from pathlib import Path
r=Path.cwd();node=r/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
assert hashlib.sha256(node.read_bytes()).hexdigest()=='3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5'
cache=r/'artifacts/advanced-split/npm-cache';cache.mkdir(exist_ok=True)
config=cache/'empty-user.npmrc'
if not config.exists():config.write_bytes(b'')
assert config.read_bytes()==b''
env=dict(os.environ);env.update(NPM_CONFIG_CACHE=str(cache),NPM_CONFIG_LOGS_DIR=str(cache/'_logs'),NPM_CONFIG_UPDATE_NOTIFIER='false',NPM_CONFIG_USERCONFIG=str(config))
for package in ['vscode','admin']:
 target=r/'packages'/package/'node_modules'
 assert target.resolve().is_relative_to(r) and not target.is_symlink() and not target.is_junction()
 command=[sys.executable,'scripts/dev/receipt_runner.py','--groups','frontend','--out',str(r/'artifacts/advanced-split/receipts/npm-provision-r1'),'--node',str(node),'npm-ci-'+package,str(node),str(node.parent/'node_modules/npm/bin/npm-cli.js'),'ci','--ignore-scripts','--no-audit','--no-fund','--prefix','packages/'+package]
 result=subprocess.run(command,env=env)
 if result.returncode:raise SystemExit(result.returncode)
