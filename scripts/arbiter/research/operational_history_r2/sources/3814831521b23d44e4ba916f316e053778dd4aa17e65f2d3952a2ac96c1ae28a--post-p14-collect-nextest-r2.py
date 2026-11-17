# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import re
import shutil
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent
receipt = json.loads((base / 'post-p14-final-full-nextest-r2.receipt.json').read_text())
assert receipt['source_unchanged'] and receipt['source_before']['sha256'] == '6de490d57d7053cbd63862366e840cc3657df4ef10e8a57c4ca018e6d19a2633'
log_path = base / 'post-p14-final-full-nextest-r2.log'
assert hashlib.sha256(log_path.read_bytes()).hexdigest() == receipt['log_sha256']
log = log_path.read_text(encoding='utf-8')
summary = next(line for line in log.splitlines() if 'Summary [' in line)
out = base / 'post-p14-final-full-nextest-r2-evidence'
out.mkdir(exist_ok=False)
junit = base / 'p10-worktree/target/nextest/ci/junit.xml'
shutil.copyfile(junit, out / 'junit.xml')
root = ET.parse(out / 'junit.xml').getroot()
failures = [{'suite': suite.get('name'), 'name': case.get('name')}
            for suite in root.iter('testsuite') for case in suite.findall('testcase')
            if case.find('failure') is not None or case.find('error') is not None]
timings = []
for name in re.findall(r'Timing report saved to (.+\.html)', log):
    path = Path(name.strip())
    shutil.copyfile(path, out / path.name)
    timings.append({'path': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
result = {'source': receipt['source_before'], 'summary': summary,
          'junit_sha256': hashlib.sha256((out / 'junit.xml').read_bytes()).hexdigest(),
          'failures': failures, 'timings': timings}
with (out / 'collection.json').open('x', encoding='utf-8') as stream:
    json.dump(result, stream, indent=2)
print(json.dumps(result))
