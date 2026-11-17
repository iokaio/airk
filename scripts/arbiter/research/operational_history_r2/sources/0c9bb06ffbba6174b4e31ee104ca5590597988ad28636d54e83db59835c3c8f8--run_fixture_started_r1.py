# SPDX-License-Identifier: Apache-2.0
"""Execute only the exact Root-reviewed private fixture packet."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import time
from prepare_fixture import pin
from fixture_bound_support_r1 import write_started

def verify(row):
    if pin(row['path']) != row:
        raise ValueError('pinned input changed: ' + row['path'])

def same_control_path(actual, expected):
    if not isinstance(actual, str):
        return False
    if actual == expected:
        return True
    try:
        return Path(actual).samefile(expected)
    except OSError:
        return False

def closure_matches(ready, closed, work, input_digest, image_digest, *, control_root=None, prior_instance=None):
    if not isinstance(ready, dict) or not isinstance(closed, dict):
        return False
    control = control_root if control_root is not None else str(Path(work) / 'control')
    return (ready.get('schema') == 'airk.private-arbiter-fixture/1'
            and ready.get('inputs_sha256') == input_digest
            and same_control_path(ready.get('control_root'), control)
            and ready.get('health', {}).get('executable_sha256') == image_digest
            and closed.get('schema') == 'airk.private-arbiter-fixture-closed/1'
            and same_control_path(closed.get('control_root'), control)
            and closed.get('instance') == ready.get('health', {}).get('instance')
            and ready.get('health', {}).get('instance') is not None
            and (prior_instance is None or ready['health']['instance'] != prior_instance)
            and closed.get('shutdown_completed') is True
            and closed.get('service_lease_reacquired') is True)

def restart_inputs(packet):
    descriptor=packet.get('restart')
    explicit=packet.get('existing_control_root')
    if descriptor is None:
        if explicit is not None:raise ValueError('existing control requires explicit pinned restart descriptor')
        return None
    if not isinstance(explicit,str) or not Path(explicit).is_absolute() or not Path(explicit).is_dir():
        raise ValueError('restart requires explicit existing absolute control root')
    verify(descriptor)
    request=json.loads(Path(descriptor['path']).read_bytes())
    if set(request)!={'ready','closed','status'}:raise ValueError('restart descriptor must bind only original Ready, closed and Status')
    refs=[descriptor];values={}
    for key,row in request.items():
        if set(row)!={'path','sha256'} or not Path(row['path']).is_absolute():raise ValueError('native restart descriptor reference differs')
        actual=pin(row['path'])
        if actual['sha256']!=row['sha256']:raise ValueError('original restart evidence changed')
        refs.append(actual);values[key]=json.loads(Path(row['path']).read_bytes())
    ready,closed,status=(values[key] for key in ('ready','closed','status'))
    old_work=Path(request['ready']['path']).parent
    if (Path(request['ready']['path']).name!='ready.json'
        or not same_control_path(request['closed']['path'],str(old_work/'closed.json'))
        or not same_control_path(explicit,str(old_work/'control'))
        or same_control_path(packet['work'],str(old_work))):
        raise ValueError('restart must preserve original control and use fresh metadata work')
    if (ready.get('evaluation') is not True or ready.get('release_qualified') is not False
        or closed.get('evaluation') is not True or closed.get('release_qualified') is not False
        or not closure_matches(ready,closed,str(old_work),packet['inputs']['sha256'],ready.get('health',{}).get('executable_sha256'),control_root=explicit)
        or status.get('schema')!='airk.arbiter.status/2' or status.get('initialization') is not None
        or status.get('configuration',{}).get('catalog')!=ready.get('health',{}).get('catalog_clock',{}).get('namespace')
        or any(status.get(stage,{}).get('queued')!=0 or status.get(stage,{}).get('running')!=0 for stage in ('focus','context'))):
        raise ValueError('genuine prior closure/current idle status and owner identity required')
    return {'control_root':explicit,'prior_instance':ready['health']['instance'],'refs':refs,'descriptor':descriptor}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--packet', required=True)
    parser.add_argument('--packet-sha256', required=True)
    parser.add_argument('--decision', required=True)
    parser.add_argument('--decision-sha256', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    started_monotonic = time.monotonic()
    started = time.time()
    if pin(args.packet)['sha256'] != args.packet_sha256 or pin(args.decision)['sha256'] != args.decision_sha256:
        raise ValueError('packet or decision digest mismatch')
    packet = json.loads(Path(args.packet).read_text(encoding='utf-8'))
    decision = json.loads(Path(args.decision).read_text(encoding='utf-8'))
    if decision.get('approved_fixture_packet_sha256') != args.packet_sha256:
        raise ValueError('Root decision must approve this exact fixture packet')
    for name in ('image', 'inputs', 'cargo_json', 'build_receipt', 'owner'):
        verify(packet[name])
    for row in packet['staged_metadata']:
        verify(row)
    if packet.get('pipeline_pending_native_plan') is not False or not packet.get('semantic_plan'):
        raise ValueError('actual native semantic plan must be bound before CAL startup')
    verify(packet['semantic_plan'])
    semantic = json.loads(Path(packet['semantic_plan']['path']).read_text(encoding='utf-8'))
    if semantic.get('schema') != 'airk.private-selected-native-semantic-setup/1' or semantic.get('NN_invocations') != 0:
        raise ValueError('actual native metadata-only semantic plan required')
    pipeline = semantic['object_plan']['pipeline_digest']
    if not isinstance(pipeline, str) or len(pipeline) != 64:
        raise ValueError('native plan pipeline digest missing')
    restart = restart_inputs(packet)
    if Path(packet['work']).exists():
        raise ValueError('fixture work already exists')
    output = Path(args.output)
    output.mkdir(exist_ok=False)
    started_reference = write_started(output, pin(args.packet), pin(args.decision), packet, started, __file__)
    started_metadata = json.loads(Path(started_reference['path']).read_bytes())
    snapshot = output / 'actual-service-fixture.exe'
    with Path(packet['image']['path']).open('rb') as source, snapshot.open('xb') as destination:
        shutil.copyfileobj(source, destination, 1024 * 1024)
    snapshot_pin = pin(snapshot)
    if (snapshot_pin['bytes'], snapshot_pin['sha256']) != (packet['image']['bytes'], packet['image']['sha256']):
        raise ValueError('copied actual Service image differs from Cargo artifact')
    spec = importlib.util.spec_from_file_location('owned_fixture', packet['owner']['path'])
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    environment = os.environ.copy()
    environment.update({
        'AIRK_ARBITER_EVALUATION_INPUTS': packet['inputs']['path'],
        'AIRK_ARBITER_EVALUATION_INPUTS_SHA256': packet['inputs']['sha256'],
        'AIRK_ARBITER_EVALUATION_WORK': packet['work'],
        'AIRK_ARBITER_EVALUATION_SECONDS': str(packet['fixture_seconds']),
        'AIRK_SELECTED_CONTEXT_CAL_OBSERVER': 'true',
        'AIRK_NATIVE_DISPATCH_OBSERVER': 'true',
        'AIRK_NATIVE_WARM_OBSERVER': 'true' if packet.get('native_warm_observer') else 'false',
        'AIRK_SELECTED_CONTEXT_MINILM_PIPELINE': pipeline,
    })
    environment.pop('AIRK_ARBITER_EVALUATION_RESTART',None)
    environment.pop('AIRK_ARBITER_EVALUATION_RESTART_SHA256',None)
    if restart:
        environment.update({'AIRK_ARBITER_EVALUATION_RESTART':restart['descriptor']['path'],
                            'AIRK_ARBITER_EVALUATION_RESTART_SHA256':restart['descriptor']['sha256']})
    remaining = packet['owner_seconds'] - (time.monotonic() - started_monotonic)
    if remaining <= 0:
        raise TimeoutError('original fixture owner phase exhausted before suspended launch')
    result = owner.run_evaluation_fixture(
        [str(snapshot), '--ignored', '--exact', packet['selector'], '--nocapture'],
        packet['cwd'], output / 'native.log', remaining, environment)
    drift = [name for name in ('image', 'inputs', 'cargo_json', 'build_receipt', 'owner')
             if pin(packet[name]['path']) != packet[name]]
    if pin(packet['semantic_plan']['path']) != packet['semantic_plan']:
        drift.append('semantic_plan')
    for row in packet['staged_metadata']:
        if pin(row['path']) != row:
            drift.append(row['path'])
    if restart:
        for reference in restart['refs']:
            if pin(reference['path'])!=reference:drift.append(reference['path'])
    closed_path = Path(packet['work']) / 'closed.json' 
    closed = json.loads(closed_path.read_text(encoding='utf-8')) if closed_path.exists() else None
    ready_path = Path(packet['work']) / 'ready.json'
    ready = json.loads(ready_path.read_text(encoding='utf-8')) if ready_path.exists() else None
    warm_closure = None
    warm_valid = not packet.get('native_warm_observer')
    if packet.get('native_warm_observer'):
        warm_summary = Path(packet['work']) / 'warm-completions.closed.json'
        warm_journal = Path(packet['work']) / 'warm-completions.jsonl'
        if warm_summary.exists() and warm_journal.exists():
            summary = json.loads(warm_summary.read_text(encoding='utf-8'))
            raw = warm_journal.read_bytes()
            records = [json.loads(row) for row in raw.splitlines() if row]
            warm_valid = bool(ready and ready.get('warm_observer_id')
                              and summary.get('schema') == 'airk.private.arbiter-warm-recorder/1'
                              and summary.get('observer_id') == ready['warm_observer_id']
                              and summary.get('complete') is True and summary.get('dropped') == 0
                              and summary.get('error') is None and summary.get('bytes') == len(raw)
                              and summary.get('records') == len(records) and summary.get('last_sequence') == len(records)
                              and all(row['observer_id'] == ready['warm_observer_id'] and row['sequence'] == i + 1
                                      for i, row in enumerate(records)))
            warm_closure = {'summary': pin(warm_summary), 'journal': pin(warm_journal), 'validated': warm_valid}
    if pin(started_reference['path']) != started_reference:
        drift.append('original_started_metadata')
    for name in ('launcher', 'bound_support'):
        if pin(started_metadata[name]['path']) != started_metadata[name]:
            drift.append(name)
    phase_expired = time.monotonic() - started_monotonic >= packet['owner_seconds']
    complete = bool(owner.successful(result) and warm_valid and not phase_expired and not drift
                    and closure_matches(ready, closed, packet['work'],
                                        packet['inputs']['sha256'], snapshot_pin['sha256'],
                                        control_root=restart['control_root'] if restart else None,
                                        prior_instance=restart['prior_instance'] if restart else None))
    receipt = {'schema': 'airk.private-selected-cal-fixture-owned-execution/1',
               'complete': complete, 'release_qualified': False,
               'packet': pin(args.packet), 'decision': pin(args.decision),
               'materialized_image': snapshot_pin,
               'started_unix': started, 'finished_unix': time.time(),
               'started_metadata': started_reference,
               'owned': result, 'post_input_drift': drift,
               'whole_phase_expired': phase_expired, 'native_warm_closure': warm_closure,
               'same_owner_restart': restart,
               'ready': pin(ready_path) if ready else None,
               'closed': pin(closed_path) if closed else None}
    (output / 'execution.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    if not complete:
        raise SystemExit('actual fixture closure failed; retained original diagnostics')

if __name__ == '__main__':
    main()
