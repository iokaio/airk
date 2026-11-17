# SPDX-License-Identifier: Apache-2.0
"""Reviewed private orchestration: canonical metadata owners, existing signatures only."""
from __future__ import annotations
import argparse
import copy
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import time


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def pinned(pin):
    path = Path(pin['path'])
    metadata = path.lstat()
    require(path.is_absolute() and stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1
            and not getattr(metadata, 'st_file_attributes', 0) & 0x400,
            'pinned input is redirected or aliased: ' + str(path))
    require(metadata.st_size == pin['bytes'] and sha(path) == pin['sha256'], 'pinned input drift: ' + str(path))
    return path


def load(root, name):
    path = root / 'scripts' / (name + '.py')
    specification = importlib.util.spec_from_file_location('adoption_' + name.replace('-', '_'), path)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def capture(root, command):
    return subprocess.check_output(list(map(str, command)), cwd=root, encoding='utf-8').strip()


def identity(root, plan):
    query = "$i=[Security.Principal.WindowsIdentity]::GetCurrent(); $s=(Get-Process -Id $PID).SessionId; [pscustomobject]@{name=$i.Name;sid=$i.User.Value;session=$s;explorer=@(Get-Process explorer -IncludeUserName|Select-Object UserName,SessionId)}|ConvertTo-Json -Depth 4 -Compress"
    observed = json.loads(capture(root, ['pwsh', '-NoProfile', '-NonInteractive', '-Command', query]))
    active = capture(root, ['quser'])
    require(observed['name'] == plan['human'] and observed['sid'] == plan['human_sid'], 'effective user differs from reviewed human')
    require(any(row['UserName'] == plan['human'] and row['SessionId'] == observed['session'] for row in observed['explorer']), 'shell session differs from logged-in Explorer owner')
    require(any(line.startswith('>') and 'tdotn' in line and 'Active' in line and str(observed['session']) in line.split() for line in active.splitlines()), 'effective shell is not the active human session')
    return {'effective': observed, 'quser': active}


def private_directory(root, path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path.absolute(), 'private temp parent is aliased')
    for ancestor in (path, *path.parents):
        metadata = ancestor.lstat()
        require(stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode)
                and not getattr(metadata, 'st_file_attributes', 0) & 0x400, 'private temp ancestor is redirected')
    query = "$item=Get-Item -LiteralPath $env:AIRK_ADOPTION_CHECK_PATH -Force -ErrorAction Stop; $acl=Get-Acl -LiteralPath $item.FullName; [pscustomobject]@{path=$item.FullName;owner_sid=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value;sddl=$acl.Sddl;access=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|ForEach-Object{[pscustomobject]@{sid=$_.IdentityReference.Value;rights=[int]$_.FileSystemRights;kind=[int]$_.AccessControlType;inheritance=[int]$_.InheritanceFlags;propagation=[int]$_.PropagationFlags;inherited=$_.IsInherited}})}|ConvertTo-Json -Depth 5 -Compress"
    prior = os.environ.get('AIRK_ADOPTION_CHECK_PATH')
    os.environ['AIRK_ADOPTION_CHECK_PATH'] = str(path)
    try:
        return json.loads(capture(root, ['pwsh', '-NoProfile', '-NonInteractive', '-Command', query]))
    finally:
        if prior is None:
            os.environ.pop('AIRK_ADOPTION_CHECK_PATH', None)
        else:
            os.environ['AIRK_ADOPTION_CHECK_PATH'] = prior


def protected_temp(root, plan):
    parent, path = Path(plan['private_temp_parent']), Path(plan['private_temp'])
    require(path.parent == parent and not path.exists() and not path.is_symlink(), 'fresh direct private temp child required')
    observed = private_directory(root, parent)
    require(observed == plan['approved_private_parent'] and observed['owner_sid'] == plan['human_sid'],
            'reviewed private parent owner/access policy changed')
    # Root approved this exact observed parent descriptor. Compare the actual
    # inherited child descriptor; never alter ownership or an ACL to get a pass.
    path.mkdir()
    child = private_directory(root, path)
    require(child['owner_sid'] == plan['human_sid'] and child['access'] == observed['access'],
            'private child did not retain the approved effective owner/access policy')
    os.environ.update(TEMP=str(path), TMP=str(path), TMPDIR=str(path))
    return {'parent': observed, 'child': child, 'TEMP': str(path), 'TMP': str(path), 'TMPDIR': str(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256', required=True)
    args = parser.parse_args()
    require(sha(args.plan) == args.plan_sha256, 'reviewed plan changed')
    plan = json.loads(args.plan.read_bytes())
    require(plan['schema'] == 'airk.private.existing-signed-native-adoption-plan/1', 'unknown plan')
    root = Path(plan['candidate_root']).resolve(strict=True)
    human = identity(root, plan)
    require(capture(root, ['git', 'rev-parse', 'HEAD']) == plan['commit'], 'candidate commit changed')
    require(capture(root, ['git', 'status', '--porcelain']) == '', 'candidate source is not clean')
    for pin in plan['pins'].values():
        pinned(pin)
    source = Path(plan['source_runtime']).resolve(strict=True)
    local = Path(plan['local_run']).resolve(strict=True)
    evidence = Path(plan['evidence']).resolve(strict=True)
    require(args.plan.resolve(strict=True).parent == evidence and Path(__file__).resolve().parent == evidence,
            'orchestration evidence path changed')
    destinations = {key: Path(plan[key]).resolve() for key in ('signed_output', 'unsigned_package_output', 'signed_package_output')}
    for path in destinations.values():
        require(path.is_relative_to(root / 'artifacts') and path != root / 'artifacts' and not path.exists(), 'fresh candidate-only output required')
    execution = evidence / 'execution.json'
    require(not execution.exists(), 'execution receipt is immutable; use a new reviewed plan after failure')
    started = time.monotonic()
    receipt = {'schema': 'airk.private.existing-signed-native-adoption/1', 'status': 'started',
               'started_utc': datetime.now(timezone.utc).isoformat(), 'identity': human,
               'plan_sha256': args.plan_sha256, 'script_sha256': sha(__file__), 'steps': [],
               'source_commit': plan['commit'], 'signing_performed': False, 'compilation_performed': False,
               'installation_performed': False, 'publication_performed': False, 'passed': False}
    with execution.open('x', encoding='utf-8') as stream:
        json.dump(receipt, stream, indent=2)
        stream.flush()
        os.fsync(stream.fileno())

    def save():
        execution.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')

    def package(output, *extra):
        name = 'unsigned-package' if not extra else 'signed-package'
        command = [sys.executable, '-B', str(root / 'scripts/package-native.py'), '--target', plan['target'],
                   '--runtime-dir', str(source), '--output', str(output), *map(str, extra)]
        row = {'step': name, 'command': command, 'status': 'started'}
        receipt['steps'].append(row)
        save()
        before = time.monotonic()
        with (evidence / (name + '.log')).open('xb') as log:
            result = subprocess.run(command, cwd=root, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=1800)
        row.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - before, status='completed')
        save()
        require(result.returncode == 0, name + ' failed; original output/log preserved')
        return output / 'publish/manifest.json'

    try:
        environment = json.loads(pinned(plan['pins']['static_environment']).read_bytes())['environment']
        os.environ.update(environment)
        os.environ['PYTHONDONTWRITEBYTECODE'] = '1'
        os.environ['AIRK_BUNDLED_MODEL_DIR'] = plan['bundled_models']
        receipt['private_temp_boundary'] = protected_temp(root, plan)
        save()
        signer = load(root, 'sign-native')
        package_owner = load(root, 'package-native')
        builder = package_owner.runtime_builder()
        installer = load(root, 'build-local-service-installer')
        admin = load(root, 'build-admin-assets')
        adapter = load(root, 'local_signing')
        tool = adapter.signtool_path()
        lock_digest = sha(root / 'Cargo.lock')
        source_identity = builder.source_identity()
        static_inputs = builder.verify_static_inputs()
        admin_block = admin.block_from_inventory(json.loads(pinned(plan['pins']['admin_inventory']).read_bytes()))
        tested = installer.verified_runtime(source, plan['version'], source_identity, static_inputs, admin_block)
        shadow = installer.verified_runtime(local / 'tested-runtime', plan['version'], source_identity, static_inputs, admin_block)
        qualified = installer.verified_runtime(local / 'qualified-runtime', plan['version'], source_identity, static_inputs, admin_block)
        require(shadow['files'] == tested['files'] and shadow['build'] == tested['build'], 'localdriver did not consume exact tested runtime')
        local_receipt = json.loads(pinned(plan['pins']['local_receipt']).read_bytes())
        driver = json.loads(pinned(plan['pins']['local_driver_receipt']).read_bytes())
        require(driver['schema'] == 'airk-readiness-command/1' and driver['passed'] is True
                and driver['exit_code'] == driver['runner_exit_code'] == 0 and driver['status'] == 'completed'
                and driver['inputs_unchanged'] is True and driver['changed_groups'] == []
                and driver['runner_unchanged'] is True and driver['runtime_unchanged'] is True
                and driver['command_inputs_unchanged'] is True and driver['evidence_errors'] == [],
                'original localdriver qualification wrapper did not pass unchanged')
        require(local_receipt['schema'] == 'airk-local-service-installer/1'
                and local_receipt['result'] == 'built' and local_receipt['signed'] is True
                and local_receipt['dirty'] is False and local_receipt['commit'] == plan['commit']
                and local_receipt['version'] == plan['version'] and local_receipt['target'] == plan['target'],
                'original signed localdriver receipt differs')
        require(local_receipt['input_runtime'] == installer.runtime_input_receipt(tested)
                and local_receipt['runtime_build'] == qualified['build'], 'localdriver original input/qualified metadata differs')
        mutable = {'airk.exe', 'airkctl.exe', 'airk-runtime.json', 'airk-build.json', 'airk-install.json'}
        require(set(qualified['files']) == set(tested['files'])
                and all(qualified['files'][name] == digest for name, digest in tested['files'].items() if name not in mutable),
                'signed qualification changed unrelated runtime payload')
        for key in ('source', 'profile', 'static_ort', 'admin_assets', 'rustc', 'embedded_model_smoke'):
            require(qualified['build'][key] == tested['build'][key], 'signed qualification inputs differ: ' + key)
        layout = package_owner.bounded_json(local / 'layout/payload-inventory.json')
        require(sha(local / 'layout/payload-inventory.json') == local_receipt['payload_inventory_sha256']
                and layout['files'] == local_receipt['files'], 'localdriver payload inventory differs')
        for name, item in layout['files'].items():
            path = (local / 'layout' / name).resolve(strict=True)
            require(path.is_relative_to(local / 'layout') and path.is_file()
                    and path.stat().st_size == item['bytes'] and sha(path) == item['sha256'], 'localdriver payload drift')
        signed_paths = {'airk': local / 'qualified-runtime/airk.exe', 'airkctl': local / 'qualified-runtime/airkctl.exe',
                        'uninstaller': local / 'signed/uninstaller.exe', 'installer': Path(local_receipt['installer'])}
        documents = {}
        for name, binary in signed_paths.items():
            document = json.loads(pinned(plan['pins']['signing_' + name]).read_bytes())
            signer.local_component(document)
            require(document['commit'] == plan['commit'] and document['target'] == plan['target']
                    and document['cargo_lock_sha256'] == lock_digest and sha(binary) == document['binary_sha256']
                    and sha(tool) == document['local_signing']['signtool_sha256'], 'signed artifact/component identity differs')
            require(local_receipt['signing']['artifacts'][name] == {key: document[key] for key in ('source_sha256', 'binary_sha256', 'identity')},
                    'localdriver signing summary differs from original component proof')
            if name in ('airk', 'airkctl'):
                require(document['source_sha256'] == tested['files'][name + '.exe'], 'signed image belongs to another tested build')
            else:
                signer.verify(binary, document, str(tool))
            documents[name] = document
        require(sha(signed_paths['installer']) == local_receipt['installer_sha256'], 'outer signed installer drift')
        receipt['preflight'] = {'source': source_identity, 'static_ort': static_inputs, 'admin_assets': admin_block,
                                'tested_payload': tested['files'], 'signed_payload': qualified['files'],
                                'local_installer_sha256': local_receipt['installer_sha256'], 'signtool_sha256': sha(tool)}
        save()
        # The unsigned package supplies genuine canonical compiler/source provenance;
        # it performs only cargo metadata --no-deps (not compilation) plus native smokes.
        unsigned_manifest = package(destinations['unsigned_package_output'])
        unsigned = package_owner.source_packaging_metadata(unsigned_manifest, plan['target'], True)
        require(unsigned['payload'] == tested['files'] and unsigned['source'] == source_identity,
                'canonical unsigned package differs from tested pair')
        output = destinations['signed_output']
        prepared_source, runtime, executables, original = signer.prepare_pair(source, output, plan['target'])
        require(executables == ('airk.exe', 'airkctl.exe') and original == tested['files'], 'canonical signing preparation differs')
        components = {}
        for name in executables:
            shutil.copyfile(signed_paths[Path(name).stem], runtime / name)
            require(sha(runtime / name) == qualified['files'][name], 'copied existing signed bytes changed')
            components[name] = copy.deepcopy(documents[Path(name).stem])
        signer.finish_pair(prepared_source, output, plan['target'], components, plan['commit'], lock_digest, str(tool), local=True)
        sealed_runtime, signing = signer.read_verified_pair(output, source, plan['target'], plan['commit'], lock_digest, str(tool), allow_local=True)
        signed_files, _ = package_owner.validate_runtime(sealed_runtime, plan['version'], plan['target'], source_identity)
        require(all(signed_files[name] == digest for name, digest in qualified['files'].items()
                    if name not in {'airk-runtime.json', 'airk-build.json', 'airk-install.json'}),
                'adopted pair differs from independently qualified signed images/payload')
        signed_manifest = package(destinations['signed_package_output'], '--signed-dir', output,
                                  '--local-artifact-signing', '--source-manifest', unsigned_manifest, '--signtool', tool)
        published = package_owner.bounded_json(signed_manifest, 4_194_304)
        require(published['signed_distribution'] is True and published['signing']['provenance'] == 'local'
                and published['payload'] == signed_files and published['commit'] == plan['commit'],
                'canonical signed archive metadata differs from adopted pair')
        require(package_owner.inventory(source) == tested['files'], 'tested source changed during adoption')
        for pin in plan['pins'].values():
            pinned(pin)
        require(capture(root, ['git', 'rev-parse', 'HEAD']) == plan['commit']
                and capture(root, ['git', 'status', '--porcelain']) == '', 'candidate source changed during packaging')
        receipt.update(status='completed', passed=True,
                       unsigned_manifest={'path': str(unsigned_manifest), 'sha256': sha(unsigned_manifest)},
                       signing_receipt={'path': str(output / 'signing.json'), 'sha256': sha(output / 'signing.json')},
                       signed_manifest={'path': str(signed_manifest), 'sha256': sha(signed_manifest)},
                       archive={'path': str(signed_manifest.parent / published['archive']), 'sha256': published['archive_sha256']})
    except BaseException as error:
        receipt.update(status='failed', error_type=type(error).__name__, error=str(error))
        raise
    finally:
        receipt['elapsed_seconds'] = time.monotonic() - started
        receipt['ended_utc'] = datetime.now(timezone.utc).isoformat()
        save()


if __name__ == '__main__':
    main()
