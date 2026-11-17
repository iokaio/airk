# SPDX-License-Identifier: Apache-2.0
"""Attach reviewed chunk protocol and actual original lifetime to post-Ready metadata."""
import argparse
import copy
import json
from pathlib import Path
from fixture_bound_support_r1 import pin, verified, validate_bound

PROTOCOL_SHA = 'da5eb4de2642deb952cdbc608b302d99137965ed43969bfafd5dcc07c187ac99'
RELEASE_SCHEMA = 'airk.private-selected-Context-FocusON-CAL-capture-release/1'

def attach(release, protocol_reference, bound_reference):
    if (release.get('schema') != RELEASE_SCHEMA or release.get('approved') is True
            or release.get('maximum_questions') != 100 or release.get('maximum_probes') != 500
            or release.get('Context_task_forward') is not False):
        raise ValueError('unapproved original CAL100/500 post-Ready release required')
    if protocol_reference['sha256'] != PROTOCOL_SHA:
        raise ValueError('exact recorded chunk protocol required')
    protocol = verified(protocol_reference)
    if (protocol.get('schema') != 'airk.private-cal-chunked-census/1'
            or protocol['frozen_source_manifest']['sha256'] != release['source_manifest_sha256']
            or protocol['frozen_corpus']['sha256'] != release['corpus_sha256']):
        raise ValueError('frozen protocol/data identity differs')
    packet_reference = release['actual_binding_evidence']['fixture_packet']
    bound = validate_bound(bound_reference, packet_reference)
    started = verified(bound['original_start_evidence'])
    packet = verified(packet_reference)
    ready_reference = release['actual_binding_evidence']['Ready']
    verified(ready_reference)
    if (not Path(packet['work']).samefile(Path(ready_reference['path']).parent)
            or not Path(release['paths']['fixture_ready']).samefile(ready_reference['path'])):
        raise ValueError('post-Ready release and original lifetime name different fixture work')
    result = copy.deepcopy(release)
    if (release['frozen_configuration']['focus']['enabled'] is not True
            or release['frozen_configuration']['focus']['selected_profile'] != release['Focus_profile']
            or release.get('Focus_configuration') not in (None, 'ON')):
        raise ValueError('actual frozen Focus ON configuration required')
    result['Focus_configuration'] = 'ON'
    result['chunk_protocol_sha256'] = protocol_reference['sha256']
    result['original_fixture_bound'] = bound_reference
    references = [protocol_reference, bound_reference, bound['original_start_evidence'],
                  packet_reference, started['decision'], started['launcher'], started['bound_support'],
                  pin(__file__), pin(Path(__file__).with_name('fixture_bound_support_r1.py'))]
    for name in ('frozen_question_scopes', 'frozen_corpus', 'frozen_source_manifest', 'frozen_rubric'):
        reference = protocol[name]
        if release['input_pins'].get(reference['path']) != reference['sha256']:
            raise ValueError('protocol frozen data is outside the reviewed post-Ready release')
        references.append(reference)
    for reference in references:
        if pin(reference['path']) != reference:
            raise ValueError('common release input changed')
        old = result['input_pins'].get(reference['path'])
        if old is not None and old != reference['sha256']:
            raise ValueError('common release cannot repin conflicting source evidence')
        result['input_pins'][reference['path']] = reference['sha256']
    result['approved'] = None
    result['status'] = 'ACTUAL_COMMON_CAL_METADATA_BOUND_ROOT_CONTROLLER_RELEASE_REQUIRED'
    result['post_capture_required'] = ('All13 natural owned phase closures within the SAME original fixture bound; '
        'exact100 unique ordered completed rows and at most500 total/5per-question probes. Prior failed attempts '
        'are separate and excluded; no failed/partial/stopped member qualifies. Original science/config/CPU/'
        'memory/per-call30s/2048tokens/bytes unchanged. Aggregate retains every original member proof.')
    return result

def main():
    parser = argparse.ArgumentParser()
    for name in ('release', 'protocol', 'fixture-bound'):
        parser.add_argument('--' + name, required=True)
        parser.add_argument('--' + name + '-sha256', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    refs = {}
    for name in ('release', 'protocol', 'fixture_bound'):
        reference = pin(getattr(args, name))
        if reference['sha256'] != getattr(args, name + '_sha256'):
            raise ValueError('exact independently supplied input pin differs')
        refs[name] = reference
    value = attach(verified(refs['release']), refs['protocol'], refs['fixture_bound'])
    value['input_pins'][refs['release']['path']] = refs['release']['sha256']
    with Path(args.output).open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
    print(json.dumps(pin(args.output)))

if __name__ == '__main__':
    main()
