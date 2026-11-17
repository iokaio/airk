# SPDX-License-Identifier: Apache-2.0
"""Immutable actual launcher start and conservative original fixture bound."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path

SCHEMA = 'airk.private-selected-cal-fixture-owned-execution/1'
FIXTURE_MILLISECONDS = 10_800_000

def pin(path):
    path = Path(path).resolve(strict=True)
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest, 'bytes': path.stat().st_size}

def verified(reference):
    if pin(reference['path']) != reference:
        raise ValueError('immutable fixture metadata changed')
    return json.loads(Path(reference['path']).read_bytes())

def write_started(output, packet_reference, decision_reference, packet, started_unix, launcher):
    value = {'schema': SCHEMA, 'complete': False, 'release_qualified': False,
             'packet': packet_reference, 'decision': decision_reference,
             'launcher': pin(launcher), 'bound_support': pin(__file__),
             'started_unix': started_unix,
             'original_fixture_seconds': packet['fixture_seconds'],
             'original_owner_seconds': packet['owner_seconds'],
             'fixture_work': packet['work'],
             'start_basis': 'actual_launcher_entry_before_native_entry'}
    path = Path(output) / 'started.json'
    with path.open('xb') as stream:
        stream.write((json.dumps(value, indent=2) + '\n').encode('utf-8'))
        stream.flush()
        os.fsync(stream.fileno())
    return pin(path)

def original_bound(start_reference, packet_reference):
    start = verified(start_reference)
    packet = verified(packet_reference)
    observed = start.get('started_unix')
    if (start.get('schema') != SCHEMA or start.get('complete') is not False
            or start.get('release_qualified') is not False
            or packet.get('schema') != 'airk.private-selected-cal-fixture-launch/1'
            or type(observed) not in (int, float) or not math.isfinite(observed) or observed <= 0
            or start.get('packet') != packet_reference
            or start.get('original_fixture_seconds') != packet.get('fixture_seconds')
            or type(packet.get('fixture_seconds')) is not int or packet['fixture_seconds'] != 10800
            or start.get('original_owner_seconds') != packet.get('owner_seconds')
            or start.get('fixture_work') != packet.get('work')
            or start.get('start_basis') != 'actual_launcher_entry_before_native_entry'):
        raise ValueError('actual original10800-second launcher evidence required')
    decision = verified(start['decision'])
    if decision.get('approved_fixture_packet_sha256') != packet_reference['sha256']:
        raise ValueError('actual fixture start lacks exact Root packet approval')
    for name in ('launcher', 'bound_support'):
        if pin(start[name]['path']) != start[name]:
            raise ValueError('actual launcher/support bytes changed')
    milliseconds = int(observed * 1000)
    return {'original_start_unix_ms': milliseconds,
            'absolute_deadline_unix_ms': milliseconds + FIXTURE_MILLISECONDS,
            'original_start_evidence': start_reference, 'original_start_field': 'started_unix'}

def validate_bound(reference, packet_reference):
    bound = verified(reference)
    expected = original_bound(bound['original_start_evidence'], packet_reference)
    if (set(bound) != set(expected) or type(bound['absolute_deadline_unix_ms']) is not int
            or bound['original_start_unix_ms'] != expected['original_start_unix_ms']
            or bound['original_start_field'] != 'started_unix'
            or not expected['original_start_unix_ms'] < bound['absolute_deadline_unix_ms'] <= expected['absolute_deadline_unix_ms']):
        raise ValueError('original conservative fixture lifetime was replaced or extended')
    return bound

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--started', required=True)
    parser.add_argument('--started-sha256', required=True)
    parser.add_argument('--fixture-packet', required=True)
    parser.add_argument('--fixture-packet-sha256', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    start = pin(args.started)
    packet = pin(args.fixture_packet)
    if start['sha256'] != args.started_sha256 or packet['sha256'] != args.fixture_packet_sha256:
        raise ValueError('reviewed original metadata pin differs')
    bound = original_bound(start, packet)
    with Path(args.output).open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(bound, stream, indent=2)
        stream.write('\n')
    print(json.dumps(pin(args.output)))

if __name__ == '__main__':
    main()
