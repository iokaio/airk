# SPDX-License-Identifier: Apache-2.0
import json, shutil, subprocess, sys, xml.etree.ElementTree as ET
from pathlib import Path
base=Path(__file__).resolve().parent; root=base.parents[1]; out=base/'windows-r1'
selection=json.loads((out/'native-selection-r1.json').read_text(encoding='utf-8'))
command=['cargo','nextest','run','--workspace','--lib','--all-features','--locked','--offline','--timings','--profile','ci','--ignore-default-filter','--no-fail-fast','--retries','0','--test-threads','4','-E',selection['selection']]
print(json.dumps(command),flush=True)
code=subprocess.run(command,cwd=root).returncode
report=root/'target/nextest/ci/junit.xml'; saved=out/'focused-r2.junit.xml'
assert not saved.exists()
if report.exists():shutil.copyfile(report,saved)
if code:sys.exit(code)
cases=list(ET.parse(saved).iter('testcase'))
assert len(cases)==len(selection['selected']),(len(cases),len(selection['selected']))
assert all(case.find('failure') is None and case.find('error') is None and case.find('skipped') is None for case in cases)
print(json.dumps({'status':'passed','tests':len(cases),'junit':str(saved)}))
