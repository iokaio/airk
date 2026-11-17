# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,xml.etree.ElementTree as ET
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
r=subprocess.run(ssh+['sh','-c',"'cd \"$HOME/code/airk\" && cat artifacts/advanced-split/receipts/mac-platform-scoped/mac-platform-tests-attempt4-observed-junit.xml'"],capture_output=True,timeout=30)
if r.returncode:raise SystemExit(r.stderr.decode())
p=Path('artifacts/advanced-split/receipts/mac-platform-scoped-attempt4/mac-platform-tests-attempt4-observed-junit.xml');p.write_bytes(r.stdout);root=ET.fromstring(r.stdout);print(json.dumps({'sha256':hashlib.sha256(r.stdout).hexdigest(),'root':root.tag,'attributes':root.attrib,'testcases':len(root.findall('.//testcase')),'failures':len(root.findall('.//failure'))},indent=2))
