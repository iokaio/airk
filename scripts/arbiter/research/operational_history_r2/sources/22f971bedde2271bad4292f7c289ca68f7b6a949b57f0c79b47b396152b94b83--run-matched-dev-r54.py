# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess,sys,hashlib
here=Path('artifacts/advanced-split/models/corrected-graph-matched-dev-r54-sid1012-r1');packet=here/'packet.json';expected='9a9ca8025414f11af51bb49b979824978575bd64daf39e28d0a93a6d30daf5f4';assert hashlib.sha256(packet.read_bytes()).hexdigest()==expected
for stage in json.loads(packet.read_bytes())['stages']:
 cmd=[sys.executable,str(here/'run_stage.py'),'--packet-sha256',expected,'--runner-sha256','1103e0c00c71262cebcfe149e1d95743880c137770bf83c65162b0c6e364e80f','--family',stage['family'],'--execute-reviewed']
 result=subprocess.run(cmd)
 if result.returncode:raise SystemExit(result.returncode)
