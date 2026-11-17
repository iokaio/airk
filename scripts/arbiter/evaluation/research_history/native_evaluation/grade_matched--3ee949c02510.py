# SPDX-License-Identifier: Apache-2.0
"""Private six-role DEV comparison; unchanged evidence grader, no FINAL or native execution."""
from pathlib import Path
from fractions import Fraction
import argparse
import hashlib
import json
import os
import threading
import time
import types

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
MODELS = HERE.parent
EXPECTED = {
    'candidate-u8': 'visible-prefix-mining-r1-u8',
    'candidate-s8': 'visible-prefix-mining-r1-s8',
    'control-shipped-u8': 'original-shipped-u8-ort130-evaluation',
    'control-mixed-u8': 'original-mixed-u8-ort130-evaluation',
    'diagnostic-shipped-s8': 'original-shipped-s8-ort130-evaluation',
    'diagnostic-mixed-s8': 'original-mixed-s8-ort130-evaluation',
}

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def require(value, message):
    if not value:
        raise ValueError(message)

def seal(path):
    return {'path': str(Path(path).resolve()), 'sha256': sha(path)}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet-sha256', required=True)
    parser.add_argument('--runner-sha256', required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--prepare-capture-manifest', action='store_true')
    parser.add_argument('--manifest-sha256')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    packet_path = HERE/'packet.json'
    require(sha(packet_path) == args.packet_sha256 and sha(__file__) == args.runner_sha256,
            'reviewed packet/grading adapter changed')
    packet = json.loads(packet_path.read_bytes())
    run_path = HERE/'run_stage.py'
    require(sha(run_path) == packet['pins'][str(run_path)], 'reviewed native wrapper changed')
    wrapper = types.ModuleType('_reviewed_matched_dev_stage'); wrapper.__file__ = str(run_path)
    exec(compile(run_path.read_bytes(), str(run_path), 'exec'), wrapper.__dict__)
    for path, expected in packet['pins'].items():
        wrapper.file_pin(path, expected)
    candidates = [(packet['retained_u8']['role'], packet['retained_u8']['family'],
                   Path(packet['retained_u8']['capture']), Path(packet['retained_u8']['execution']),
                   Path(packet['retained_u8']['request']))]
    for stage in packet['stages']:
        execution = HERE/('execution-'+stage['family'])
        candidates.append((stage['role'], stage['family'], Path(stage['environment']['AIRK_NATIVE_DEV64_OUTPUT']),
                           execution/'execution.json', execution/'request.json'))
    require({role: family for role, family, *_ in candidates} == EXPECTED, 'fixed six-role comparison required')
    if args.prepare_capture_manifest:
        require(not args.manifest.exists() and args.manifest.parent.resolve() == HERE,
                'fresh exact private capture manifest required')
        rows = []
        for role, family, capture_path, execution_path, request_path in candidates:
            execution = json.loads(execution_path.read_bytes())
            request = json.loads(request_path.read_bytes())
            require(execution['complete'] is True and wrapper.natural(execution['owned'])
                    and execution['post_input_drift'] == [], 'complete natural unchanged native stage required')
            require(execution['capture_sha256'] == sha(capture_path), 'capture differs from owned execution')
            if role == 'candidate-u8':
                require(sha(capture_path) == packet['retained_u8']['capture_sha256']
                        and sha(execution_path) == packet['retained_u8']['execution_sha256']
                        and sha(request_path) == packet['retained_u8']['request_sha256']
                        and execution['bound_packet_sha256'] == packet['retained_u8']['bound_packet_sha256'],
                        'retained U8 identity changed')
            else:
                require(execution['packet_sha256'] == request['packet_sha256'] == args.packet_sha256
                        and request['runner_sha256'] == packet['pins'][str(run_path)], 'native wrapper/packet differs')
            rows.append({'role': role, 'family': family, 'capture': seal(capture_path),
                         'execution': seal(execution_path), 'request': seal(request_path)})
        manifest = {'schema': 'airk.private-corrected-graph-DEV-capture-manifest/1',
                    'packet_sha256': args.packet_sha256, 'captures': rows, 'FINAL': False}
        args.manifest.write_text(json.dumps(manifest, indent=2)+'\n', encoding='utf-8')
        print(json.dumps({'manifest': str(args.manifest), 'sha256': sha(args.manifest),
                          'native_or_grading_executed': False, 'capture_count': len(rows)}))
        return
    require(args.manifest_sha256 and sha(args.manifest) == args.manifest_sha256,
            'reviewed complete capture manifest required')
    require(all(os.environ.get(k) == '1' for k in ('OMP_NUM_THREADS', 'MKL_NUM_THREADS', 'OPENBLAS_NUM_THREADS')),
            'original singlethread grading environment required')
    manifest = json.loads(args.manifest.read_bytes())
    require(manifest['packet_sha256'] == args.packet_sha256
            and len(manifest['captures']) == 6
            and {row['role']: row['family'] for row in manifest['captures']} == EXPECTED,
            'exact matched six-role capture census required')
    inputs = {Path(p): expected for p, expected in packet['pins'].items()}
    inputs.update({packet_path: args.packet_sha256, Path(__file__).resolve(): args.runner_sha256,
                   args.manifest.resolve(): args.manifest_sha256})
    expected_paths = {role: (family, capture.resolve(), execution.resolve(), request.resolve())
                      for role, family, capture, execution, request in candidates}
    for row in manifest['captures']:
        require((row['family'], *(Path(row[k]['path']).resolve() for k in ('capture', 'execution', 'request')))
                == expected_paths[row['role']], 'foreign capture/evidence path')
        for kind in ('capture', 'execution', 'request'):
            path = Path(row[kind]['path']); wrapper.file_pin(path, row[kind]['sha256'])
            inputs[path] = row[kind]['sha256']
    output = args.output.resolve() if args.output else HERE/'grading-r1'
    require(not output.exists() and output.parent == HERE, 'fresh exact private grading sibling required')
    helper_path = MODELS/'visible-prefix-export-source-r1/grade_candidates.py'
    helper = types.ModuleType('_retained_grade_helpers'); helper.__file__ = str(helper_path)
    exec(compile(helper_path.read_bytes(), str(helper_path), 'exec'), helper.__dict__)
    tools = ROOT/'tools/retrieval-train'
    modules_source = [('_matched_bounded', MODELS/'a1-next-experiment-blind-metadata-r5/audit.py'),
                      ('goal_rubric', tools/'goal_rubric.py'), ('goal_sources', tools/'goal_sources.py'),
                      ('grade_native_retrieval', tools/'grade_native_retrieval.py'),
                      ('development_retrieval', tools/'development_retrieval.py')]
    source_rows = [(name, path, path.read_bytes()) for name, path in modules_source]
    with helper.verified_modules(source_rows) as modules:
        clock = modules['_matched_bounded']; clock.START = time.monotonic(); clock.JOB_STATE = clock.enforce_budget()
        def expire():
            if not clock.done.wait(60): os._exit(124)
        threading.Thread(target=expire, daemon=True).start()
        owner = modules['development_retrieval']
        fixture = MODELS/'a1-component-dev-native-fixture-r1'
        source_manifest = fixture/'manifest.private.json'
        rubric = helper.load(fixture/'rubric.private.json')['tasks']
        mask = helper.load(MODELS/'a1-component-dev-evaluation-protocol-r1/quality-mask.json')
        metadata = helper.load(source_manifest); owner.verify_fixture(fixture, metadata)
        cache = modules['goal_sources'].FrozenSourceMetadata(fixture, inputs[source_manifest])
        results = []
        for row in manifest['captures']:
            native = helper.load(row['capture']['path'])
            require([r['id'] for r in native['records']] == packet['original_question_order'], 'original all64 order differs')
            for key, value in packet['capture_common'].items():
                require(native.get(key) == value, 'matched native/source/query identity differs: '+key)
            if row['role'] == 'candidate-u8':
                require(native['query_only'] is True and native['indexing_performed'] is False,
                        'retained U8 is not the reviewed query-only capture')
            else:
                stage = next(s for s in packet['stages'] if s['family'] == row['family'])
                wrapper.validate_capture(native, stage, packet)
            result = owner.grade_family(native, rubric, mask, fixture, inputs[source_manifest], cache)
            result['role'] = row['role']; results.append(result)
            del native
        comparisons = []
        for candidate in (r for r in results if r['role'].startswith('candidate-')):
            for control in (r for r in results if r['role'].startswith('control-')):
                a, b = candidate['summary'], control['summary']
                gain = Fraction(a['complete_count']-b['complete_count'], 20)
                delta = Fraction(*a['mean_evidence_recall_fraction'])-Fraction(*b['mean_evidence_recall_fraction'])
                comparisons.append({'candidate': candidate['family'], 'control': control['family'],
                    'complete_gain_fraction': [gain.numerator, gain.denominator],
                    'mean_delta_fraction': [delta.numerator, delta.denominator],
                    'descriptive_DEV_paired_reference_met': gain >= Fraction(3, 100) and delta >= Fraction(-2, 100),
                    'FINAL_or_promotion_pass': False})
        owner.verify_fixture(fixture, metadata)
        for path, expected in inputs.items(): wrapper.file_pin(path, expected)
        output.mkdir()
        clock.atomic_stream(output/'grading.private.json', {
            'schema': 'airk.private-corrected-graph-matched-DEV-grading/1', 'families': results,
            'paired_comparisons': comparisons, 'source_inputs': {str(p): v for p, v in inputs.items()},
            'retained_U8': packet['retained_u8'], 'old_same_index_bridge_claimed': False,
            'FINAL_or_promotion_pass': False, 'scope': 'All64 original DEV,20aligned44refused; same R54 image/corrected graph algorithm; retainedU8 exact vector regeneration; timing not latency qualification.'})
        receipt = {'elapsed_seconds': time.monotonic()-clock.START, 'peak_committed_process_bytes': clock.peak_memory(),
                   'hard_seconds': 60, 'hard_bytes': 268435456, 'singlethread_BLAS': True, 'native_or_model_execution': False}
        clock.atomic_stream(output/'operation-receipt.json', receipt); clock.done.set()
        print(json.dumps({'summaries': [dict(role=r['role'], **r['summary']) for r in results],
                          'paired_comparisons': comparisons, 'operation': receipt}))

if __name__ == '__main__':
    main()
