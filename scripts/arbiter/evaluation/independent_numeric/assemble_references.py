# SPDX-License-Identifier: Apache-2.0
"""Join actual independent runs into an unqualified native template; no NN or approvals."""
import argparse
import copy
import json
import math
from pathlib import Path
from maintained_helpers import worker, private_root

HERE = Path(__file__).resolve().parent


def original_settlement(run):
    if (run.get('schema') != 'airk.arbiter.independent-numeric-reference-execution/1'
            or run.get('complete') is not True or run.get('failure', 'absent') is not None
            or run.get('post_input_drift') != [] or run.get('whole_phase_deadline_not_refreshed') is not True
            or run.get('whole_phase_expired') is not False or run.get('no_automatic_retry') is not True
            or run.get('other_processes_touched') is not False or run.get('native_airk_nn_invocations') != 0):
        raise ValueError('actual independently owned complete source execution required')
    owned = run.get('owned', {})
    required = {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
        'owned_descendants_settled': True, 'owned_job_forced_termination': False,
        'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
        'parent_assigned_to_job': False, 'child_assigned_before_resume': True,
        'owned_total_processes': 1, 'unexpected_third_process': False,
        'whole_process_deadline_not_refreshed': True}
    if any(type(owned.get(key)) is not type(value) or owned.get(key) != value for key, value in required.items()):
        raise ValueError('actual one-child custody/exit/accounting did not settle')
    if owned.get('retained_member_exits') != [{'exit_observed': True, 'exit_code': 0}]:
        raise ValueError('actual child exit observation absent')
    close = owned.get('handle_close_proof', {})
    required = {'forced': False, 'settled': True, 'final_active': 0, 'members_before': [], 'final_member_pids': []}
    if any(type(close.get(key)) is not type(value) or close.get(key) != value for key, value in required.items()):
        raise ValueError('original handle closure does not prove an empty natural Job')


def joined_rows(template, same, original):
    result = copy.deepcopy(template['rows'])
    for kind, execution, column in [('same_artifact', same, 'reference_same_artifact'), ('original_fp32', original, 'reference_original_fp32')]:
        original_settlement(execution)
        if (execution.get('reference_kind') != kind or execution.get('model') != template['model']
                or execution.get('compiler') != template['compiler'] or execution.get('resource_policy_digest') != template['resource_policy']
                or execution.get('torch_forward_invocations') != len(result) or len(execution['rows']) != len(result)):
            raise ValueError('source model/compiler/policy/kind/population changed')
        for row, source in zip(result, execution['rows']):
            if row.get(column) is not None:
                raise ValueError('native reference column was already populated')
            if source['name'] != row['name'] or source['input'] != row['input']:
                raise ValueError('complete native tensor input/identity was substituted')
            scores = source['scores']
            if len(scores) != len(row['input']['candidate_positions']) or not all(type(value) in (int, float) and math.isfinite(value) for value in scores):
                raise ValueError('complete finite original raw arrays required')
            row[column] = copy.deepcopy(scores)
    return result


def measured_like(native_pin, path):
    # Keep the serialized Rust pin shape. No Python Native DTO/default table.
    result = copy.deepcopy(native_pin)
    if 'path' not in result or 'sha256' not in result:
        raise ValueError('original native file pin shape required')
    path = worker.regular(path)
    result['path'], result['sha256'] = str(path), worker.digest(path)
    if 'bytes' in result:
        result['bytes'] = path.stat().st_size
    return result


def assemble(args):
    output_root = private_root(args.output_root)
    if not args.output.resolve().is_relative_to(output_root) or args.output.exists():
        raise ValueError("fresh output under explicit private output root required")
    native_input = worker.pinned({'path': str(args.native_input), 'sha256': args.native_input_sha256})
    template_path = worker.pinned({'path': str(args.template), 'sha256': args.template_sha256})
    roster_path = worker.pinned({'path': str(args.original_roster), 'sha256': args.original_roster_sha256})
    request, template, roster = [json.loads(path.read_bytes()) for path in [native_input, template_path, roster_path]]
    profile = json.loads(worker.pinned(request['profile']).read_bytes())
    if (template['model'] != profile['model'] or template['compiler'] != profile['compiler']
            or template['resource_policy'] != request['policy_digest'] or roster['model'] != template['model']
            or roster['compiler'] != template['compiler'] or roster['resource_policy_digest'] != template['resource_policy']
            or roster['native_input_sha256'] != args.native_input_sha256):
        raise ValueError('exact native input/template/original roster binding differs')
    kinds = {}
    pins = []
    pin_template = next(iter(roster['original_reference_files'].values()))
    source_assets = [asset for asset in profile['assets'] if asset['role'] in ('model', 'external_weights', 'tokenizer')]
    for kind, path in [('same_artifact', args.same_artifact), ('original_fp32', args.original_fp32)]:
        execution = json.loads(worker.regular(path).read_bytes())
        original_settlement(execution)
        if execution['source_assets'] != (source_assets if kind == 'same_artifact' else roster['original_reference_assets']):
            raise ValueError('actual original neural source roster differs')
        seals = execution['input_seals']
        for required_path, sha in [(str(native_input), args.native_input_sha256), (str(template_path), args.template_sha256), (str(roster_path), args.original_roster_sha256)]:
            if seals.get(required_path) != sha:
                raise ValueError('actual reference phase did not seal these native inputs')
        for source in roster['original_source_files'].values():
            if seals.get(source['path']) != source['sha256']:
                raise ValueError('original configuration/checkpoint source was not sealed')
        for source_path, sha in seals.items():
            worker.pinned({'path': source_path, 'sha256': sha})
        raw = json.loads(worker.pinned(execution['raw_output']).read_bytes())
        if raw.get('complete') is not True or raw['rows'] != execution['rows'] or raw['torch_forward_invocations'] != execution['torch_forward_invocations']:
            raise ValueError('actual raw child output differs from parent projection')
        worker.pinned(execution['stdout'])
        worker.pinned(execution['owned']['suspended_assignment']['image'])
        kinds[kind] = execution
        pins.append(measured_like(pin_template, path))
    if kinds['same_artifact']['execution_user_sid'] != kinds['original_fp32']['execution_user_sid']:
        raise ValueError('independent sources used different execution identities')
    rows = joined_rows(template, kinds['same_artifact'], kinds['original_fp32'])
    output = args.output.resolve()
    if not output.is_relative_to(output_root) or output.exists():
        raise ValueError('fresh output under this exact private preparation required')
    output.mkdir()
    producer = {'schema': 'airk.arbiter.independent-native-numeric-reference/2', 'model': template['model'],
        'compiler': template['compiler'], 'resource_policy_digest': template['resource_policy'],
        'rows': rows, 'candidate_assets': source_assets,
        'original_reference_assets': roster['original_reference_assets'],
        'original_reference_files': roster['original_reference_files'],
        'original_source_files': roster['original_source_files'],
        'reference_executions': pins, 'native_template': measured_like(pin_template, template_path),
        'original_source_roster': measured_like(pin_template, roster_path),
        'qualification': False, 'scope': 'Actual independently owned Torch raw arrays. Native validation and separate recorded source approval remain required.'}
    producer_path = output / 'reference-producer.json'
    with producer_path.open('x', encoding='utf-8') as stream:
        json.dump(producer, stream, indent=2)
        stream.write('\n')
    fixture = copy.deepcopy(template)
    fixture['rows'] = rows
    fixture['original_reference_assets'] = copy.deepcopy(roster['original_reference_assets'])
    fixture['reference_producer'] = measured_like(roster['original_source_input'], producer_path)
    fixture_path = output / 'numeric-fixture.json'
    with fixture_path.open('x', encoding='utf-8') as stream:
        json.dump(fixture, stream, indent=2)
        stream.write('\n')
    result = {'producer': measured_like(pin_template, producer_path), 'fixture': measured_like(request['profile'], fixture_path),
        'model_executed_by_assembler': False, 'qualification_granted': False}
    with (output / 'assembly.json').open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps(result))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['native-input', 'template', 'original-roster', 'same-artifact', 'original-fp32', 'output', 'output-root']:
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ['native-input', 'template', 'original-roster']:
        parser.add_argument('--' + name + '-sha256', required=True)
    assemble(parser.parse_args())


if __name__ == '__main__':
    main()
