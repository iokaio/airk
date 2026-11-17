# SPDX-License-Identifier: Apache-2.0
"""Prepare exact native CAL setup argv; never emit a native request DTO."""
import argparse
import json
from pathlib import Path
from prepare_fixture import ROOT, pin

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--runtime-pair', required=True)
    parser.add_argument('--minilm-directory', required=True)
    parser.add_argument('--fixture-packet', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--existing-drafts')
    parser.add_argument('--require-ready', action='store_true')
    args = parser.parse_args()
    fixture = json.loads(Path(args.fixture_packet).read_text(encoding='utf-8'))
    data_path = ROOT / 'artifacts/advanced-split/models/selected-native-goal-driver-r1/native-execution-preparation-r1/data-bindings.actual.json'
    data = json.loads(data_path.read_text(encoding='utf-8'))['partitions']['CAL']
    runtime = Path(args.runtime_pair).resolve(strict=True)
    ctl = runtime / 'airkctl.exe'
    build = json.loads((runtime / 'airk-build.json').read_text(encoding='utf-8'))
    images = [pin(runtime / name) for name in ('airk.exe', 'airkctl.exe')]
    if any(row['sha256'] != build['runtime'][key] for row, key in
           zip(images, ('airk_sha256', 'airkctl_sha256'))):
        raise ValueError('matched pair images disagree with actual builder metadata')
    model = Path(args.minilm_directory).resolve(strict=True)
    work = Path(fixture['work'])
    ready_path = work / 'ready.json'
    ready_pin = pin(ready_path) if ready_path.exists() else None
    if args.require_ready and ready_pin is None:
        raise ValueError('actual native Ready required before setup approval')
    if ready_pin:
        ready = json.loads(ready_path.read_text(encoding='utf-8'))
        if (ready.get('schema') != 'airk.private-arbiter-fixture/1'
                or ready.get('inputs_sha256') != fixture['inputs']['sha256']
                or ready.get('health', {}).get('executable_sha256') != fixture['image']['sha256']
                or not Path(ready['control_root']).samefile(work / 'control')):
            raise ValueError('actual native Ready identity/input/control mismatch')
    # Native emitter creates this protected output; it must not exist yet.
    drafts = Path(args.existing_drafts).resolve(strict=True) if args.existing_drafts else work.parent / 'cal-vector-drafts-r102'
    if not args.existing_drafts and drafts.exists():
        raise ValueError('native draft output must be fresh')
    semantic_plan = pin(drafts / 'minilm-plan.json') if args.existing_drafts else None
    if semantic_plan:
        actual_plan = json.loads(Path(semantic_plan['path']).read_text(encoding='utf-8'))
        if not Path(actual_plan['source_directory']).samefile(model):
            raise ValueError('existing native plan names another model directory')
    common = [str(ctl), '--root', data['source_root'], '--data-dir', str(work / 'index-data'),
              '--service-runtime-dir', str(work / 'control')]
    selected = common + ['--collection', 'selected-context-cal-r1']
    commands = [{
        'label': 'actual-native-semantic-drafts', 'deadline_seconds': 60,
        'argv': [fixture['image']['path'], '--ignored', '--exact',
                 'service_runtime::evaluation_fixture_tests::emit_private_selected_vector_configuration', '--nocapture'],
        'environment': {'AIRK_SELECTED_MINILM_DIRECTORY': str(model),
                        'AIRK_SELECTED_VECTOR_DRAFT_OUTPUT': str(drafts)},
        'before_fixture_start': True,
    }, {
        'label': 'actual-index-setup', 'deadline_seconds': 300,
        'argv': common + ['library', 'setup', '--name', 'Selected Context CAL',
                          '--index-collection', 'selected-context-cal-r1', '--watch', 'off', '--apply'],
    }]
    if args.existing_drafts:
        commands = [step for step in commands if not step.get('before_fixture_start')]
    for name in ('minilm-install.json', 'vectors-enabled.json', 'vector-selection-all.json'):
        commands.append({'label': 'configure-' + name, 'deadline_seconds': 300,
                         'argv': selected + ['configure', '--file', str(drafts / name)]})
    for command in ('setup-model', 'index'):
        commands.append({'label': command, 'deadline_seconds': 1800,
                         'argv': selected + [command]})
    result = {
        'schema': 'airk.private-selected-cal-native-setup-commands/1',
        'prepared_only': True, 'qualification_granted': False,
        'fixture_packet': pin(args.fixture_packet), 'data_bindings': pin(data_path),
        'fixture_ready': ready_pin,
        'runtime_images': images,
        'runtime_metadata': [pin(runtime / name) for name in ('airk-build.json', 'airk-runtime.json')],
        'model_definition': pin(model / 'model.json'), 'commands': commands,
        'semantic_plan': semantic_plan,
        'post_setup_required': [
            'Read actual CatalogIndexSetupApplied.receipt.index_id from retained native stdout.',
            'Create one singleton set using library set-create --name Selected Context CAL Set --index ACTUAL_INDEX_ID --yes.',
            'Observe successful semantic publication; submission is not readiness.',
            'Emit/apply actual Focus Configure, then bounded successful Status observations until Ready.',
            'Emit/apply actual Compact Context Configure, then bounded successful Status observations until Ready.',
            'Emit cal-live bindings, freeze exact Root capture release, run existing capture_selected_service_cal.py once.',
            'Close actual SDK session before native shutdown; retain genuine closed.json and owned natural closure.',
        ],
    }
    with Path(args.output).open('x', encoding='utf-8', newline='\n') as out:
        json.dump(result, out, indent=2)
        out.write('\n')

if __name__ == '__main__':
    main()
