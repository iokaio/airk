# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import re,json
p=Path('artifacts/optimizations/f02-workspace.log')
text=p.read_text(encoding='utf-8',errors='replace')
sections=[];current=None
for line in text.splitlines():
    if re.match(r'\s*Running ',line) or re.match(r'\s*Doc-tests ',line):
        current={'target':line.strip(),'results':[]};sections.append(current)
    m=re.search(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',line)
    if m:
        if current is None:current={'target':'unassigned','results':[]};sections.append(current)
        current['results'].append(dict(status=m[1],passed=int(m[2]),failed=int(m[3]),ignored=int(m[4]),measured=int(m[5]),filtered=int(m[6])))
reported=[r for s in sections for r in s['results']]
outer=[s['results'][-1] for s in sections if s['results']]
extra=[r for s in sections for r in s['results'][:-1]]
def counts(rows):return {key:sum(r[key] for r in rows) for key in ['passed','failed','ignored','measured','filtered']}
result={'complete_sections':sum(bool(s['results']) for s in sections),'all_reported_summaries':counts(reported),'last_summary_per_cargo_target':counts(outer),'additional_summaries_within_target':counts(extra),'sections':sections}
Path('artifacts/optimizations/f02-workspace-counts.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='sections'}))
