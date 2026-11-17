# SPDX-License-Identifier: Apache-2.0
"""Observe exact native emissions/copy receipts. No model, protocol construction or defaults."""
import hashlib
import json
import os
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = Path('D:/code/Github/airk')
REFERENCE = REPO / 'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1'

def read(path):
    return json.loads(Path(path).read_bytes())

def pin(path):
    path = Path(path).resolve(strict=True)
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}

def audit():
    native_report_path = HERE.parent / 'metadata/metadata-regeneration-report.private.json'
    report = read(native_report_path)
    assert report['complete'] is True and report['NN_invocations'] == 0 and report['error'] is None
    assert report['native_source_group'] == '5f788569885d7b33b476f3c0381b55528f3981a000fe960d884b0128a997075c'
    assert len(report['profiles']) == 3
    catalog = read(REPO / 'crates/airk-retrieval/src/arbiter/release-catalog.json')
    originals = {row['model']: row for row in catalog['profiles']}
    copy_receipt = read(HERE / 'flat-copy-summary.json')
    assert copy_receipt['complete'] and copy_receipt['copied_files'] == 23
    assert copy_receipt['expected_bytes'] == copy_receipt['copied_bytes'] == 24130011811
    copy_rows = [json.loads(line) for line in (HERE / 'flat-copy-census.jsonl').read_text().splitlines()]
    assert len(copy_rows) == 23
    for copied in copy_rows:
        observed = Path(copied['destination']).stat()
        assert observed.st_nlink == 1 and observed.st_size == copied['bytes']
        assert not getattr(observed, 'st_file_attributes', 0) & 0x400
    emissions = read(HERE / 'native-input-emissions.json')
    assert emissions['complete'] and len(emissions['invocations']) == 6
    assert all(row['failure'] is None and row['exit_code'] == 0 for row in emissions['invocations'])
    models, policies = [], []
    unchanged_assets, changed_assets = 0, []
    for row in report['profiles']:
        profile = read(row['profile']['path'])
        old = originals[profile['model']]
        focus = profile['model'] == 'arbiter-focus-qwen3-reranker-06b'
        assert profile['manifest'] == row['manifest']
        assert profile['qualification'] is None and not profile.get('downloads') and not profile.get('qualified_targets')
        assert profile['max_tokens'] == (512 if focus else 8192)
        if focus:
            for field in ['compiler', 'calibration', 'policy', 'task_policies']:
                assert profile[field] == old[field]
        else:
            assert profile['compiler'] != old['compiler'] and profile['task_policies'] != old['task_policies']
        before = {asset['name']: asset for asset in old['assets']}
        for asset in profile['assets']:
            if before.get(asset['name']) == asset:
                unchanged_assets += 1
            else:
                assert not focus and asset['role'] in ['compiler', 'calibration']
                changed_assets.append({'model': profile['model'], 'asset': asset})
        request = read(row['resource_run_input']['path'])
        policies.append(request['policy'])
        assert request['policy']['shared_input_tokens'] == request['policy']['context_input_tokens'] == 8192
        assert request['policy']['focus_pair_input_tokens'] == 512 and request['policy']['deadline_ms'] == 120000
        assert request['policy']['required_warm_samples'] == 20 and request['policy']['p95_limit_us'] == 120000000
        base = HERE / row['model']
        template, roster = read(base / 'template.json'), read(base / 'original-roster.json')
        assert template['model'] == roster['model'] == profile['model']
        assert template['compiler'] == roster['compiler'] == profile['compiler']
        assert template['resource_policy'] == roster['resource_policy_digest'] == request['policy_digest']
        assert roster['native_input_sha256'] == row['resource_run_input']['sha256']
        assert roster['native_airk_nn_invocations'] == 0 and roster['qualification'] is False
        assert template['reference_producer'] is None and template['original_reference_assets'] == []
        rows = []
        for case in template['rows']:
            values = case['input']
            assert case['reference_same_artifact'] is None and case['reference_original_fp32'] is None
            assert len(values['input_ids']) == len(values['attention_mask']) == len(values['position_ids'])
            assert all(value == 1 for value in values['option_mask'])
            if not focus:
                assert all(value == 1 for value in values['attention_mask'])
            rows.append({'name': case['name'], 'tokens': len(values['input_ids']), 'candidates': len(values['candidate_positions'])})
        assert [item['tokens'] for item in rows] == ([512] * 12 if focus else [17, 128, 8192])
        if not focus:
            assert [item['candidates'] for item in rows] == [3, 8, 8]
        models.append({'model': row['model'], 'manifest': profile['manifest'], 'max_tokens': profile['max_tokens'],
            'profile': row['profile'], 'resource_run_input': row['resource_run_input'], 'template': pin(base / 'template.json'),
            'original_roster': pin(base / 'original-roster.json'), 'native_source_files': len(roster['original_source_files']),
            'native_original_assets': len(roster['original_reference_assets']), 'publisher_revision': roster['publisher_revision'], 'rows': rows})
    assert unchanged_assets == 19 and len(changed_assets) == 4 and all(policies[0] == policy for policy in policies)
    preparation = read(HERE / 'inert-plan-preparation.json')
    assert preparation['complete'] and len(preparation['results']) == 6 and not preparation['failures']
    plans = []
    for result in preparation['results']:
        assert result['failure'] is None and result['exit_code'] == 0
        observed_pin = pin(result['plan'])
        assert observed_pin['sha256'] == result['plan_sha256']
        plan = read(result['plan'])
        assert plan['human_sid'] == 'S-1-5-21-3678370870-3858642417-826968105-1002'
        assert plan['expected_gpu_name'] == 'NVIDIA RTX A6000'
        assert plan['input_seals'][plan['worker']['path']] == plan['worker']['sha256']
        assert plan['input_seals'][plan['ownership_module']['path']] == plan['ownership_module']['sha256']
        plans.append({'tier': result['tier'], 'kind': result['kind'], 'plan': observed_pin, 'limits': plan['limits'],
            'input_seals': len(plan['input_seals']), 'builder_preflight_seconds': result['elapsed_seconds'],
            'native_input': plan['native_input'], 'native_template': plan['native_template'], 'original_roster': plan['original_roster']})
    output = {'schema': 'airk.private.actual-nn0-preparation/1', 'complete': True,
        'metadata_report': pin(native_report_path), 'metadata_execution': pin(HERE / 'metadata-execution.json'),
        'native_build_receipt': report['native_build_receipt'], 'native_source_group': report['native_source_group'],
        'models': models, 'policy': policies[0], 'flat_copy_receipt': pin(HERE / 'flat-copy-summary.json'),
        'flat_copy_census': pin(HERE / 'flat-copy-census.jsonl'), 'ordinary_copied_files': 23, 'ordinary_copied_bytes': 24130011811,
        'copied_destination_link_count': 1, 'unchanged_original_assets': unchanged_assets, 'new_context_metadata': changed_assets,
        'native_emissions': pin(HERE / 'native-input-emissions.json'), 'inert_plan_preparation': pin(HERE / 'inert-plan-preparation.json'),
        'reference_review': pin(REFERENCE / 'primary-review-r3.json'), 'plans': plans,
        'execution_user': 'TYAI\\tdotn', 'execution_sid': 'S-1-5-21-3678370870-3858642417-826968105-1002',
        'selected_model_forward_invocations': 0, 'qualification_granted': False,
        'pending': ['Root review of exact concrete native outputs and inert plans before selected-model execution.',
                    'Actual independently owned reference executions and arrays; fresh native numerical/resource qualification.',
                    'Separate human semantic labels remain unavailable for dependent training/promotion.']}
    destination = HERE / 'nn0-preparation-summary.json'
    with destination.open('x', encoding='utf-8') as stream:
        json.dump(output, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'summary': pin(destination), 'models': len(models), 'inert_plans': len(plans), 'selected_model_forward_invocations': 0}))

if __name__ == '__main__':
    audit()