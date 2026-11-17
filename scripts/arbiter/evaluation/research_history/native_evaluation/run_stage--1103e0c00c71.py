# SPDX-License-Identifier: Apache-2.0
"""Private matched corrected-graph DEV stages. No build, gradients or FINAL entry."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import time
import types

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def require(condition, message):
    if not condition:
        raise ValueError(message)

def file_pin(path, expected):
    path = Path(path)
    state = path.stat(follow_symlinks=False)
    require(path.is_file() and not path.is_symlink() and state.st_nlink == 1
            and not getattr(state, 'st_file_attributes', 0) & 0x400 and sha(path) == expected,
            'sealed source/artifact identity differs: ' + str(path))

def isolated(child, frozen):
    child, frozen = Path(child).resolve(), Path(frozen).resolve()
    return not child.is_relative_to(frozen) and not frozen.is_relative_to(child)

def natural(result):
    return (result.get('exit_code') == 0 and result.get('timed_out') is False
        and result.get('log_oversized') is False and result.get('owned_descendants_settled') is True
        and result.get('owned_job_forced_termination') is False
        and result.get('owned_job_final_active_processes') == 0
        and result.get('owned_job_final_member_pids') == [] and result.get('owned_total_processes') == 1
        and not result.get('unexpected_third_process') and not result.get('cleanup_refusal'))

def validate_capture(capture, stage, packet):
    require(capture.get('complete') is True and capture.get('family') == stage['family'], 'incomplete/foreign capture')
    for key, value in packet['capture_common'].items():
        require(capture.get(key) == value, 'native capture common identity differs: ' + key)
    require(capture['model_json_sha256'] == stage['model_json_sha256']
            and capture['encoder_input_sha256'] == stage['descriptor_sha256'], 'native model/descriptor differs')
    require(capture['source_before_sha256'] == capture['source_after_sha256']
            == packet['source_roster_sha256'], 'source roster changed')
    records = capture['records']
    require([row['id'] for row in records] == packet['original_question_order'], 'original all64 order differs')
    require(len(capture['collections']) == 1 and capture['collections'][0]['reused'] is False,
            'fresh single-collection native indexing required')
    publication = capture['collections'][0]['publication']
    for row in records:
        require('native_error' not in row and type(row['query_us']) is int and row['query_us'] >= 0,
                'failed/malformed native query')
        require(Path(row['source_root']).samefile(packet['source_root']), 'native source differs')
        body = row['primary_vector']
        require(body['publication'] == publication and len(body['hits']) <= 10, 'native publication/top10 differs')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet', type=Path, default=HERE/'packet.json')
    parser.add_argument('--packet-sha256', required=True)
    parser.add_argument('--runner-sha256', required=True)
    parser.add_argument('--family', required=True)
    parser.add_argument('--execute-reviewed', action='store_true')
    args = parser.parse_args()
    file_pin(args.packet, args.packet_sha256)
    file_pin(__file__, args.runner_sha256)
    packet = json.loads(args.packet.read_bytes())
    stages = [s for s in packet['stages'] if s['family'] == args.family]
    require(len(stages) == 1, 'one reviewed DEV family required')
    stage = stages[0]
    pins = {Path(p): v for p, v in packet['pins'].items()}
    pins.update({args.packet.resolve(): args.packet_sha256, Path(__file__).resolve(): args.runner_sha256})
    for path, expected in pins.items():
        file_pin(path, expected)
    compilation = json.loads(Path(packet['compile_receipt']['path']).read_bytes())
    require(compilation['exit_code'] == 0, 'successful R54 compiler receipt required')
    for path, expected in packet['compiled_source_seals'].items():
        require(compilation['groups']['native']['files'][path] == expected, 'R54 source relationship differs')
    dep = Path(packet['kernel_depinfo']).read_text().replace('\\', '/')
    require(stage['owning_module'] in dep, 'actual compiler did not consume selected DEV owner')
    anchor = Path(stage['anchor'])
    require(anchor.is_dir() and {p.name for p in anchor.iterdir()} == {'tmp'}
            and not any((anchor/'tmp').iterdir()), 'fresh private native stage anchor required')
    for frozen in packet['frozen_roots']:
        require(isolated(anchor, frozen), 'output anchor overlaps frozen source/model/proof')
    capture_path = Path(stage['environment']['AIRK_NATIVE_DEV64_OUTPUT'])
    require(capture_path.parent.resolve() == anchor.resolve() and capture_path.name == 'capture.json'
            and not capture_path.exists(), 'fresh exact capture sibling required')
    data_path = Path(stage['environment']['AIRK_NATIVE_DEV64_DATA'])
    require(data_path.parent.resolve() == anchor.resolve() and data_path.name == 'data'
            and not data_path.exists(), 'fresh exact native data sibling required')
    if not args.execute_reviewed:
        print(json.dumps({'static_preflight': True, 'stage_started': False, 'family': args.family,
                          'sealed_files': len(pins), 'requires_native_selector_listing_by_root': stage['selector']}))
        return
    require(os.name == 'nt', 'reviewed Windows owned-process stage required')
    for previous in packet['stages'][:packet['stages'].index(stage)]:
        previous_path = HERE/('execution-'+previous['family'])/'execution.json'
        require(previous_path.is_file(), 'sequential batch must retain prior stage completion')
        previous_result = json.loads(previous_path.read_bytes())
        require(previous_result['complete'] is True and previous_result['packet_sha256'] == args.packet_sha256
                and natural(previous_result['owned']) and not previous_result['post_input_drift'],
                'stop batch on first failure; no later native stage')
    # List only after explicit execution release; this is not a semantic or model call.
    binary = Path(packet['binary']['path'])
    listed = subprocess.run([str(binary), '--exact', stage['selector'], '--ignored', '--list'],
                            cwd=ROOT, capture_output=True, text=True, timeout=10)
    require(listed.returncode == 0 and listed.stdout.count(stage['selector']+': test') == 1,
            'exactly one native selector required')
    work = HERE/('execution-'+args.family)
    require(not work.exists(), 'retain previous attempt; no overwrite/replay')
    raw = Path(packet['owned_process_source']).read_bytes()
    owner = types.ModuleType('_matched_dev_owned_process'); owner.__file__ = packet['owned_process_source']
    exec(compile(raw, owner.__file__, 'exec'), owner.__dict__)
    environment = {k: v for k, v in os.environ.items() if not k.startswith('AIRK_NATIVE_') and k != 'AIRK_CANDIDATE_ENCODER_INPUT_SHA256'}
    environment.update(stage['environment'])
    argv = [str(binary), '--exact', stage['selector'], '--ignored', '--nocapture', '--test-threads=1']
    limits = packet['limits']
    work.mkdir(); (work/'stdin.txt').write_bytes(b'')
    request = {'argv': argv, 'environment': stage['environment'], 'limits': limits,
               'packet_sha256': args.packet_sha256, 'runner_sha256': args.runner_sha256,
               'pre_input_seals': {str(p): v for p, v in pins.items()}, 'selector_list': listed.stdout}
    (work/'request.json').write_text(json.dumps(request, indent=2)+'\n', encoding='utf-8')
    def suspended(api, job, info, deadline):
        members, active, total = owner.job_snapshot(api, job)
        require(members == [int(info.pid)] and active == total == 1, 'one expected native process required')
        image = owner.image_proof(api, info.process, packet['binary'])
        require(time.monotonic() < deadline, 'original deadline expired before resume')
        return {'members': members, 'active': active, 'total': total, 'image': image}
    started = time.monotonic()
    with (work/'native.log').open('xb') as log, (work/'stdin.txt').open('rb') as stdin:
        owned = owner.run_windows_owned_training(argv, ROOT, log, limits['seconds'], environment,
            stdin, limits['host_bytes'], before_resume=suspended)
    drift = [str(p) for p, expected in pins.items() if sha(p) != expected]
    result = {'owned': owned, 'elapsed_seconds': time.monotonic()-started, 'post_input_drift': drift,
              'packet_sha256': args.packet_sha256, 'family': args.family, 'complete': False,
              'quality_or_release_qualified': False}
    if capture_path.is_file():
        result['capture_sha256'] = sha(capture_path)
        try:
            validate_capture(json.loads(capture_path.read_bytes()), stage, packet)
            result['complete'] = natural(owned) and not drift
        except (ValueError, KeyError, TypeError, OSError) as error:
            result['capture_refusal'] = str(error)
    (work/'execution.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(result))
    if not result['complete']:
        raise SystemExit(1)

if __name__ == '__main__':
    main()
