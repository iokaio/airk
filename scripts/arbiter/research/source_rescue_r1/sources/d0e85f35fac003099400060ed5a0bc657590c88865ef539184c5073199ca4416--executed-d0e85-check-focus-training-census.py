# SPDX-License-Identifier: Apache-2.0
"""One owned CPU-only native Focus instruction census; no model/gradient dispatcher.

Caller first establishes the logged-in human execution identity. The frozen
parent/child SID is rechecked before output and before resume. See owning
focus-training-census/CONTRACT.md. Execution is Windows-only; tests are pure.
"""
import argparse
import ctypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import time

sys.dont_write_bytecode = True
SCHEMA = 'airk.private.focus-compiler-census-owned-plan/1'
SELECTOR = 'arbiter::compiler::focus::experiment::tests::emission::emit_original_human_pair_instruction_census'
LIMITS = {'wall_seconds': 120, 'host_committed_bytes': 4 << 30, 'log_bytes': 64 << 20,
          'report_bytes': 512 << 20, 'cleanup_seconds': 5}
SUPPORT = {'worker': '9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7',
           'parent': 'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d',
           'ownership': '2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1',
           'visibility': '307d249868767c3c22bc6938c2024b93aada9c590a5b13c2a21165b042a30a66'}
HEX = re.compile(r'[0-9a-f]{64}\Z')
PIN_FIELDS = ('runner', 'base_python', 'worker', 'parent', 'ownership', 'visibility', 'image',
              'image_origin', 'cargo_json', 'native_build_receipt', 'native_report', 'roster', 'replay', 'tokenizer')
ENV_KEYS = {'PYTHONDONTWRITEBYTECODE', 'TEMP', 'TMP', 'TMPDIR'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def regular(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(), 'canonical regular input required')
    for entry in [path, *path.parents]:
        metadata = entry.lstat()
        require(not entry.is_symlink() and not getattr(metadata, 'st_file_attributes', 0) & 0x400, 'input alias/reparse refused')
    return path


def pin(value):
    path = regular(value['path'])
    require(digest(path) == value['sha256'], 'sealed input changed: ' + str(path))
    if 'bytes' in value:
        require(path.stat().st_size == value['bytes'], 'sealed input size changed')
    return path


def load(value, name):
    path = pin(value)
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def key(path):
    return os.path.normcase(os.path.abspath(path)).removeprefix('\\\\?\\')


def validate_plan(plan):
    require(plan.get('schema') == SCHEMA and plan.get('selector') == SELECTOR and plan.get('limits') == LIMITS,
            'only fixed CPU Focus compiler selector/120s/4GiB is supported')
    require(plan.get('original_code_rows') == 1955 and plan.get('fixed_replay_rows') == 45,
            'entire original1955+fixed45 denominator required')
    require(isinstance(plan.get('human_sid'), str) and plan['human_sid'].startswith('S-1-5-21-'), 'frozen actual human SID required')
    for name in PIN_FIELDS:
        value = plan[name]
        require(Path(value['path']).is_absolute() and isinstance(value.get('sha256'), str) and HEX.fullmatch(value['sha256']), 'absolute SHA-pinned ' + name + ' required')
        require(plan['input_seals'].get(value['path']) == value['sha256'], 'pin absent from input closure: ' + name)
    require(all(plan[k]['sha256'] == expected for k, expected in SUPPORT.items()), 'reviewed maintained ownership primitives changed')
    require(not plan.get('directory_censuses'), 'compiler adapter has no model/source directory discovery')
    require(set(plan['environment']) <= ENV_KEYS and plan['environment'].get('PYTHONDONTWRITEBYTECODE') == '1', 'unsupported child environment override')
    require(Path(plan['run_directory']).is_absolute(), 'absolute fresh private run directory required')
    require(plan['runner']['path'] == str(Path(__file__).resolve()), 'actual tracked adapter path required')


def validate_build(receipt, cargo_text, plan):
    require(receipt.get('schema') == 'airk-readiness-command/1' and receipt.get('status') == 'completed'
            and type(receipt.get('runner_exit_code')) is int and receipt['runner_exit_code'] == 0
            and receipt.get('runtime_unchanged') is True and receipt.get('changed_groups') == [],
            'actual native build wrapper schema/status/runtime evidence required')
    for name in ('passed', 'inputs_unchanged', 'runner_unchanged', 'command_inputs_unchanged'):
        require(receipt.get(name) is True, 'actual native build receipt not successful/unchanged: ' + name)
    require(type(receipt.get('exit_code')) is int and receipt['exit_code'] == 0 and not receipt.get('evidence_errors'), 'actual build failed or lost evidence')
    argv = receipt['command']
    require(isinstance(argv, list) and argv[1] == 'test' and '-p' in argv and argv[argv.index('-p') + 1] == 'airk-retrieval'
            and '--no-run' in argv and '--locked' in argv and '--timings' in argv and '--profile' in argv
            and argv[argv.index('--profile') + 1] == 'release-local' and '--message-format=json' in argv,
            'genuine optimized CPU Retrieval lib-test build required')
    expected_log = receipt['log']
    require(isinstance(expected_log, str) and key(Path(plan['native_build_receipt']['path']).parent / expected_log) == key(plan['cargo_json']['path']), 'CargoJSON is not the sealed actual build log')
    artifacts, finished = [], []
    for line in cargo_text.splitlines():
        if not line.lstrip().startswith('{'):
            continue
        record = json.loads(line)
        if record.get('reason') == 'build-finished':
            finished.append(record)
        if record.get('reason') == 'compiler-artifact' and record.get('target', {}).get('name') == 'airk_retrieval' and record.get('executable'):
            artifacts.append(record)
    require(len(finished) == 1 and finished[0].get('success') is True and len(artifacts) == 1, 'one successful actual Retrieval artifact required')
    artifact = artifacts[0]
    require(key(artifact['executable']) == key(plan['image_origin']['path']), 'Cargo artifact origin differs')
    features = set(artifact['features'])
    require(features in ({'local-inference', 'test-support'}, {'default', 'local-inference', 'test-support'}), 'GPU or unrelated native features refused')
    profile = artifact['profile']
    require(profile.get('test') is True and str(profile.get('opt_level')) == '3' and profile.get('debuginfo') == 0
            and profile.get('debug_assertions') is False and profile.get('overflow_checks') is False,
            'actual release-local opt3/debug0 CPU lib-test profile required')
    require(artifact.get('target', {}).get('kind') == ['lib'], 'Retrieval lib-test only')
    sources = receipt['groups']['native']['files']
    root = Path(plan['repository_root'])
    for relative, expected in plan['native_sources'].items():
        require(sources.get(relative) == expected and plan['input_seals'].get(str(root / relative)) == expected,
                'compiled native source pin absent from actual build receipt')
    required = {'crates/airk-retrieval/src/arbiter/compiler.rs', 'crates/airk-retrieval/src/arbiter/compiler/focus.rs',
                'crates/airk-retrieval/src/arbiter/compiler/focus/experiment.rs',
                'crates/airk-retrieval/src/arbiter/compiler/focus/experiment/tests/emission.rs'}
    require(required <= set(plan['native_sources']), 'actual compiler/Focus/experiment/emitter sources must be pinned')
    return {'features': sorted(features), 'profile': profile, 'native_group': receipt['groups']['native']['sha256']}


def validate_body(body, plan, roster, replay):
    require(body.get('schema') == 'airk.private.focus-instruction-emission/1' and body.get('model_requests_dispatched') is False
            and body.get('training_gradients') is False and body.get('qualification') is False, 'compiler-only native emission required')
    for field, source in [('native_report', 'native_report'), ('frozen_role_roster', 'roster'), ('replay_roster', 'replay'), ('tokenizer', 'tokenizer')]:
        require(body.get(field) == plan[source]['sha256'], 'native input identity differs: ' + field)
    image = body['native_image']
    require(image['sha256'] == plan['image']['sha256'] and key(image['path']) == key(plan['image']['path']), 'actual native image differs')
    for field in ('native_focus_task_policy', 'focus_compiler_source_lf_sha256'):
        require(isinstance(body.get(field), str) and HEX.fullmatch(body[field]) and body[field] != '0' * 64, 'actual native policy/source field absent')
    require(body['focus_compiler_source_lf_sha256'] == plan['focus_source_lf_sha256'], 'actual compiled Focus LF source differs')
    census = body['census']
    require(census.get('schema') == 'airk.private.focus-instruction-census/1' and census.get('qualification') is False
            and census.get('model_requests_dispatched') is False and census.get('original_denominator') == 2000
            and len(census.get('rows', [])) == 2000, 'entire native2000row denominator required')
    require(len(roster) == 1955 and len(replay) == 45, 'entire frozen role/replay inputs required')
    originals, general = {r['id']: r for r in roster}, {r['id']: r for r in replay}
    require(len(originals) == 1955 and len(general) == 45 and not set(originals) & set(general), 'duplicate frozen role/replay IDs')
    all_ids = set(originals) | set(general)
    observed, counts = set(), {'publisher': {'compiled': 0, 'refused': 0}, 'software_evidence_v1': {'compiled': 0, 'refused': 0}}
    for record in census['rows']:
        pair = record['pair']; ident = pair['id']
        require(ident not in observed and ident in all_ids, 'duplicate/foreign native pair')
        observed.add(ident)
        if ident in originals:
            r = originals[ident]
            require(pair['label'] == r['label'] and pair['role'] == r['role'] and pair['rights_scope'] == r['rights_review']
                    and pair['question_sha256'] == r['question_sha256'] and pair['document_sha256'] == r['function_sha256'], 'original human pair/role changed')
        else:
            r = general[ident]
            require(pair['label'] == 1 and pair['role'] == 'REPLAY' and pair['rights_scope'] == r['rights_review']
                    and pair['question_sha256'] == r['question_sha256'] and pair['document_sha256'] == r['window_sha256'], 'original human replay changed')
        require(hashlib.sha256(pair['question'].encode()).hexdigest() == pair['question_sha256']
                and hashlib.sha256(pair['document'].encode()).hexdigest() == pair['document_sha256'], 'original native question/window bytes changed')
        for recipe in counts:
            outcome = record[recipe]; kind = outcome['kind']
            require(kind in counts[recipe], 'unknown native compile/refusal disposition')
            counts[recipe][kind] += 1
            if kind == 'refused':
                require(bool(outcome['reason']), 'native refusal reason absent')
            else:
                value = outcome['input']
                require(len(value['input_ids']) == len(value['attention_mask']) == len(value['position_ids']) == 512
                        and value['candidate_positions'] == [2152, 9693] and value['option_mask'] == [True, True]
                        and value['decision_position'] == 511, 'actual native512 readout differs')
    require(observed == set(originals) | set(general), 'native row subset')
    return counts


def fresh_output(output, plan_path, root):
    output, plan_path, root = Path(output), Path(plan_path), Path(root)
    require(not output.exists() and output.is_absolute() and output.resolve() == output
            and output.is_relative_to(root / 'artifacts/v190/final-plan')
            and output.parent.resolve(strict=True) == output.parent and output.parent == plan_path.parent,
            'canonical fresh immediate private output required')
    for entry in [output.parent, *output.parent.parents]:
        metadata = entry.lstat()
        require(not entry.is_symlink() and not getattr(metadata, 'st_file_attributes', 0) & 0x400,
                'output parent alias/reparse refused')
    return output


def record_failure(receipt, error):
    receipt['complete'] = False
    receipt['failure'] = {'type': type(error).__name__, 'message': str(error)}
    if hasattr(error, 'owned_cleanup'):
        receipt['owned_cleanup'] = error.owned_cleanup


def file_owner(path):
    from ctypes import wintypes as w
    advapi, kernel = ctypes.WinDLL('advapi32', use_last_error=True, winmode=0x800), ctypes.WinDLL('kernel32', use_last_error=True, winmode=0x800)
    advapi.GetNamedSecurityInfoW.argtypes = [w.LPWSTR, ctypes.c_int, w.DWORD, ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
    advapi.GetNamedSecurityInfoW.restype = w.DWORD
    advapi.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(w.LPWSTR)]; advapi.ConvertSidToStringSidW.restype = w.BOOL
    kernel.LocalFree.argtypes = [ctypes.c_void_p]; kernel.LocalFree.restype = ctypes.c_void_p
    owner, descriptor, text = ctypes.c_void_p(), ctypes.c_void_p(), w.LPWSTR()
    result = advapi.GetNamedSecurityInfoW(str(path), 1, 1, ctypes.byref(owner), None, None, None, ctypes.byref(descriptor))
    if result: raise ctypes.WinError(result)
    try:
        if not advapi.ConvertSidToStringSidW(owner, ctypes.byref(text)): raise ctypes.WinError(ctypes.get_last_error())
        return text.value
    finally:
        if text: kernel.LocalFree(ctypes.cast(text, ctypes.c_void_p))
        if descriptor: kernel.LocalFree(descriptor)


def finish(receipt, started, deadline, proofs, visibility, hash_file=digest, clock=time.monotonic):
    errors = []
    def observed(path):
        try: return hash_file(path)
        except BaseException as error:
            errors.append({'type': type(error).__name__, 'message': str(error), 'path': str(path)})
            receipt['complete'] = False
            if receipt['failure'] is None:
                receipt['failure'] = {'type': 'FinalHashError', 'message': 'actual final retained hash unavailable'}
            return None
    receipt['final_hash_errors'] = errors
    receipt['elapsed_scope'] = 'Original phase through post-input checks and final report/log hashes; final execution receipt encoding/write excluded.'
    if receipt['failure'] is not None:
        receipt['complete'] = False
    visibility.finish_proofs(receipt, started, deadline, proofs, hash_file=observed, clock=clock)



def run(plan_path, expected, execute=False):
    started = time.monotonic(); deadline = started + LIMITS['wall_seconds']
    plan_path = Path(plan_path).resolve(); plan = json.loads(pin({'path': str(plan_path), 'sha256': expected}).read_bytes()); validate_plan(plan)
    plan['input_seals'][str(plan_path)] = expected
    worker = load(plan['worker'], 'reference_worker')
    require(worker.windows_sid() == plan['human_sid'], 'actual parent token differs from verified human')
    parent = load(plan['parent'], 'focus_census_native_parent'); owned = load(plan['ownership'], 'focus_census_owned'); visibility = load(plan['visibility'], 'focus_census_visibility_primitives')
    require('torch' not in sys.modules, 'compiler-only parent must not import Torch')
    visibility.seal_inputs(plan, deadline)
    build = validate_build(json.loads(pin(plan['native_build_receipt']).read_bytes()), pin(plan['cargo_json']).read_text(encoding='utf-8'), plan)
    focus_source = (Path(plan['repository_root']) / 'crates/airk-retrieval/src/arbiter/compiler/focus.rs').read_bytes().replace(b'\r\n', b'\n')
    require(b'\r' not in focus_source and hashlib.sha256(focus_source).hexdigest() == plan['focus_source_lf_sha256'], 'frozen current Focus LF source differs')
    image, origin = pin(plan['image']), pin(plan['image_origin']); a, b = image.stat(), origin.stat()
    require(plan['image']['sha256'] == plan['image_origin']['sha256'] and (a.st_dev, a.st_ino) != (b.st_dev, b.st_ino)
            and a.st_nlink == b.st_nlink == 1, 'fresh independent image copy required')
    require(file_owner(image) == file_owner(origin) == plan['human_sid'], 'actual native images must remain human-owned')
    if not execute:
        return {'plan_valid': True, 'native_execution': False, 'build': build}
    require(sys.platform == 'win32', 'actual bounded census requires the verified Windows host')
    root = Path(plan['repository_root']).resolve()
    output = fresh_output(plan['run_directory'], plan_path, root)
    require(file_owner(output.parent) == plan['human_sid'], 'fresh human-owned private artifact run directory required')
    output.mkdir(); log = output / 'stdout-stderr.log'; stdin = output / 'stdin.bin'; report = output / 'native-census.json'
    argv = [str(image), '--ignored', '--exact', SELECTOR, '--nocapture', '--test-threads=1']
    environment = dict(os.environ); environment.update(plan['environment'])
    for variable, name in [('NATIVE_REPORT', 'native_report'), ('ROSTER', 'roster'), ('REPLAY', 'replay'), ('TOKENIZER', 'tokenizer')]:
        environment['AIRK_F2_' + variable] = plan[name]['path']; environment['AIRK_F2_' + variable + '_SHA256'] = plan[name]['sha256']
    environment['AIRK_F2_CENSUS_OUTPUT'] = str(report)
    receipt = {'schema': 'airk.private.focus-compiler-census-owned-execution/1', 'complete': False, 'failure': None,
               'plan': {'path': str(plan_path), 'sha256': expected}, 'command': argv, 'execution_sid': plan['human_sid'],
               'limits': LIMITS, 'whole_phase_deadline_not_refreshed': True, 'whole_phase_expired': False,
               'native_image': plan['image'], 'native_build_receipt': plan['native_build_receipt'], 'build': build,
               'no_automatic_retry': True, 'model_requests_dispatched': False, 'forward_count_observation': None,
               'training_gradients': False, 'qualification_granted': False, 'post_input_drift': []}
    visibility.write(output / 'started.json', receipt)
    def before_resume(api, job, info, child_deadline):
        return parent.suspended_authentication(api, info, deadline, child_deadline, plan['human_sid'], plan['image'], owned.image_proof)
    def after_resume(api, job, info, child_deadline):
        require(child_deadline <= deadline, 'original deadline renewed')
        return visibility.monitor(api, info, deadline, report, log)
    try:
        visibility.check_deadline(deadline)
        with stdin.open('xb'): pass
        with stdin.open('rb') as initial, log.open('xb') as stream:
            result = owned.run_windows_owned_training(argv, root, stream, max(0, deadline - time.monotonic()), environment, initial,
                    LIMITS['host_committed_bytes'], before_resume=before_resume, after_resume=after_resume,
                    cleanup_seconds=LIMITS['cleanup_seconds'], absolute_deadline=deadline)
        receipt['owned'] = result
        require(parent.settled(result), 'actual child/HANDLE/Job/handle-close proof did not naturally settle')
        require(0 < result['peak_host_committed_bytes'] <= LIMITS['host_committed_bytes'], 'actual committed cap exceeded')
        require(report.stat().st_size <= LIMITS['report_bytes'] and log.stat().st_size <= LIMITS['log_bytes'], 'actual output cap exceeded')
        text = log.read_text(encoding='utf-8', errors='replace')
        require(re.search(r'(?m)^running 1 test\s*$', text) and re.search(r'test result: ok\. 1 passed; 0 failed;', text), 'actual selector did not pass exactly1test')
        body = json.loads(report.read_bytes()); receipt['counts'] = validate_body(body, plan, json.loads(pin(plan['roster']).read_bytes()), json.loads(pin(plan['replay']).read_bytes()))
        receipt['native_focus_task_policy'] = body['native_focus_task_policy']; receipt['focus_compiler_source_lf_sha256'] = body['focus_compiler_source_lf_sha256']
        receipt['original1955_publisher_parity'] = 'Authenticated native emitter assertions passed; no old policy/input identity retagged.'
        receipt['complete'] = True
    except BaseException as error:
        record_failure(receipt, error)
    finally:
        try: visibility.seal_inputs(plan, deadline)
        except BaseException as error:
            receipt['complete'] = False; receipt['post_input_drift'].append({'type': type(error).__name__, 'message': str(error)})
            if receipt['failure'] is None: receipt['failure'] = {'type': 'InputClosureRefusal', 'message': str(error)}
        finish(receipt, started, deadline, [('native_report', report), ('log', log)], visibility)
        visibility.write(output / 'execution.json', receipt)
    return {'output': str(output), 'complete': receipt['complete'], 'failure': receipt['failure'], 'qualification_granted': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256', required=True); parser.add_argument('--execute', action='store_true'); args = parser.parse_args()
    result = run(args.plan, args.plan_sha256, args.execute); print(json.dumps(result))
    return 0 if result.get('complete', result.get('plan_valid', False)) else 1


if __name__ == '__main__':
    raise SystemExit(main())
