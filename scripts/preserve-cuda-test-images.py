# SPDX-License-Identifier: Apache-2.0
"""Preserve authenticated CUDA lib-test images additively; never execute them."""
import argparse
import datetime
import hashlib
import importlib.util
import struct
import sys
sys.dont_write_bytecode = True
import json
import math
import os
import re
from pathlib import Path
import shutil
import subprocess
import time
import types

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


def check_deadline(deadline):
    if deadline is not None:
        require(time.monotonic() < deadline, 'original graph provenance deadline expired')


def digest(path, *, deadline=None):
    check_deadline(deadline)
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        while True:
            check_deadline(deadline)
            block = stream.read(4 << 20)
            check_deadline(deadline)
            if not block:
                break
            value.update(block)
    return value.hexdigest()


def regular(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path, 'canonical absolute input required')
    for item in [path, *path.parents]:
        require(not item.is_symlink() and not getattr(item.lstat(), 'st_file_attributes', 0) & 0x400, 'alias/reparse input refused')
    require(path.is_file() and path.stat().st_nlink == 1, 'single-link regular input required')
    return path


def pin(path, *, deadline=None):
    check_deadline(deadline)
    path = regular(path)
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': digest(path,deadline=deadline)}


def exact_pin(path, sha, *, deadline=None):
    value = pin(path,**({} if deadline is None else {'deadline':deadline}))
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


def cargo_rows(text, archive=ARCHIVE):
    rows = [json.loads(line) for line in text.splitlines() if line.lstrip().startswith('{')]
    finish = [r for r in rows if r.get('reason') == 'build-finished']
    require(len(finish) == 1 and finish[0].get('success') is True, 'one actual successful Cargo finish required')
    ort = [r for r in rows if r.get('reason') == 'build-script-executed' and '#ort-sys@' in r.get('package_id', '')]
    require(len(ort) == 1 and ort[0]['package_id'].endswith('#ort-sys@2.0.0-rc.10') and ort[0]['linked_libs'] == ['static=onnxruntime'] and ort[0]['linked_paths'] == ['native=' + str(Path(archive).parent)], 'exact CUDA static link required')
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


PROVENANCE_OWNERS = (
    'scripts/cuda-runtime-environment.py', 'scripts/cuda-static-runtime.py',
    'scripts/cuda-static-aggregate.py', 'scripts/cuda-static-incremental.py',
    'scripts/cuda-static-archive.py', 'scripts/cuda-static-checkpointed.py',
    'scripts/cuda-static-continuation-r12.py',
    'scripts/cuda-static-runtime/verification/checkpoint_sections.py',
    'scripts/cuda-static-runtime/verification/materialized.py',
    'scripts/cuda-component-execution.py', 'scripts/provision-static-onnx-runtime.py',
    'scripts/static-onnx-runtime-lock.json',
)
GRAPH_EXPORT = 'airk_ort_collect_final_graph_metadata_v1'
OLD_PRODUCER = 'f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178'


def source_roster(path, sha, *, deadline=None):
    check_deadline(deadline)
    kw = {} if deadline is None else {'deadline':deadline}
    roster_pin = exact_pin(path, sha,**kw)
    require(roster_pin['bytes'] <= 1 << 20, 'bounded provenance source roster required')
    rows = json.loads(Path(path).read_bytes())
    require(isinstance(rows, dict) and set(rows) == set(PROVENANCE_OWNERS), 'exact reviewed provenance source closure required')
    check_deadline(deadline)
    return roster_pin, [exact_pin(ROOT / key, rows[key],**kw) for key in PROVENANCE_OWNERS]


def verify_environment(path, sha, roster_path, roster_sha, *, deadline=None):
    check_deadline(deadline)
    kw = {} if deadline is None else {'deadline':deadline}
    roster_pin, sources = source_roster(roster_path, roster_sha,**kw)
    env_pin = exact_pin(path, sha,**kw)
    require(Path(path).is_relative_to(USER_ROOT), 'incremental environment outside approved human root')
    require(env_pin['bytes'] <= 16 << 20, 'bounded incremental environment required')
    env = json.loads(Path(path).read_bytes())
    require(env.get('schema') == 'airk.cuda-static-environment/5', 'explicit incremental /5 environment required')
    owner_path = ROOT / 'scripts/cuda-runtime-environment.py'
    spec = importlib.util.spec_from_file_location('preservation_incremental_environment', owner_path)
    owner = importlib.util.module_from_spec(spec); spec.loader.exec_module(owner)
    check_deadline(deadline)
    verified = owner.verify_handoff(path, sha, ROOT,**kw)
    check_deadline(deadline)
    proof = verified['gpu_build']; aggregate = verified['gpu_aggregate']
    require(proof.get('schema') == 'airk.cuda-static-build-proof/3' and proof == env['build']
            and aggregate == env['aggregate'] and verified.get('activation_pins_available') is False,
            'dedicated incremental producer verification differs')
    archive = proof['new_aggregate']; producer = proof['build_receipt']
    require(pin(archive['path'],**kw) == archive and pin(producer['path'],**kw) == producer
            and producer['sha256'] != OLD_PRODUCER and archive['sha256'] != ARCHIVE_SHA,
            'fresh actual producer/archive required')
    require(verified['environment'] == {'LIBONNXRUNTIME_NO_PKG_CONFIG':'1','ORT_SKIP_DOWNLOAD':'1',
            'ORT_PREFER_DYNAMIC_LINK':'0','ORT_LIB_LOCATION':str(Path(archive['path']).parent),'ORT_LIB_PROFILE':'Release'},
            'actual incremental archive environment differs')
    require(source_roster(roster_path, roster_sha,**kw) == (roster_pin, sources), 'provenance sources changed during verification')
    return {'environment':env_pin,'source_roster':roster_pin,'source_pins':sources,
            'proof':proof,'aggregate':aggregate,'archive':archive,'producer':producer}


def validate_graph_manifest(manifest, manifest_pin, runtime, provenance, *, deadline=None):
    check_deadline(deadline)
    require(manifest.get('schema') == 'airk.native.cuda-runtime-manifest/1'
            and manifest.get('ort_version') == '1.30.0', 'native graph runtime manifest required')
    producer = provenance['producer']
    name = manifest.get('producer_name')
    require(isinstance(name, str) and Path(name).name == name and name not in ['', '.', '..'], 'direct native producer leaf required')
    require(manifest.get('static_core_build') == manifest.get('producer_sha256') == producer['sha256']
            and manifest.get('producer_bytes') == producer['bytes'] and producer['sha256'] != OLD_PRODUCER,
            'native manifest/fresh producer binding differs')
    require(Path(manifest_pin['path']) == Path(runtime) / 'airk-cuda-runtime.json', 'reviewed manifest outside runtime')
    actual = pin(Path(runtime) / name,**({} if deadline is None else {'deadline':deadline}))
    require((actual['bytes'],actual['sha256']) == (producer['bytes'],producer['sha256']), 'runtime producer bytes differ')
    return actual


FRESH_BINDING_SCHEMA = 'airk.private.cuda-fresh-finalgraph-image-binding/1'
FRESH_ENV_SCHEMA = 'airk.cuda-static-environment/6'
FRESH_BUILD_SCHEMA = 'airk.private.cuda-fresh-static-build-proof/1'
FRESH_PROVENANCE_OWNERS = (
    'scripts/cuda-runtime-environment.py', 'scripts/cuda-static-runtime.py',
    'scripts/cuda-static-incremental.py', 'scripts/cuda-static-archive.py',
    'scripts/cuda-static-runtime/build/fresh_verify.py',
    'scripts/cuda-static-runtime/build/fresh_projection.py',
    'scripts/cuda-static-runtime/build/fresh_aggregate.py',
    'scripts/cuda-static-runtime/build/fresh_compile.py',
    'scripts/cuda-static-runtime/build/fresh_configure.py',
    'scripts/cuda-static-runtime/build/fresh_source.py',
    'scripts/cuda-static-runtime/build/fresh_process.py',
    'scripts/cuda-static-runtime/build/r8/provision_cuda_runtime.py',
    'scripts/cuda-static-runtime/build/r8/cuda-runtime-producer-lock.json',
    'scripts/cuda-static-runtime/build/r12/wait_process_tree.ps1',
    'scripts/cuda-component-execution.py', 'scripts/cuda-native-execution.py',
    'scripts/provision-static-onnx-runtime.py', 'scripts/static-onnx-runtime-lock.json',
    'scripts/dev/assert-human-execution.ps1',
)
FRESH_PROVIDERS = ('onnxruntime_providers_shared.dll', 'onnxruntime_providers_cuda.dll')


def fresh_deadline(deadline):
    began = time.monotonic()
    require(type(deadline) in (int, float) and math.isfinite(deadline)
            and began < deadline <= began + 600, 'explicit original fresh provenance deadline required')


def fresh_bytes(row, maximum, deadline):
    check_deadline(deadline)
    require(isinstance(row, dict) and set(row) == {'path', 'bytes', 'sha256'}
            and isinstance(row['path'], str) and type(row['bytes']) is int
            and 0 < row['bytes'] <= maximum and isinstance(row['sha256'], str)
            and re.fullmatch(r'[0-9a-f]{64}', row['sha256']), 'bounded fresh provenance FilePin required')
    path = regular(row['path'])
    require(path.stat().st_size == row['bytes'], 'fresh provenance pin length differs')
    raw = bytearray()
    value = hashlib.sha256()
    limit = row['bytes'] + 1
    with path.open('rb') as stream:
        while len(raw) < limit:
            check_deadline(deadline)
            block = stream.read(min(4 << 20, limit - len(raw)))
            check_deadline(deadline)
            if not block:
                break
            raw.extend(block)
            value.update(block)
    require(len(raw) == row['bytes'] and value.hexdigest() == row['sha256'],
            'consumed fresh provenance bytes differ: ' + str(path))
    return bytes(raw)


def fresh_json(row, maximum, deadline):
    def pairs(fields):
        value = {}
        for key, item in fields:
            require(key not in value, 'duplicate fresh provenance JSON key')
            value[key] = item
        return value

    def invalid(_):
        raise ValueError('nonfinite fresh provenance JSON')

    return json.loads(fresh_bytes(row, maximum, deadline), object_pairs_hook=pairs, parse_constant=invalid)


def fresh_reviewed_pin(path, sha, deadline):
    check_deadline(deadline)
    path = regular(path)
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': sha}


def fresh_source_roster(path, sha, *, deadline):
    roster_pin = fresh_reviewed_pin(path, sha, deadline)
    rows = fresh_json(roster_pin, 1 << 20, deadline)
    require(isinstance(rows, dict) and set(rows) == set(FRESH_PROVENANCE_OWNERS),
            'exact fresh provenance source roster required')
    source_pins = [fresh_reviewed_pin(ROOT / key, rows[key], deadline) for key in FRESH_PROVENANCE_OWNERS]
    for row in source_pins:
        fresh_bytes(row, 2 << 20, deadline)
    return roster_pin, source_pins


def fresh_source_loader(source_pins, deadline):
    """Load only authenticated maintained source bytes for the fresh verifier."""
    approved = {Path(row['path']): row for row in source_pins}

    def load(path, name):
        path = Path(path)
        require(path in approved and path.suffix == '.py', 'unreviewed fresh executable source refused')
        raw = fresh_bytes(approved[path], 2 << 20, deadline)
        module = types.ModuleType(name)
        module.__file__ = str(path)
        exec(compile(raw, str(path), 'exec', dont_inherit=True), module.__dict__)
        if callable(getattr(module, 'load', None)):
            module.load = load
        if path == ROOT / 'scripts/cuda-runtime-environment.py':
            module._load = lambda leaf: load(path.parent / (leaf + '.py'), leaf.replace('-', '_'))

            def load_owner(owner_path, row, owner_name, core, runtime, cutoff):
                require(cutoff == deadline and approved.get(Path(owner_path)) == row,
                        'fresh recipe and reviewed source roster disagree')
                return load(owner_path, owner_name)

            module._fresh_owner = load_owner
        if path == ROOT / 'scripts/cuda-component-execution.py':
            module.base = lambda: load(ROOT / 'scripts/cuda-native-execution.py', 'fresh_preserve_native_owner')
        if path == ROOT / 'scripts/provision-static-onnx-runtime.py':
            lock = approved[ROOT / 'scripts/static-onnx-runtime-lock.json']
            require(module.LOCK == fresh_json(lock, 1 << 20, deadline), 'fresh CPU owner consumed lock differs')
        check_deadline(deadline)
        return module

    return load


def fresh_chain(env, *, deadline):
    """Authenticate the distinct recent receipts and derive their absolute archive pin."""
    proof, aggregate = env['build'], env['aggregate']
    request = env['request']
    require(set(request) == {'destination', 'aggregate_receipt'}
            and env['schema'] == FRESH_ENV_SCHEMA and env['activation_pins_available'] is False
            and proof['schema'] == FRESH_BUILD_SCHEMA and proof['qualification'] is False
            and proof['activation_pins_available'] is False
            and aggregate['schema'] == 'airk.cuda-static-aggregate-proof/1'
            and aggregate['qualification'] is False, 'fresh unqualified environment/6 required')
    root = Path(request['destination'])
    producer = proof['build_receipt']
    aggregate_pin = request['aggregate_receipt']
    configured_pin, configure_recipe_pin = proof['configure_receipt'], proof['configure_recipe']
    require(root.parent == USER_ROOT and root.is_absolute()
            and Path(producer['path']) == root / 'fresh-compile.json'
            and Path(configured_pin['path']) == root / 'fresh-configure.json'
            and Path(aggregate_pin['path']) == root / 'fresh-aggregate.json'
            and len({aggregate_pin['sha256'], producer['sha256'], configured_pin['sha256']}) == 3,
            'distinct same-build fresh receipt identities required')
    compiled = fresh_json(producer, 32 << 20, deadline)
    configured = fresh_json(configured_pin, 32 << 20, deadline)
    configure = fresh_json(configure_recipe_pin, 32 << 20, deadline)
    raw = fresh_json(aggregate_pin, 32 << 20, deadline)
    aggregate_recipe_pin = raw['recipe']
    require(configure_recipe_pin != aggregate_recipe_pin, 'fresh configure and aggregate recipe pins must differ')
    recipe = fresh_json(aggregate_recipe_pin, 32 << 20, deadline)

    require(compiled['schema'] == 'airk.private.cuda-fresh-compiled/1'
            and compiled['status'] == 'COMPILED_UNQUALIFIED' and compiled['qualification'] is False
            and compiled['configured'] == configured_pin and compiled['recipe'] == configure_recipe_pin
            and compiled['driver'] == proof['producer_source']
            and compiled['providers'] == proof['provider_dlls']
            and configured['schema'] == 'airk.private.cuda-fresh-configured/1'
            and configured['status'] == 'CONFIGURED_UNQUALIFIED' and configured['qualification'] is False
            and configured['recipe'] == configure_recipe_pin
            and configure['schema'] == 'airk.private.cuda-fresh-configure-recipe/1'
            and configure['msvc_ciso646_compat'] is True and configure['output_audit_owner'] == compiled['driver']
            and configure['execution_authorized'] is True and configure['qualification'] is False
            and Path(configure['fresh_output']) == root,
            'fresh configure/compile receipts differ from verified proof')
    require(raw['schema'] == 'airk.private.cuda-fresh-aggregate/1'
            and raw['status'] == 'AGGREGATED_UNQUALIFIED' and raw['qualification'] is False and raw['error'] is None
            and raw['compile_receipt'] == producer and raw['configure_receipt'] == configured_pin
            and recipe['schema'] == 'airk.private.cuda-fresh-aggregate-recipe/1'
            and recipe['compile_receipt'] == producer and recipe['qualification'] is False
            and recipe['execution_authorized'] is True
            and aggregate['receipt_sha256'] == aggregate_pin['sha256']
            and aggregate['plan_sha256'] == raw['plan_sha256'] == census_digest(raw['plan'])
            and aggregate['coff'] == raw['coff']
            and raw['plan']['compile_receipt'] == producer
            and raw['plan']['producer_proof_sha256'] == aggregate['producer_proof_sha256'] == census_digest(proof),
            'fresh actual aggregate receipt or plan differs')
    relative = aggregate['output']
    require(relative == raw['output'] and relative['path'] == 'gpu-aggregate/onnxruntime.lib',
            'fresh aggregate output differs')
    archive = {**relative, 'path': str(root / relative['path'])}
    require(pin(archive['path'], deadline=deadline) == archive
            and producer['sha256'] != OLD_PRODUCER and archive['sha256'] != ARCHIVE_SHA,
            'fresh actual producer/archive required')
    return dict(proof=proof, aggregate=aggregate, archive=archive, producer=producer,
                aggregate_receipt=aggregate_pin, aggregate_recipe=aggregate_recipe_pin,
                configure_receipt=configured_pin, configure_recipe=configure_recipe_pin)


def verify_fresh_environment(path, sha, roster_path, roster_sha, *, deadline):
    fresh_deadline(deadline)
    roster_pin, sources = fresh_source_roster(roster_path, roster_sha, deadline=deadline)
    env_pin = fresh_reviewed_pin(path, sha, deadline)
    require(Path(env_pin['path']).is_relative_to(USER_ROOT), 'fresh environment outside approved human root')
    env = fresh_json(env_pin, 16 << 20, deadline)
    require(env.get('schema') == FRESH_ENV_SCHEMA, 'explicit fresh environment/6 required')
    load = fresh_source_loader(sources, deadline)
    owner = load(ROOT / 'scripts/cuda-runtime-environment.py', 'preservation_fresh_environment')
    verified = owner.verify_handoff(path, sha, ROOT, deadline=deadline)
    check_deadline(deadline)
    require(verified['gpu_build'] == env['build'] and verified['gpu_aggregate'] == env['aggregate']
            and verified['environment'] == env['environment'] and verified['activation_pins_available'] is False,
            'dedicated fresh producer verification differs')
    provenance = fresh_chain(env, deadline=deadline)
    require(verified['environment'] == {'LIBONNXRUNTIME_NO_PKG_CONFIG': '1', 'ORT_SKIP_DOWNLOAD': '1',
            'ORT_PREFER_DYNAMIC_LINK': '0', 'ORT_LIB_LOCATION': str(Path(provenance['archive']['path']).parent),
            'ORT_LIB_PROFILE': 'Release'}, 'actual fresh archive environment differs')
    require(fresh_source_roster(roster_path, roster_sha, deadline=deadline) == (roster_pin, sources),
            'fresh provenance sources changed during verification')
    return dict(environment=env_pin, source_roster=roster_pin, source_pins=sources, **provenance)


def validate_fresh_graph_manifest(manifest, manifest_pin, runtime, provenance, *, deadline):
    actual = validate_graph_manifest(manifest, manifest_pin, runtime, provenance, deadline=deadline)
    require(manifest['producer_name'] == 'fresh-compile.json' and manifest['fits'] == [],
            'fresh graph requires unqualified native manifest and original compile leaf')
    providers = provenance['proof']['provider_dlls']
    names = tuple(Path(row['path']).name for row in providers)
    require(names == FRESH_PROVIDERS, 'exact fresh same-build provider pair required')
    files = [row for row in manifest['files'] if row['name'].lower().startswith('onnxruntime_providers_')]
    require(len(files) == len(FRESH_PROVIDERS) and {row['name'] for row in files} == set(FRESH_PROVIDERS),
            'native graph manifest provider roster differs')
    manifest_pair = {row['name']: row for row in files}
    for name, expected in zip(names, providers):
        row = manifest_pair[name]
        staged = pin(Path(runtime) / name, deadline=deadline)
        require((staged['bytes'], staged['sha256']) == (expected['bytes'], expected['sha256'])
                == (row['bytes'], row['sha256']), 'same-build staged graph provider bytes differ: ' + name)
    return actual


def exported_collectors(path, *, deadline=None):
    """Read bounded actual PE exports without executing the image or tools."""
    check_deadline(deadline)
    pe(path)
    check_deadline(deadline)
    with Path(path).open('rb') as f:
        size = Path(path).stat().st_size
        def read(offset, count):
            check_deadline(deadline)
            require(0 <= offset <= size and 0 <= count <= 1 << 20 and offset + count <= size, 'PE export read bound')
            f.seek(offset); data = f.read(count); check_deadline(deadline); require(len(data) == count, 'short PE export read'); return data
        header = int.from_bytes(read(60,4),'little')
        coff = read(header+4,20); sections = int.from_bytes(coff[2:4],'little'); optional_size = int.from_bytes(coff[16:18],'little')
        require(0 < sections <= 96 and 120 <= optional_size <= 4096, 'PE section/header bound')
        optional = read(header+24,optional_size)
        require(int.from_bytes(optional[108:112],'little') >= 1, 'PE export directory absent')
        export_rva, export_size = struct.unpack_from('<II',optional,112)
        section_rows=[]
        for index in range(sections):
            section=read(header+24+optional_size+index*40,40)
            virtual_size,rva,raw_size,raw_offset=struct.unpack_from('<IIII',section,8)
            section_rows.append((rva,raw_size,raw_offset,int.from_bytes(section[36:40],'little')))
        def mapped(rva, count):
            candidates=[(offset+rva-start,flags) for start,raw_size,offset,flags in section_rows if start <= rva and rva+count <= start+raw_size]
            require(len(candidates)==1, 'PE export RVA ambiguous/outside raw section'); return candidates[0]
        def at(rva,count): return read(mapped(rva,count)[0],count)
        directory=at(export_rva,40)
        functions,names,functions_rva,names_rva,ordinals_rva=struct.unpack_from('<IIIII',directory,20)
        require(0 < names <= functions <= 65536, 'PE export table bound')
        name_table=at(names_rva,names*4); ordinal_table=at(ordinals_rva,names*2); function_table=at(functions_rva,functions*4)
        found=[]
        for index in range(names):
            name_rva=struct.unpack_from('<I',name_table,index*4)[0]
            name=bytearray()
            for n in range(4096):
                char=at(name_rva+n,1)
                if char==b'\0':break
                name.extend(char)
            else: raise ValueError('PE export name bound')
            decoded=name.decode('ascii')
            if decoded in ['airk_ort_collect_final_placement_v1',GRAPH_EXPORT]:
                ordinal=struct.unpack_from('<H',ordinal_table,index*2)[0]; require(ordinal < functions, 'PE export ordinal bound')
                target=struct.unpack_from('<I',function_table,ordinal*4)[0]
                require(not export_rva <= target < export_rva+export_size and mapped(target,1)[1] & 0x20000000,
                        'collector is forwarded or nonexecutable')
                found.append(decoded)
        require(sorted(found)==sorted(['airk_ort_collect_final_placement_v1',GRAPH_EXPORT]), 'both actual static collector exports required')
        return sorted(found)


def verify_fresh_current(binding, manifest_pin, image_pin, build_pin, *, deadline):
    """Reopen current proof/document/link relationships after full fresh admission."""
    env_pin, roster_pin = binding['environment'], binding['source_roster']
    roster, sources = fresh_source_roster(roster_pin['path'], roster_pin['sha256'], deadline=deadline)
    env = fresh_json(env_pin, 16 << 20, deadline)
    expected = dict(environment=env_pin, source_roster=roster, source_pins=sources,
                    **fresh_chain(env, deadline=deadline))
    return verify_graph_inputs(binding, manifest_pin, image_pin, build_pin, expected, deadline=deadline)


def verify_graph_binding(binding, manifest_pin, image_pin, build_pin, *, deadline=None):
    check_deadline(deadline)
    kw = {} if deadline is None else {'deadline':deadline}
    if binding.get('schema') == FRESH_BINDING_SCHEMA:
        expected = verify_fresh_environment(binding['environment']['path'], binding['environment']['sha256'],
                                            binding['source_roster']['path'], binding['source_roster']['sha256'],
                                            deadline=deadline)
        return verify_graph_inputs(binding, manifest_pin, image_pin, build_pin, expected, deadline=deadline)
    require(binding.get('schema') == 'airk.private.cuda-finalgraph-image-binding/1'
            and binding.get('build_receipt') == build_pin and binding.get('manifest') == manifest_pin,
            'exact finalgraph image provenance binding required')
    for row in [binding['preserver_source'],binding['cargo_json'],manifest_pin,image_pin]:
        require(pin(row['path'],**kw) == row, 'graph image binding input drift')
    require(binding['preserver_source'] == pin(Path(__file__).resolve(),**kw), 'reviewed graph preserver changed')
    expected=verify_environment(binding['environment']['path'],binding['environment']['sha256'],
                                binding['source_roster']['path'],binding['source_roster']['sha256'],**kw)
    require(all(binding.get(k)==v for k,v in expected.items()), 'finalgraph producer proof drift')
    runtime=Path(manifest_pin['path']).parent
    actual=validate_graph_manifest(json.loads(Path(manifest_pin['path']).read_bytes()),manifest_pin,runtime,expected,**kw)
    require(actual==binding['runtime_producer'], 'runtime producer binding drift')
    ort,_=cargo_rows(Path(binding['cargo_json']['path']).read_text(encoding='utf-8'),Path(expected['archive']['path']))
    require(ort==binding['actual_static_ORT_rows'], 'actual finalgraph Cargo link differs')
    exports=exported_collectors(image_pin['path'],**kw)
    require(binding['image_exports'].get(image_pin['sha256'])==exports, 'actual image collector export differs')
    check_deadline(deadline)
    return expected


def verify_graph_inputs(binding, manifest_pin, image_pin, build_pin, expected, *, deadline):
    check_deadline(deadline)
    require(binding.get('schema') == FRESH_BINDING_SCHEMA
            and binding.get('build_receipt') == build_pin and binding.get('manifest') == manifest_pin
            and all(binding.get(k) == v for k, v in expected.items()), 'exact fresh finalgraph image binding differs')
    for row in [binding['preserver_source'], binding['cargo_json'], manifest_pin, image_pin, build_pin]:
        require(pin(row['path'], deadline=deadline) == row, 'fresh graph image binding input drift')
    require(binding['preserver_source'] == pin(Path(__file__).resolve(), deadline=deadline),
            'reviewed fresh graph preserver changed')
    runtime = Path(manifest_pin['path']).parent
    manifest = fresh_json(manifest_pin, 128 << 10, deadline)
    actual = validate_fresh_graph_manifest(manifest, manifest_pin, runtime, expected, deadline=deadline)
    require(actual == binding['runtime_producer'], 'fresh runtime producer binding drift')
    ort, _ = cargo_rows(fresh_bytes(binding['cargo_json'], 64 << 20, deadline).decode('utf-8'),
                        Path(expected['archive']['path']))
    require(ort == binding['actual_static_ORT_rows'], 'actual fresh finalgraph Cargo link differs')
    exports = exported_collectors(image_pin['path'], deadline=deadline)
    require(binding['image_exports'].get(image_pin['sha256']) == exports, 'actual fresh image collector exports differ')
    check_deadline(deadline)
    return expected


def preservation_mode(args):
    incremental = getattr(args, 'incremental_finalgraph', False)
    fresh = getattr(args, 'fresh_finalgraph', False)
    seconds = getattr(args, 'verification_seconds', None)
    fresh_inputs = (getattr(args, 'fresh_environment', None), getattr(args, 'fresh_environment_sha256', None))
    if not fresh:
        require(not any(x is not None for x in (*fresh_inputs, seconds)),
                'fresh proof inputs require explicit --fresh-finalgraph')
        return incremental, None
    require(not incremental and not any(getattr(args, name, None) is not None
                for name in ('incremental_environment', 'incremental_environment_sha256')),
            'fresh and incremental finalgraph preservation are mutually exclusive')
    require(all((*fresh_inputs, args.provenance_source_roster, args.provenance_source_roster_sha256, args.manifest_sha256)),
            'complete reviewed fresh graph preservation inputs required')
    require(type(seconds) in (int, float) and math.isfinite(seconds) and 0 < seconds <= 600,
            'finite positive --verification-seconds up to 600 required')
    deadline = time.monotonic() + seconds
    fresh_deadline(deadline)
    return False, deadline


def run(args):
    incremental, deadline = preservation_mode(args)
    fresh = deadline is not None
    graph = incremental or fresh
    fresh_kw = {'deadline': deadline} if fresh else {}
    helper_pin = exact_pin(Path(__file__).resolve(), args.approved_script_sha256, **fresh_kw)
    guard_pin = exact_pin(GUARD, args.guard_sha256, **fresh_kw)
    shell('. ' + quote(GUARD) + '; Assert-AirkHumanExecution -ExpectedSid ' + quote(SID) + ' | Out-Null')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(head == args.source_commit, 'reviewed source commit differs')
    validate_destinations(args.output, args.runtime, args.inference_name)
    guard(args.output, ARTIFACT_ROOT, args.guard_sha256)
    guard(args.runtime/args.inference_name, USER_ROOT, args.guard_sha256)
    parents = {str(p): policy(p) for p in [ARTIFACT_ROOT, USER_ROOT]}
    owner_policy(parents[str(ARTIFACT_ROOT)])
    owner_policy(parents[str(USER_ROOT)], payload=True)
    receipt_pin = exact_pin(args.receipt, args.receipt_sha256, **fresh_kw)
    log_pin = exact_pin(args.log, args.log_sha256, **fresh_kw)
    graph_binding = None
    receipt = fresh_json(receipt_pin, 32 << 20, deadline) if fresh else json.loads(args.receipt.read_bytes())
    validate_receipt(receipt, args.native_group, args.log.name)
    if graph:
        if fresh:
            provenance = verify_fresh_environment(args.fresh_environment, args.fresh_environment_sha256,
                                                  args.provenance_source_roster, args.provenance_source_roster_sha256,
                                                  deadline=deadline)
            manifest_pin = fresh_reviewed_pin(args.runtime / 'airk-cuda-runtime.json', args.manifest_sha256, deadline)
            manifest = fresh_json(manifest_pin, 128 << 10, deadline)
            runtime_producer = validate_fresh_graph_manifest(manifest, manifest_pin, args.runtime, provenance,
                                                             deadline=deadline)
            cargo_log = fresh_bytes(log_pin, 64 << 20, deadline).decode('utf-8')
        else:
            require(all(getattr(args,k,None) for k in ['incremental_environment','incremental_environment_sha256','provenance_source_roster','provenance_source_roster_sha256','manifest_sha256']), 'complete reviewed incremental inputs required')
            provenance=verify_environment(args.incremental_environment,args.incremental_environment_sha256,args.provenance_source_roster,args.provenance_source_roster_sha256)
            manifest_pin=exact_pin(args.runtime/'airk-cuda-runtime.json',args.manifest_sha256)
            runtime_producer=validate_graph_manifest(json.loads(Path(manifest_pin['path']).read_bytes()),manifest_pin,args.runtime,provenance)
            cargo_log=args.log.read_text(encoding='utf-8')
        env_pin,archive_pin=provenance['environment'],provenance['archive']
        ort,selected=cargo_rows(cargo_log,Path(archive_pin['path']))
        graph_binding={'schema':FRESH_BINDING_SCHEMA if fresh else 'airk.private.cuda-finalgraph-image-binding/1',**provenance,
            'manifest':manifest_pin,'runtime_producer':runtime_producer,'build_receipt':receipt_pin,
            'cargo_json':log_pin,'actual_static_ORT_rows':ort,'image_exports':{},'preserver_source':helper_pin}
    else:
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
        check_deadline(deadline)
        source = regular(row['executable'])
        require(source.is_relative_to(Path(receipt['target_directory']).resolve()) and 0 < source.stat().st_size <= 4 << 30, 'image outside actual target/size bounds')
        pe(source)
        originals[package] = snapshot(source)
        if graph:
            kw = {'deadline': deadline} if fresh else {}
            graph_binding['image_exports'][originals[package]['pin']['sha256']] = exported_collectors(source, **kw)
    runtime = runtime_census(args.runtime, args.guard_sha256)
    require(census_digest(runtime) == args.runtime_census_sha256, 'reviewed full runtime census differs')
    runtime_policy = policy(args.runtime)
    owner_policy(runtime_policy, payload=True)
    if graph:
        require(any(row['pin']==graph_binding['manifest'] for row in runtime) and any(row['pin']==graph_binding['runtime_producer'] for row in runtime), 'fresh manifest/producer absent from runtime census')
    else:
        validate_runtime(runtime)
    context = None
    require(bool(args.compile_input_context) == bool(args.compile_input_context_sha256), 'compile-context path/hash pair required')
    if args.compile_input_context:
        context = exact_pin(args.compile_input_context, args.compile_input_context_sha256)
        require(context['bytes'] <= 16 << 20, 'bounded compile context required')
    pins = [helper_pin, guard_pin, receipt_pin, log_pin, env_pin, archive_pin] + ([context] if context else [])
    if graph:
        pins += [graph_binding[k] for k in ['source_roster','producer','manifest','runtime_producer']] + graph_binding['source_pins']
    def reopen(addition=None):
        check_deadline(deadline)
        kw = {'deadline': deadline} if fresh else {}
        require(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head, 'HEAD drift')
        for value in pins:
            require(pin(value['path'], **kw) == value, 'build/static/context pin drift')
        for relative, expected in receipt['groups']['native']['files'].items():
            exact_pin(ROOT/relative, expected, **kw)
        unchanged_runtime(runtime, runtime_census(args.runtime, args.guard_sha256), addition)
        require(policy(args.runtime) == runtime_policy, 'runtime directory policy drift')
        for package, value in originals.items():
            require(snapshot(value['pin']['path']) == value, 'original native image drift: ' + package)
        require(all(policy(p) == value for p, value in parents.items()), 'output parent policy drift')
        if fresh:
            verify_fresh_current(graph_binding, graph_binding['manifest'], originals['airk-inference']['pin'],
                                 receipt_pin, deadline=deadline)
        elif incremental:
            verify_graph_binding(graph_binding,graph_binding['manifest'],originals['airk-inference']['pin'],receipt_pin)
    reopen()
    check_deadline(deadline)
    make_directory(args.output, ARTIFACT_ROOT, args.guard_sha256)
    outcome = {'status':'RUNNING','native_group':args.native_group,'source_commit':head,'native_model_request_dispatched':False}
    try:
        images = args.output/'images'
        make_directory(images, ARTIFACT_ROOT, args.guard_sha256)
        result = []
        bindings = []
        addition = None
        for package, row in selected:
            check_deadline(deadline)
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
        if graph: inventory['static_graph_binding']=graph_binding
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
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--incremental-finalgraph', action='store_true')
    modes.add_argument('--fresh-finalgraph', action='store_true')
    parser.add_argument('--fresh-environment', type=Path)
    parser.add_argument('--fresh-environment-sha256')
    parser.add_argument('--verification-seconds', type=float)
    parser.add_argument('--incremental-environment',type=Path)
    parser.add_argument('--incremental-environment-sha256')
    parser.add_argument('--provenance-source-roster',type=Path)
    parser.add_argument('--provenance-source-roster-sha256')
    parser.add_argument('--manifest-sha256')
    parser.add_argument('--inference-name', required=True)
    parser.add_argument('--compile-input-context', type=Path)
    parser.add_argument('--compile-input-context-sha256')
    run(parser.parse_args())


if __name__ == '__main__':
    main()