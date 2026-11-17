# SPDX-License-Identifier: Apache-2.0
import hashlib, json, re
from pathlib import Path

base = Path(__file__).resolve().parent
root = Path.cwd()
ledger = json.loads((base / 'ledger.json').read_text(encoding='utf-8'))
checks = {x['id']: x for x in ledger['checks']}
assert len([k for k in checks if k.isdigit()]) == 78
hashes = {}
def walk(value):
    if isinstance(value, dict):
        if {'path', 'content_hash'} <= value.keys():
            hashes.setdefault(value['path'], set()).add(value['content_hash'])
        for v in value.values(): walk(v)
    elif isinstance(value, list):
        for v in value: walk(v)
for check in checks.values(): walk(check['data'])
for path, observed in hashes.items():
    actual = hashlib.sha256((root / path).read_bytes()).hexdigest()
    assert observed == {actual}, (path, observed, actual)
for i in ['02', '03', '67', '71', '75']:
    d = checks[i]['data']
    assert d['version'] == '1.8.7' and d['query_concurrency'] == 16
    assert d['health'] == 'ok' and d['index_state'] == 'available'
assert checks['04']['data']['query_concurrency'] == 16
assert checks['04']['data']['runtime']['versions_match']
for i in ['20', '21']: assert checks[i]['data']['hits'] == []
assert checks['29']['data']['items'] == []
for i in ['15','17','18','19','22','24']:
    d=checks[i]; assert d['data']['hits']
    for hit in d['data']['hits']:
        p=hit['citation']['path']; a=d['args']
        assert p.startswith(a.get('path_prefix') or '')
        assert not a.get('exclude_path_prefix') or not p.startswith(a['exclude_path_prefix'])
for i in ['43','45','46']: assert checks[i]['data']['read']['status'] == 'not_modified'
for i in ['36','44','48','70','74','78']:
    hit=checks[i]['data']['hits'][0]; c=hit['citation']
    raw=(root/c['path']).read_bytes().decode('utf-8')
    lines=raw.splitlines(keepends=True)
    assert hit['text'] == ''.join(lines[c['start_line']-1:c['end_line']])
for initial, follow in [('25','37'),('28','38'),('30','39'),('31','40'),('32','41'),('33','42')]:
    a,b=checks[initial]['data'],checks[follow]['data']
    assert a['cursor'] and checks[follow]['args']['cursor'] == a['cursor']
    key='paths' if 'paths' in a else 'items'
    assert a[key] and b[key]
    aa={json.dumps(x,sort_keys=True) for x in a[key]}
    bb={json.dumps(x,sort_keys=True) for x in b[key]}
    assert not aa & bb, (initial,follow)
path='crates/airk-cli/tests/mcp.rs'
oracle=[n for n,line in enumerate((root/path).read_text(encoding='utf-8').splitlines(),1) if re.search(r'\bSession\b',line)]
for i in ['73','77','69-retry']:
    d=checks[i]['data']; hits=d['evidence']['hits']
    assert [h['citation']['start_line'] for h in hits] == oracle
    assert not d['page']['more']
assert len(checks['51']['data']['items']) == 4
assert checks['52']['data']['status'] == 'prepared'
assert checks['53']['data']['executed'] and checks['53']['data']['steps'][0]['error'] is None
assert checks['53']['data']['truncated'] and checks['53']['data']['completeness']
assert checks['55']['data']['valid'] and not checks['55']['data']['admitted']
for i in ['56','57','58','59','60']:
    d=checks[i]['data']; assert d['status']=='pending_review' and not d['executed'] and not d['applied']
assert checks['65']['data']['valid'] is False and checks['65']['data']['code']=='INVALID_SCHEMA'
for i in ['61','62','63','64','66']: assert checks[i]['isError']
assert checks['69']['data']['code']=='publication_busy'
assert checks['69']['data']['action']=='repeat_same_request'
assert checks['69-retry']['pass']
summary={'checks':78,'functional_checks_passed_after_corrections_and_retry':78,'first_valid_attempt_passes':77,'transient_retry_required':1,'initial_parse_harness_input_errors':2,'live_file_hashes_verified':len(hashes),'reference_lines_verified_per_complete_response':len(oracle),'status_errors_in_three_mixed_batches':0,'limits':['Single warm Windows MCP connection; no cold-start or other-platform qualification','Pagination checked on selected adjacent pages; not a full corpus scan','Empty Session struct relationships retain the known incomplete navigation limitation','Draft submissions were left pending in Explorer; no UI application tested','Invalid handoff token refusal tested; successful user-shared handoff not tested']}
(base/'validation.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
print(json.dumps(summary,indent=2))
