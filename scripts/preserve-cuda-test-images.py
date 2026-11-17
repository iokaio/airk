# SPDX-License-Identifier: Apache-2.0
"""Preserve authenticated CUDA lib-test images additively; never execute them."""
import argparse
import datetime
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
USER_ROOT = Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z')
ARTIFACT_ROOT = ROOT / 'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z/current-orchestration'
SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
ENVIRONMENT = USER_ROOT / 'gpu-checkpointed-static-environment-r1.json'
ENVIRONMENT_SHA = 'a348c538e349e8311841654ddce2365c229fb5972670f0b33cd0c71a3c882ef2'
ARCHIVE = USER_ROOT / 'cu8/gpu-aggregate/onnxruntime.lib'
ARCHIVE_SHA = '042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00'
MANIFEST_SHA = 'af6d1ec57f9e93e9d92b23024d5b7de76576af182f152769f78c288441480d45'
CARGO_ARGUMENTS = ['test', '-p', 'airk-retrieval', '-p', 'airk-inference', '--lib', '--all-features', '--locked', '--offline', '--no-run', '--message-format=json', '--timings']
INCREMENTAL = 'profile default; environment unset'
GUARD = ROOT / 'scripts/dev/assert-human-execution.ps1'
POWERSHELL = 'C:/Program Files/PowerShell/7/pwsh.exe'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def regular(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path, 'canonical absolute input required')
    for item in [path, *path.parents]:
        require(not item.is_symlink() and not getattr(item.lstat(), 'st_file_attributes', 0) & 0x400, 'alias/reparse input refused')
    require(path.is_file() and path.stat().st_nlink == 1, 'single-link regular input required')
    return path


def pin(path):
    path = regular(path)
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': digest(path)}


def exact_pin(path, sha):
    value = pin(path)
    require(value['sha256'] == sha, 'caller-pinned bytes changed: ' + str(path))
    return value


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


def shell(code):
    return subprocess.check_output([POWERSHELL, '-NoProfile', '-Command', "$ErrorActionPreference='Stop'; " + code], text=True)


def guard(path, root, guard_sha, created=False):
    exact_pin(GUARD, guard_sha)
    function = 'Assert-AirkCreatedOutput' if created else 'Assert-AirkOutputParent'
    shell('. ' + quote(GUARD) + '; ' + function + ' -Path ' + quote(path) + ' -Root ' + quote(root) + ' -ExpectedSid ' + quote(SID) + ' | Out-Null')


def policy(path):
    return json.loads(shell('$a=Get-Acl -LiteralPath ' + quote(path) + '; @{owner_sid=$a.GetOwner([Security.Principal.SecurityIdentifier]).Value;sddl=$a.Sddl;rules=@($a.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|ForEach-Object{@{sid=$_.IdentityReference.Value;rights=[int64]$_.FileSystemRights;type=$_.AccessControlType.ToString()}})} | ConvertTo-Json -Depth 5 -Compress'))


def validate_policy_batch(value, paths, elapsed):
    require(0 <= elapsed < 10, 'runtime policy batch budget expired')
    require(value.get('schema') == 'airk.runtime-policy-batch/1', 'runtime policy batch schema differs')
    audit = value['audit']
    require(audit.get('schema') == 'airk.human-output-audit/1' and audit.get('expired') is False
            and 0 <= audit['elapsed_seconds'] < 10, 'runtime policy audit expired or invalid')
    policies = value['policies']
    require(len(audit['results']) == len(policies) == len(paths), 'partial runtime policy batch refused')
    for index, (path, result, item) in enumerate(zip(paths, audit['results'], policies)):
        require(type(result.get('index')) is int and result['index'] == index
                and result.get('passed') is True and result.get('error') is None,
                'runtime policy path audit failed: ' + str(result))
        require(type(item.get('index')) is int and item['index'] == index
                and item.get('path') == str(path), 'runtime policy order/path drift refused')
        policy_value = item['policy']
        require(isinstance(policy_value.get('sddl'), str) and isinstance(policy_value.get('rules'), list),
                'runtime policy metadata incomplete')
        for rule in policy_value['rules']:
            require(isinstance(rule.get('sid'), str) and type(rule.get('rights')) is int
                    and rule.get('type') in ['Allow', 'Deny'], 'runtime policy ACE metadata invalid')
        owner_policy(policy_value, payload=True)
    return [item['policy'] for item in policies]


def runtime_policies(paths, guard_sha):
    require(0 < len(paths) <= 128 and len({str(p).casefold() for p in paths}) == len(paths),
            'bounded unique runtime policy paths required')
    exact_pin(GUARD, guard_sha)
    entries = [{'path': str(p), 'root': str(USER_ROOT), 'created': True} for p in paths]
    code = """
$clock = [Diagnostics.Stopwatch]::StartNew()
. GUARD_LITERAL
$entries = @(ConvertFrom-Json -InputObject ENTRIES_LITERAL)
$identity = Assert-AirkHumanExecution -ExpectedSid SID_LITERAL
$audit = Assert-AirkOutputBatch -Entries $entries -ExpectedSid SID_LITERAL -BudgetSeconds (10 - $clock.Elapsed.TotalSeconds)
if ($audit.expired -or @($audit.results | Where-Object { -not $_.passed }).Count) {
    throw ('Runtime path audit refused: ' + ($audit | ConvertTo-Json -Depth 8 -Compress))
}
$policies = @(for ($index=0; $index -lt $entries.Count; $index++) {
    if ($clock.Elapsed.TotalSeconds -ge 10) { throw 'Runtime policy batch expired' }
    Assert-AirkCurrentToken $identity
    $path = Get-AirkCheckedPath -Path $entries[$index].path -Root $entries[$index].root
    $a = Get-Acl -LiteralPath $path -ErrorAction Stop
    $policy = @{owner_sid=$a.GetOwner([Security.Principal.SecurityIdentifier]).Value;sddl=$a.Sddl;rules=@($a.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|ForEach-Object{@{sid=$_.IdentityReference.Value;rights=[int64]$_.FileSystemRights;type=$_.AccessControlType.ToString()}})}
    if ($clock.Elapsed.TotalSeconds -ge 10) { throw 'Runtime policy batch expired' }
    @{index=$index;path=$path;policy=$policy}
})
@{schema='airk.runtime-policy-batch/1';audit=$audit;policies=$policies} | ConvertTo-Json -Depth 10 -Compress
"""
    code = code.replace('GUARD_LITERAL', quote(GUARD)).replace('ENTRIES_LITERAL', quote(json.dumps(entries))).replace('SID_LITERAL', quote(SID))
    started = time.monotonic()
    output = subprocess.check_output([POWERSHELL, '-NoProfile', '-Command',
                                      "$ErrorActionPreference='Stop'; " + code], text=True, timeout=10)
    elapsed = time.monotonic() - started
    require(len(output) <= 16 << 20, 'runtime policy metadata response too large')
    return validate_policy_batch(json.loads(output), paths, elapsed)


def owner_policy(value, payload=False):
    trusted = {'S-1-5-18', 'S-1-5-32-544', 'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464'} if payload else set()
    require(value['owner_sid'] == SID, 'human owner required')
    require(not any(r['type'] == 'Allow' and r['sid'] != SID and r['sid'] not in trusted and r['rights'] & 0x500d0156 for r in value['rules']), 'foreign writable output refused')


def identity(path):
    item = Path(path).stat()
    return {'volume': item.st_dev, 'file_id': item.st_ino, 'links': item.st_nlink, 'bytes': item.st_size, 'mtime_ns': item.st_mtime_ns}


def snapshot(path):
    result = {'pin': pin(path), 'identity': identity(path), 'policy': policy(path)}
    require(result['policy']['owner_sid'] == SID, 'foreign input owner refused')
    return result


def pe(path):
    with Path(path).open('rb') as stream:
        head = stream.read(64)
        require(len(head) == 64 and head[:2] == b'MZ', 'PE DOS header required')
        offset = int.from_bytes(head[60:64], 'little')
        require(64 <= offset <= 1 << 20, 'bounded PE offset required')
        stream.seek(offset)
        head = stream.read(26)
    require(len(head) == 26 and head[:4] == b'PE\0\0' and int.from_bytes(head[4:6], 'little') == 0x8664 and int.from_bytes(head[24:26], 'little') == 0x20b and not int.from_bytes(head[22:24], 'little') & 0x2000, 'PE64 x64 executable required')
    return {'machine_u16': 0x8664, 'optional_header_magic_u16': 0x20b}


def validate_receipt(value, native_group, log_name):
    require(value.get('schema') == 'airk-readiness-command/1' and value.get('status') == 'completed' and value.get('passed') is True and value.get('exit_code') == value.get('runner_exit_code') == 0, 'successful completed Cargo receipt required')
    require(all(value.get(k) is True for k in ['inputs_unchanged', 'runner_unchanged', 'runtime_unchanged', 'command_inputs_unchanged']) and value.get('changed_groups') == [], 'unchanged actual build required')
    require(value['groups']['native']['sha256'] == native_group and value['incremental'] == INCREMENTAL, 'native source/incremental disclosure differs')
    require(value['log'] == log_name and value['command'][1:] == CARGO_ARGUMENTS, 'exact offline no-run command required')


def cargo_rows(text):
    rows = [json.loads(line) for line in text.splitlines() if line.lstrip().startswith('{')]
    finish = [r for r in rows if r.get('reason') == 'build-finished']
    require(len(finish) == 1 and finish[0].get('success') is True, 'one actual successful Cargo finish required')
    ort = [r for r in rows if r.get('reason') == 'build-script-executed' and '#ort-sys@' in r.get('package_id', '')]
    require(len(ort) == 1 and ort[0]['package_id'].endswith('#ort-sys@2.0.0-rc.10') and ort[0]['linked_libs'] == ['static=onnxruntime'] and ort[0]['linked_paths'] == ['native=' + str(ARCHIVE.parent)], 'exact CUDA static link required')
    selected = []
    for package in ['airk-inference', 'airk-retrieval']:
        matches = [r for r in rows if r.get('reason') == 'compiler-artifact' and r.get('profile', {}).get('test') is True and r.get('executable') and r.get('target', {}).get('kind') == ['lib'] and r['target'].get('name') == package.replace('-', '_') and Path(r.get('manifest_path', '')).resolve() == (ROOT/'crates'/package/'Cargo.toml').resolve()]
        require(len(matches) == 1 and 'gpu-cuda' in matches[0].get('features', []), 'one GPU lib-test artifact required: ' + package)
        selected.append((package, matches[0]))
    return ort, selected


def census_digest(rows):
    return hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()


def runtime_census(root, guard_sha):
    root = Path(root)
    require(root.is_absolute() and root.resolve(strict=True) == root and root.is_dir(), 'canonical source runtime required')
    paths = sorted(root.iterdir(), key=lambda p: p.name)
    require(0 < len(paths) <= 128 and len({p.name.casefold() for p in paths}) == len(paths), 'bounded collision-free direct runtime census required')
    rows = [{'name': path.name, 'pin': pin(path), 'identity': identity(path)} for path in paths]
    policies = runtime_policies(paths, guard_sha)
    for path, row, policy_value in zip(paths, rows, policies):
        regular(path)
        require(identity(path) == row['identity'], 'runtime stat/file ID changed during policy batch')
        owner_policy(policy_value, payload=True)
        row['policy'] = policy_value
    require(sorted(root.iterdir(), key=lambda p: p.name) == paths, 'runtime membership changed during policy batch')
    return rows


def validate_runtime(rows):
    manifests = [r for r in rows if r['name'] == 'airk-cuda-runtime.json']
    require(len(manifests) == 1 and manifests[0]['pin']['sha256'] == MANIFEST_SHA, 'original native af6 runtime manifest required')
    require(any(r['name'] == 'gpu-checkpointed-build-receipt.json' for r in rows), 'original static producer receipt required')
    require(any(r['name'].startswith('airk-cuda-notice-') for r in rows), 'original legal roster required')
    require(any(r['name'].lower().endswith('.dll') for r in rows), 'original provider/dependency roster required')


def validate_destinations(output, runtime, inference_name):
    require(output.parent == ARTIFACT_ROOT and not output.exists(), 'fresh direct approved inventory child required')
    require(runtime.is_absolute() and runtime.resolve(strict=True) == runtime and runtime.is_dir() and runtime.is_relative_to(USER_ROOT), 'existing canonical approved runtime required')
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*\.exe', inference_name) is not None and Path(inference_name).name == inference_name, 'direct executable leaf required')
    require(not (runtime/inference_name).exists(), 'new create-only Inference filename required')
    require(not output.is_relative_to(runtime) and not runtime.is_relative_to(output), 'output/runtime overlap refused')


def unchanged_runtime(before, after, addition=None):
    expected = list(before) + ([addition] if addition else [])
    require(sorted(after, key=lambda r:r['name']) == sorted(expected, key=lambda r:r['name']), 'preexisting runtime bytes/stat/ACL or directory membership changed')

def make_directory(path, root, guard_sha, payload=False):
    guard(path, root, guard_sha)
    path.mkdir()
    guard(path, root, guard_sha, created=True)
    owner_policy(policy(path), payload)


def write_json(path, value, root, guard_sha):
    guard(path, root, guard_sha)
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
    guard(path, root, guard_sha, created=True)
    owner_policy(policy(path))


def copy_one(before, destination, root, guard_sha, payload=False):
    source = Path(before['pin']['path'])
    require(snapshot(source) == before, 'source changed before copy')
    guard(destination, root, guard_sha)
    with source.open('rb') as src, destination.open('xb') as dst:
        shutil.copyfileobj(src, dst, 1 << 20)
    guard(destination, root, guard_sha, created=True)
    after = snapshot(destination)
    owner_policy(after['policy'], payload)
    require((after['pin']['bytes'], after['pin']['sha256']) == (before['pin']['bytes'], before['pin']['sha256']), 'copied bytes differ')
    require((after['identity']['volume'], after['identity']['file_id']) != (before['identity']['volume'], before['identity']['file_id']), 'independent destination file ID required')
    require(snapshot(source) == before, 'source bytes/stat/owner/ACL changed')
    return after


def run(args):
    helper_pin = exact_pin(Path(__file__).resolve(), args.approved_script_sha256)
    guard_pin = exact_pin(GUARD, args.guard_sha256)
    shell('. ' + quote(GUARD) + '; Assert-AirkHumanExecution -ExpectedSid ' + quote(SID) + ' | Out-Null')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(head == args.source_commit, 'reviewed source commit differs')
    validate_destinations(args.output, args.runtime, args.inference_name)
    guard(args.output, ARTIFACT_ROOT, args.guard_sha256)
    guard(args.runtime/args.inference_name, USER_ROOT, args.guard_sha256)
    parents = {str(p): policy(p) for p in [ARTIFACT_ROOT, USER_ROOT]}
    owner_policy(parents[str(ARTIFACT_ROOT)])
    owner_policy(parents[str(USER_ROOT)], payload=True)
    receipt_pin = exact_pin(args.receipt, args.receipt_sha256)
    log_pin = exact_pin(args.log, args.log_sha256)
    env_pin = exact_pin(ENVIRONMENT, ENVIRONMENT_SHA)
    archive_pin = exact_pin(ARCHIVE, ARCHIVE_SHA)
    require(archive_pin['bytes'] == 899807276, 'static archive size differs')
    receipt = json.loads(args.receipt.read_bytes())
    validate_receipt(receipt, args.native_group, args.log.name)
    env = json.loads(ENVIRONMENT.read_bytes())
    require(env['schema'] == 'airk.cuda-static-environment/4' and env['activation_pins_available'] is False and env['aggregate']['output']['sha256'] == ARCHIVE_SHA and env['environment'] == {'LIBONNXRUNTIME_NO_PKG_CONFIG':'1','ORT_SKIP_DOWNLOAD':'1','ORT_PREFER_DYNAMIC_LINK':'0','ORT_LIB_LOCATION':str(ARCHIVE.parent),'ORT_LIB_PROFILE':'Release'}, 'original static environment differs')
    ort, selected = cargo_rows(args.log.read_text(encoding='utf-8'))
    originals = {}
    for package, row in selected:
        source = regular(row['executable'])
        require(source.is_relative_to(Path(receipt['target_directory']).resolve()) and 0 < source.stat().st_size <= 4 << 30, 'image outside actual target/size bounds')
        pe(source)
        originals[package] = snapshot(source)
    runtime = runtime_census(args.runtime, args.guard_sha256)
    require(census_digest(runtime) == args.runtime_census_sha256, 'reviewed full runtime census differs')
    runtime_policy = policy(args.runtime)
    owner_policy(runtime_policy, payload=True)
    validate_runtime(runtime)
    context = None
    require(bool(args.compile_input_context) == bool(args.compile_input_context_sha256), 'compile-context path/hash pair required')
    if args.compile_input_context:
        context = exact_pin(args.compile_input_context, args.compile_input_context_sha256)
        require(context['bytes'] <= 16 << 20, 'bounded compile context required')
    pins = [helper_pin, guard_pin, receipt_pin, log_pin, env_pin, archive_pin] + ([context] if context else [])
    def reopen(addition=None):
        require(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head, 'HEAD drift')
        for value in pins:
            require(pin(value['path']) == value, 'build/static/context pin drift')
        for relative, expected in receipt['groups']['native']['files'].items():
            exact_pin(ROOT/relative, expected)
        unchanged_runtime(runtime, runtime_census(args.runtime, args.guard_sha256), addition)
        require(policy(args.runtime) == runtime_policy, 'runtime directory policy drift')
        for package, value in originals.items():
            require(snapshot(value['pin']['path']) == value, 'original native image drift: ' + package)
        require(all(policy(p) == value for p, value in parents.items()), 'output parent policy drift')
    reopen()
    make_directory(args.output, ARTIFACT_ROOT, args.guard_sha256)
    outcome = {'status':'RUNNING','native_group':args.native_group,'source_commit':head,'native_model_request_dispatched':False}
    try:
        images = args.output/'images'
        make_directory(images, ARTIFACT_ROOT, args.guard_sha256)
        result = []
        bindings = []
        addition = None
        for package, row in selected:
            original = originals[package]
            destination = args.runtime/args.inference_name if package == 'airk-inference' else images/Path(row['executable']).name
            after = copy_one(original, destination, USER_ROOT if package == 'airk-inference' else ARTIFACT_ROOT, args.guard_sha256, payload=package == 'airk-inference')
            if package == 'airk-inference':
                addition = {'name':destination.name, **after}
            machine = pe(destination)
            image = {'binary_id':package, **after['pin'], 'original_path':row['executable'], 'original_file_owner_sid':SID,'preserved_file_owner_sid':SID}
            result.append(image)
            bindings.append({'materialized_image':image,'actual_cargo_artifact':row,'pe':machine,'original':original,'preserved':after})
        reopen(addition)
        inventory = {'schema':'airk.preserved-native-test-images/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_commit':head,'native_group':args.native_group,'actual_build_receipt':receipt_pin,'images':result,'source_dirty':bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=ROOT,text=True)),'scope':'Independent CUDA lib-test copies, one additive Inference filename; all preexisting runtime objects unchanged. No execution or qualification.'}
        inventory_path = args.output/'native-test-images.json'
        write_json(inventory_path, inventory, ARTIFACT_ROOT, args.guard_sha256)
        write_json(args.output/'cargo-image-bindings.json', {'schema':'airk.private-current-image-preservation-binding/1','inventory':pin(inventory_path),'build_receipt':receipt_pin,'cargo_json':log_pin,'actual_static_ORT_rows':ort,'cuda_static_environment':env_pin,'actual_static_archive':archive_pin,'caller_compile_input_context':context,'incremental':receipt['incremental'],'runtime_source_census':runtime,'runtime_source_census_sha256':args.runtime_census_sha256,'runtime_directory':str(args.runtime),'runtime_addition':addition,'preexisting_runtime_objects_unchanged':True,'images':bindings,'preserver_source':pin(Path(__file__).resolve()),'guard_source':pin(GUARD),'native_model_request_dispatched':False,'qualification':False}, ARTIFACT_ROOT, args.guard_sha256)
        outcome.update(status='PASS_IMAGE_PRESERVATION_ONLY_UNQUALIFIED', inventory=pin(inventory_path))
    except BaseException as error:
        outcome.update(status='FAILED_PRESERVED_NO_RETRY', error=type(error).__name__+': '+str(error))
        raise
    finally:
        write_json(args.output/'preservation-result.json', outcome, ARTIFACT_ROOT, args.guard_sha256)
    print(json.dumps(outcome))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['receipt','log','output','runtime']:
        parser.add_argument('--'+name, type=Path, required=True)
    for name in ['approved-script-sha256','guard-sha256','receipt-sha256','log-sha256','native-group','source-commit','runtime-census-sha256']:
        parser.add_argument('--'+name, required=True)
    parser.add_argument('--inference-name', required=True)
    parser.add_argument('--compile-input-context', type=Path)
    parser.add_argument('--compile-input-context-sha256')
    run(parser.parse_args())


if __name__ == '__main__':
    main()