# SPDX-License-Identifier: Apache-2.0
"""Metadata only: reuse native packet producers and exact prior ownership proof."""
import argparse
import importlib.util
import json
from pathlib import Path
import sys

BASE = Path(__file__).resolve().parent.parent
ROOT = BASE.parents[3]
sys.path.insert(0, str(BASE))
from prepare_fixture import cargo_image, pin

SCIENCE = ('maximum_questions', 'maximum_probes', 'Context_task_forward',
           'Focus_profile', 'Context_bootstrap_profile', 'MiniLM_pipeline',
           'task_policy', 'source_manifest_sha256', 'corpus_sha256', 'frozen_configuration')

def require_closed(value):
    owned = value['owned']
    if not (value['complete'] is True and value['post_input_drift'] == []
            and value['whole_phase_expired'] is False and owned['exit_code'] == 0
            and owned['owned_descendants_settled'] is True
            and owned['owned_job_final_active_processes'] == 0
            and owned['owned_job_forced_termination'] is False):
        raise ValueError('actual latest native ownership closure required')

def same_science(release, previous):
    if release.get('approved') is not True or any(release.get(k) != previous.get(k) for k in SCIENCE):
        raise ValueError('Root-approved corrected-source release must preserve scientific inputs/caps')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('kind', choices=('fixture', 'capture'))
    parser.add_argument('--cargo-json', type=Path)
    parser.add_argument('--build-receipt', type=Path)
    parser.add_argument('--image', type=Path)
    parser.add_argument('--approved-release', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    prep = json.loads((Path(__file__).parent / 'preparation.json').read_bytes())
    for key in ('original_owner_anchor', 'latest_actual_fixture_closure', 'latest_native_closed',
                'launcher', 'capture_controller', 'base_template', 'postready_release_producer'):
        if pin(prep[key]['path']) != prep[key]:
            raise ValueError('retained preparation input drift: ' + key)
    require_closed(json.loads(Path(prep['latest_actual_fixture_closure']['path']).read_bytes()))
    if args.kind == 'fixture':
        if not all((args.cargo_json, args.build_receipt, args.image)):
            parser.error('fixture requires actual Cargo JSON/build receipt/retained image')
        build = json.loads(args.build_receipt.read_bytes())
        if build.get('exit_code') != 0 or build.get('inputs_unchanged') is not True:
            raise ValueError('actual successful unchanged build required')
        image = pin(args.image)
        actual = pin(cargo_image(args.cargo_json))
        if (image['bytes'], image['sha256']) != (actual['bytes'], actual['sha256']):
            raise ValueError('retained image differs from actual Cargo artifact')
        value = json.loads((BASE / 'fixture-packet-restart-r135.proposed.json').read_bytes())
        value.update(image=image, cargo_json=pin(args.cargo_json), build_receipt=pin(args.build_receipt),
                     native_source_group=build['groups']['native']['sha256'], work=prep['work'])
        if Path(value['work']).exists():
            raise ValueError('next metadata work must remain fresh')
    else:
        if args.approved_release is None:
            parser.error('capture requires exact actual Root-approved post-Ready release')
        release = json.loads(args.approved_release.read_bytes())
        previous = json.loads((BASE / 'CAL-capture-release.root-approved-corrected-r137.json').read_bytes())
        same_science(release, previous)
        value = json.loads((BASE / 'CAL-owned-controller-packet.root-approved-corrected-r137.json').read_bytes())
        value.update(release=pin(args.approved_release),
                     capture_output=str(ROOT / 'artifacts/advanced-split/models/actual-composed-service-r2/CAL-capture-next-corrected-r1'),
                     owner_output=str(BASE / 'CAL-capture-owned-next-corrected-r1'))
        if any(Path(value[key]).exists() for key in ('capture_output', 'owner_output')):
            raise ValueError('next capture outputs must remain fresh')
    with args.output.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
    print(json.dumps(pin(args.output)))

if __name__ == '__main__':
    main()
