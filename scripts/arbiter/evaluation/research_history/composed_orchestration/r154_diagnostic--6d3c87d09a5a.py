# SPDX-License-Identifier: Apache-2.0
"""Fresh one-Answer R154-question attribution diagnostic; never retries or grades."""
import argparse
import copy
import csv
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

BASE = Path(__file__).resolve().parent
SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'

def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()

def require(condition, message):
    if not condition:
        raise ValueError(message)

def pinned(path, expected):
    path = Path(path).resolve(strict=True)
    require(path.is_file() and digest(path) == expected, 'Pinned input changed: ' + str(path))
    return path

def identity():
    require(os.name == 'nt', 'Prepared Windows diagnostic only')
    raw = subprocess.check_output(['whoami', '/user', '/fo', 'csv', '/nh'], text=True)
    rows = list(csv.reader(raw.strip().splitlines()))
    require(len(rows) == 1 and rows[0][1] == SID, 'Verified human execution identity required')
    return {'account': rows[0][0], 'sid': rows[0][1]}

def write(path, value):
    with Path(path).open('x', encoding='utf-8') as output:
        json.dump(value, output, ensure_ascii=False, indent=2)
        output.write('\n')

def preparation():
    packet = json.loads((BASE / 'preparation.json').read_bytes())
    require(packet['classification'].startswith('fresh attribution-only'), 'Wrong experiment identity')
    if 'orchestrator' in packet:
        pinned(__file__, packet['orchestrator']['sha256'])
        pinned(packet['source_custody']['path'], packet['source_custody']['sha256'])
    for row in packet['historical_pins'] + packet['helpers']:
        pinned(row['original_path'], row['sha256'])
        pinned(row['copy_path'], row['sha256'])
    for row in packet['copied_source_files']:
        for field in ['original_path', 'copy_path']:
            source = pinned(row[field], row['sha256'])
            require(source.stat().st_size == row['bytes'], 'Source length changed')
    pinned(packet['node']['path'], packet['node']['sha256'])
    return packet

def sdk_closure(repository, manifest_path, expected):
    manifest_path = pinned(manifest_path, expected)
    manifest = json.loads(manifest_path.read_bytes())
    require(manifest.get('schema') == 'airk.ui-artifact-handoff/1', 'Reviewed complete human UI handoff required')
    rows = manifest['editor_outputs']
    require(len(rows) == manifest['editor_output_count'] and rows, 'Incomplete reviewed SDK closure')
    out = repository / 'packages/vscode/out'
    for row in rows:
        path = out / row['path']
        require(path.resolve().is_relative_to(out.resolve()), 'SDK path escapes compiled output')
        pinned(path, row['sha256'])
        require(path.stat().st_size == row['bytes'], 'SDK size changed')
    # Read-only native ACL observation; no trust/configuration/ACL mutation.
    command = "$v=[Console]::In.ReadToEnd()|ConvertFrom-Json;$v|ForEach-Object{[pscustomobject]@{Path=$_;Owner=(Get-Acl -LiteralPath $_).Owner}}|ConvertTo-Json -Compress"
    paths = [str(out / row['path']) for row in rows]
    observed = subprocess.run(['pwsh', '-NoProfile', '-NonInteractive', '-Command', command],
        input=json.dumps(paths), text=True, capture_output=True, check=True, timeout=30)
    owners = json.loads(observed.stdout)
    if isinstance(owners, dict): owners = [owners]
    require(len(owners) == len(paths) and all(row['Owner'].lower() == ('tyai' + chr(92) + 'tdotn') for row in owners),
        'Foreign ignored SDK artifacts cannot be adopted')
    return {'path': str(manifest_path), 'sha256': expected, 'files': len(rows)}

def execute(args, packet, account):
    require(args.phase_seconds is not None and 0 < args.phase_seconds <= 1800,
        'Explicit bounded diagnostic phase required')
    repository = Path(args.repository_root).resolve(strict=True)
    output = Path(args.output).absolute()
    require(not output.exists() and output.resolve().is_relative_to((repository / 'artifacts').resolve()),
        'Fresh artifact output required outside indexed sources')
    source_root = Path(packet['source_root']).resolve(strict=True)
    require(not output.resolve().is_relative_to(source_root), 'Output cannot change indexed sources')
    ready_path = pinned(args.fixture_ready, args.fixture_ready_sha256)
    require(output.resolve().is_relative_to(ready_path.parent),
        'Diagnostic output must inherit the actual native owner-private fixture work directory')
    bindings_path = pinned(args.index_bindings, args.index_bindings_sha256)
    policy_path = pinned(args.resource_policy_input, args.resource_policy_input_sha256)
    template_path = pinned(ready_path.parent / 'context-request.json', args.request_template_sha256)
    ready = json.loads(ready_path.read_bytes())
    require(ready['evaluation'] is True and ready['release_qualified'] is False,
        'Actual private native evaluation fixture required')
    require(ready['health']['executable_sha256'] == args.service_executable_sha256,
        'Actual native fixture executable differs from reviewed build')
    require({args.context_manifest, args.focus_manifest}.issubset({r['manifest'] for r in ready['profiles']}),
        'Selected current profiles absent from actual native fixture')
    bindings = json.loads(bindings_path.read_bytes())
    selected = [row for row in bindings['bindings'] if Path(row['source_root']).resolve() == source_root]
    require(len(selected) == 1 and len(bindings['bindings']) == 1,
        'Fresh diagnostic requires exactly the copied source index binding')
    sdk = sdk_closure(repository, args.sdk_manifest, args.sdk_manifest_sha256)
    pair = Path(args.runtime_pair).resolve(strict=True)
    build_path = pinned(pair / 'airk-build.json', args.runtime_build_sha256)
    node = Path(packet['node']['path'])
    os.environ['PATH'] = str(node.parent) + os.pathsep + os.environ.get('PATH', '')
    sys.path.insert(0, str(BASE / 'helpers'))
    from native_goal_client import NativeGoalClient, require_normal_session_close
    from native_policy import NativePolicy
    policy_pin = {'path': str(policy_path), 'sha256': args.resource_policy_input_sha256}
    policy = NativePolicy(policy_pin)
    template = json.loads(template_path.read_bytes())
    require(template['budget']['deadline_ms'] == policy.deadline_ms and
        template['budget']['input_tokens'] == policy.shared_input_tokens,
        'Rust request template and native resource policy differ')
    arguments = copy.deepcopy(template)
    arguments['question'] = packet['question']
    require(set(arguments) == set(template) and
        all(arguments[key] == template[key] for key in template if key != 'question'),
        'Only the frozen question may replace native template data')
    started = time.monotonic()
    cutoff = started + args.phase_seconds
    def remaining():
        value = cutoff - time.monotonic()
        require(value > 0, 'Original diagnostic phase deadline expired')
        return value
    report = {'schema': 'airk.private-r154-fresh-attribution/1',
        'classification': packet['classification'], 'qualification_granted': False,
        'original_call_retried': False, 'original_missing_observer_reconstructed': False,
        'question_sha256': packet['original_question_sha256'], 'source_index': selected[0]['index_id'],
        'source_root': str(source_root), 'execution_identity': account, 'sdk': sdk,
        'runtime_build': {'path': str(build_path), 'sha256': args.runtime_build_sha256},
        'request_template': {'path': str(template_path), 'sha256': args.request_template_sha256},
        'canonical_call_attempts': 0, 'neural_invocations': None, 'original_native_work_settlement': 'historical closure only',
        'fresh_service_process_settlement': 'not asserted; fixture owner closed.json and native journals required',
        'complete': False}
    output.mkdir(parents=True, exist_ok=False)
    write(output / 'execution-inputs.json', vars(args))
    write(output / 'fresh-question-arguments.json', arguments)
    client = None
    failed = None
    try:
        client = NativeGoalClient(pair, ready['control_root'], selected[0]['index_id'], output / 'canonical-call',
            fixture_ready=ready_path, index_bindings=bindings_path, repository_root=repository,
            resource_policy_input=policy_pin)
        before = client.observe_stage_status(output / 'stage-status', 'before', args.context_manifest,
            args.focus_manifest, timeout=min(remaining(), policy.deadline_seconds))
        actual = before['full_native_response']
        require(all(actual[stage]['availability'] == 'ready' for stage in ['focus', 'context']),
            'Both exact selected current stages must already be natively warm/Ready')
        require(remaining() >= policy.deadline_seconds, 'Remaining phase cannot admit one unchanged native read budget')
        # Exactly one fresh call. Native owners mint its new attempt/absolute
        # expiry. Historical request/admission identities are never submitted.
        report['canonical_call_attempts'] = 1
        receipt = client.call('answer', arguments, policy.deadline_seconds)
        report['canonical_call'] = receipt
        report['fresh_call_validated'] = receipt.get('validated') is True
        if receipt.get('timed_out'):
            report['outcome'] = 'fresh call outcome unknown; no retry'
        else:
            report['outcome'] = 'fresh full canonical receipt retained'
            report['stage_after'] = client.observe_stage_status(output / 'stage-status', 'after',
                args.context_manifest, args.focus_manifest, timeout=min(remaining(), policy.deadline_seconds))
    except BaseException as error:
        failed = error
        report['failure'] = {'type': type(error).__name__, 'message': str(error)}
    finally:
        if client is not None:
            try:
                close = client.close()
                report['session_close'] = close
                if client.session is not None: require_normal_session_close(close)
            except BaseException as error:
                report['session_close_failure'] = {'type': type(error).__name__, 'message': str(error)}
                if failed is None: failed = error
        report['elapsed_seconds'] = time.monotonic() - started
        try:
            preparation()
            pinned(ready_path, args.fixture_ready_sha256)
            pinned(bindings_path, args.index_bindings_sha256)
            pinned(policy_path, args.resource_policy_input_sha256)
            pinned(build_path, args.runtime_build_sha256)
            sdk_closure(repository, args.sdk_manifest, args.sdk_manifest_sha256)
            report['prepared_sources_and_inputs_unchanged'] = True
        except BaseException as error:
            report['post_input_failure'] = {'type': type(error).__name__, 'message': str(error)}
            if failed is None: failed = error
        # Session/helper exit is retained separately; never equate it with
        # cancellation, dispatch drain, lease release or native service exit.
        write(output / 'diagnostic.json', report)
    print(json.dumps({'output': str(output), 'fresh_call_validated': report.get('fresh_call_validated', False),
        'canonical_call_attempts': report['canonical_call_attempts'], 'qualification_granted': False}))
    if failed is not None: raise failed
    return 0 if report.get('fresh_call_validated') else 1

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute-new-diagnostic', action='store_true')
    for name in ['repository-root', 'runtime-pair', 'runtime-build-sha256', 'fixture-ready',
        'fixture-ready-sha256', 'service-executable-sha256', 'index-bindings', 'index-bindings-sha256',
        'resource-policy-input', 'resource-policy-input-sha256', 'request-template-sha256',
        'context-manifest', 'focus-manifest', 'sdk-manifest', 'sdk-manifest-sha256', 'output']:
        parser.add_argument('--' + name)
    parser.add_argument('--phase-seconds', type=float)
    args = parser.parse_args()
    account = identity()
    packet = preparation()
    if not args.execute_new_diagnostic:
        print(json.dumps({'preparation_verified': True, 'source_files': len(packet['copied_source_files']),
            'source_bytes': packet['source_bytes'], 'helpers': len(packet['helpers']),
            'native_or_model_execution': False, 'execution_identity': account}))
        return 0
    require(all(value is not None for value in vars(args).values()), 'All future execution input pins are required')
    return execute(args, packet, account)

if __name__ == '__main__':
    sys.exit(main())
