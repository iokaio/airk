# SPDX-License-Identifier: Apache-2.0
import base64,hashlib,json,re,subprocess,xml.etree.ElementTree as ET
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import base64,hashlib,json,re,xml.etree.ElementTree as ET
from pathlib import Path
out=Path('artifacts/advanced-split/receipts/mac-platform-scoped');log=(out/'mac-platform-tests-attempt5.log').read_text();run=re.search(r'Nextest run ID ([0-9a-f-]+)',log).group(1);selected=int(re.search(r'Starting (\\d+) tests',log).group(1));path=Path('target/nextest/ci/junit.xml');data=path.read_bytes();assert len(data)<2*1024*1024;root=ET.fromstring(data)
assert root.attrib['uuid']==run and int(root.attrib['tests'])==selected and int(root.attrib['failures'])==0 and int(root.attrib['errors'])==0
retained=out/'mac-platform-tests-attempt5-observed-junit.xml'
with retained.open('xb') as output:output.write(data)
print(json.dumps({'source_path':str(path),'retained_path':str(retained),'uuid':run,'selected':selected,'sha256':hashlib.sha256(data).hexdigest(),'xml':base64.b64encode(data).decode()}))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=35)
if r.returncode:raise SystemExit(r.stderr.decode())
v=json.loads(r.stdout);content=base64.b64decode(v.pop('xml'));out=Path('artifacts/advanced-split/receipts/mac-platform-scoped-attempt5');(out/'mac-platform-tests-attempt5-observed-junit.xml').write_bytes(content);root=ET.fromstring(content);v['summary']=root.attrib;v['suites']=[suite.attrib for suite in root.findall('testsuite')];(out/'junit-origin.json').write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8');print(json.dumps({'sha256':v['sha256'],'uuid':v['uuid'],'selected':v['selected'],'summary':root.attrib,'suites':[{k:s.attrib.get(k) for k in ['name','tests','failures','errors','skipped']} for s in root.findall('testsuite')]},indent=2))
