# SPDX-License-Identifier: Apache-2.0
"""Match actual Nextest UUID/counts and independently listed required tests."""
import hashlib
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET

selections_path, group, target, out, receipt_name = sys.argv[1:]
selection = json.loads(Path(selections_path).read_text())[group]
target, out = Path(target), Path(out)
text = (out / (receipt_name + '.log')).read_text()
run = re.search(r'Nextest run ID ([0-9a-f-]+)', text).group(1)
selected = int(re.search(r'Starting (\d+) tests', text).group(1))
assert selected > 0
if 'planned_selected_exact' in selection:
    assert selected == selection['planned_selected_exact'], selected
else:
    assert selected >= selection['planned_selected_minimum'], selected
required = {name for names in selection['required'].values() for name in names}
root = Path.cwd().resolve()
for path in (target / 'nextest/ci/junit.xml', root / 'target/nextest/ci/junit.xml'):
    if not path.is_file() or path.is_symlink():
        continue
    raw = path.read_bytes()
    assert len(raw) <= 8 * 1024 * 1024
    xml = ET.fromstring(raw)
    if xml.attrib.get('uuid') != run:
        continue
    assert int(xml.attrib['tests']) == selected
    assert int(xml.attrib['failures']) == int(xml.attrib['errors']) == 0
    cases = list(xml.iter('testcase'))
    assert len(cases) == selected and not any(list(c) for c in cases if any(x.tag in ('failure', 'error', 'skipped') for x in c))
    names = {case.attrib['name'] for case in cases}
    assert required <= names, sorted(required - names)
    (out / (receipt_name + '-matching-junit.xml')).write_bytes(raw)
    evidence = {'uuid': run, 'selected': selected, 'required_count': len(required),
                'required_names': sorted(required), 'all_passed': True,
                'junit_sha256': hashlib.sha256(raw).hexdigest(),
                'scope': selection['scope'], 'full_platform_or_model_qualification': False}
    (out / (receipt_name + '-exact-regressions.json')).write_text(json.dumps(evidence, indent=2) + '\n')
    print(json.dumps({k: v for k, v in evidence.items() if k != 'required_names'}))
    break
else:
    raise ValueError('No actual matching Nextest UUID JUnit; no pass claim')
