# SPDX-License-Identifier: Apache-2.0
"""Aggregate retained native readiness counts only; no model or grading."""
from pathlib import Path
from collections import Counter
import hashlib
import json
HERE=Path(__file__).resolve().parent
pins={'visible-prefix-mining-r1-u8':'9aec2ccc46b1d278db426da62183078702496879dd826442f5b3fb8f999caeb9','visible-prefix-mining-r1-s8':'2fa9ee761e3c9bf9fc4f0d560d5b3635ac5763b2a17d4274e6c7b64cf3a1af51'}
families=[]
for name,pin in pins.items():
    raw=(HERE/name/'capture.json').read_bytes()
    if hashlib.sha256(raw).hexdigest()!=pin:raise ValueError('retained native capture changed')
    records=json.loads(raw)['records'];ready=[r['primary_vector']['semantic_readiness'] for r in records];execution=[r['primary_vector']['execution'] for r in records]
    families.append({'family':name,'records':len(records),'states':dict(Counter(r['state'] for r in ready)),'artifact_count_pairs':sorted({(r['selected_artifacts'],r['completed_artifacts']) for r in ready}),'unit_count_pairs':sorted({(r['selected_units'],r['completed_units']) for r in ready}),'modes':dict(Counter(r['served_mode'] for r in execution)),'incomplete':sum(r['incomplete'] for r in execution)})
result={'families':families,'capture_sha256':pins,'no_new_NN_or_grade':True,'ANN_recall_proven':False}
out=HERE/'native-readiness-count-addendum.json'
if out.exists():raise ValueError('fresh metadata artifact required')
out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
