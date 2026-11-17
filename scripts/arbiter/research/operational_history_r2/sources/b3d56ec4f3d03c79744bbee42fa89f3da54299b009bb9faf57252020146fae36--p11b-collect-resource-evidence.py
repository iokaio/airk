# SPDX-License-Identifier: Apache-2.0
import hashlib,json,xml.etree.ElementTree as ET
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');work=base/'p10-worktree';output=base/'p11b-resource-observed-r1'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
receipt=json.loads((base/'p11b-resource-tests-r1.receipt.json').read_text())
source={'sha256':'8c4fc38e85435cb5df60f3dfa618e348706b02465989518e953ed1b76ec0c773','files':954}
assert receipt['exit_code']==receipt['actual_exit_code']==0 and receipt['source_before']==receipt['source_after']==source
assert sha(base/'p11b-resource-tests-r1.log')==receipt['log_sha256']
junit=(work/'target/nextest/ci/junit.xml').read_bytes();cases=ET.fromstring(junit).findall('.//testcase')
required=['actual_helper_retention_migration_rebinds_evidence_without_new_embedding_inputs',
          'explicit_native_retention_migration_keeps_old_publication_then_reuses_only_equal_effective_files']
assert len(cases)==2 and all(c.find('failure') is None and c.find('error') is None and c.find('skipped') is None for c in cases)
assert {c.get('name').rsplit('::',1)[-1] for c in cases}==set(required)
with (output/'junit.xml').open('xb') as f:f.write(junit)
kernel=json.loads((output/'kernel-retention.json').read_text());helper=json.loads((output/'helper-retention.json').read_text())
assert [v['phase'] for v in kernel['observations']]==['owned_before_cold','legacy_cold','lean_migration','rich_quota_refused','lean_noop','lean_one_rule_override','lean_one_source_edit']
assert [v['phase'] for v in helper['observations']]==['baseline_model_build','lean','rich','rich_noop']
kernel_rows=[{'phase':v['phase'],'wall_us':v['wall_us'],'logical_used_bytes':v['quota']['used_bytes'],
    'reserved_bytes':v['quota']['reserved_bytes'],'indexed':(v['index_report'] or {}).get('indexed'),
    'reused':(v['index_report'] or {}).get('reused')} for v in kernel['observations']]
keys=['embedding_inputs','embedding_cache_misses','vector_reused_inputs','embedding_model_load_us','embedding_inference_us']
helper_rows=[]
for value in helper['observations']:
    counters=value['native_index_operation_cumulative']['index_work']['metrics']
    assert all(counters[k]['samples']>0 for k in keys)
    helper_rows.append({'phase':value['phase'],'wall_us':value['wall_us'],
        'logical_used_bytes':value['quota']['used_bytes'],'reserved_bytes':value['quota']['reserved_bytes'],
        'graph_records':value['graph_records'],'vector_record_bytes':value['retained_vector_record_bytes'],
        'cumulative_metrics':{k:counters[k] for k in keys},
        'native_operation_resources':value['native_index_operation_cumulative']['resources']})
original=helper['observations'][0]['model_and_graph']
assert all(v['model_and_graph']['model']==original['model'] and v['model_and_graph']['vectors']==original['vectors'] for v in helper['observations'])
assert [int(v['cumulative_metrics']['embedding_inputs']['value']) for v in helper_rows]==[4,4,4,4]
assert [int(v['cumulative_metrics']['embedding_cache_misses']['value']) for v in helper_rows]==[4,4,4,4]
assert [int(v['cumulative_metrics']['vector_reused_inputs']['value']) for v in helper_rows]==[0,4,8,8]
record={'status':'passed','source':source,'tests':2,'names':required,'junit_sha256':sha(output/'junit.xml'),
    'resource_tests_receipt':str(base/'p11b-resource-tests-r1.receipt.json'),'kernel':kernel_rows,'helper':helper_rows,
    'files':{p.name:sha(p) for p in [output/'kernel-retention.json',output/'helper-retention.json',output/'junit.xml']},
    'limits':[kernel['limits'],helper['limits'],
        'One tiny fixture on a development build, without a matched performance baseline. No speedup or physical/peak memory claim.',
        'Reported model-load timing increases across migrations; this is not zero model-load work. Tiny wordvector helper does not establish default ONNX forward behavior.']}
with (output/'verification.json').open('x',encoding='utf-8') as f:json.dump(record,f,indent=2);f.write('\n')
print(json.dumps({'status':record['status'],'source':source,'tests':2,'kernel_phases':len(kernel_rows),'helper_phases':len(helper_rows)}))
