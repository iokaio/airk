# SPDX-License-Identifier: Apache-2.0
"""Copy only the seven verified Windows distribution files; no package/build/run."""
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
from datetime import datetime, timezone
import zipfile

ROOT = Path('D:/code/Github/airk')
CANDIDATE = ROOT / 'artifacts/v190/final-plan/candidate-worktree-r1'
EVIDENCE = ROOT / 'artifacts/v190/final-plan/exec-20261116T052942Z'
DESTINATION = ROOT / 'artifacts/arbiter-1.8.207/windows-candidate-r1'
COMMIT = '1933a3930959802c85fb2e99a6974871216d7482'
HUMAN = 'S-1-5-21-3678370870-3858642417-826968105-1002'


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def module(name):
    spec = importlib.util.spec_from_file_location('delivery_' + name.replace('-', '_'), CANDIDATE / 'scripts' / (name + '.py'))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def capture(command):
    return subprocess.check_output(list(map(str, command)), cwd=CANDIDATE, text=True, encoding='utf-8').strip()


def policy(path):
    query = "$a=Get-Acl -LiteralPath $env:AIRK_DELIVERY_ACL_PATH;[pscustomobject]@{owner=$a.GetOwner([Security.Principal.SecurityIdentifier]).Value;sddl=$a.Sddl}|ConvertTo-Json -Compress"
    import os
    previous = os.environ.get('AIRK_DELIVERY_ACL_PATH')
    os.environ['AIRK_DELIVERY_ACL_PATH'] = str(path)
    try:
        return json.loads(capture(['pwsh','-NoProfile','-NonInteractive','-Command',query]))
    finally:
        if previous is None:
            os.environ.pop('AIRK_DELIVERY_ACL_PATH', None)
        else:
            os.environ['AIRK_DELIVERY_ACL_PATH'] = previous


def regular(path):
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1
            and not getattr(metadata, 'st_file_attributes', 0) & 0x400, 'redirected/aliased distribution source')
    return metadata


def census(archive):
    blocked = {'.git','.airk','temp','fixtures','training','weights-v2','candidate-assets','credentials','target','state','cache'}
    rows = []
    with zipfile.ZipFile(archive) as zipped:
        for item in zipped.infolist():
            name = PurePosixPath(item.filename)
            require(not name.is_absolute() and '..' not in name.parts and '\\' not in item.filename
                    and not any(':' in part for part in name.parts), 'unsafe archive path')
            require(not any(part.lower() in blocked for part in name.parts), 'private source/state path in archive')
            if 'node_modules' in name.parts:
                require(item.filename in {
                    'extension/node_modules/jsonc-parser/LICENSE.md',
                    'extension/node_modules/jsonc-parser/package.json',
                    'extension/node_modules/jsonc-parser/lib/umd/main.js',
                    'extension/node_modules/jsonc-parser/lib/umd/impl/edit.js',
                    'extension/node_modules/jsonc-parser/lib/umd/impl/format.js',
                    'extension/node_modules/jsonc-parser/lib/umd/impl/parser.js',
                    'extension/node_modules/jsonc-parser/lib/umd/impl/scanner.js',
                    'extension/node_modules/jsonc-parser/lib/umd/impl/string-intern.js'},
                    'unexpected dependency/source subtree outside pinned public runtime closure')
            require(not item.filename.lower().endswith(('.safetensors','.npz','.parquet','.arrow','.sqlite','.pfx','.p12','.key','.pem')),
                    'private data/key format in archive')
            require(not stat.S_ISLNK(item.external_attr >> 16), 'archive link')
            rows.append({'name': item.filename, 'bytes': item.file_size})
    return rows


def sums(directory, archive):
    text = (directory / 'SHA256SUMS').read_text(encoding='utf-8')
    expected = digest(directory / archive) + '  ' + archive + '\n' + digest(directory / 'manifest.json') + '  manifest.json\n'
    require(text == expected, 'canonical SHA256SUMS differs')


def main():
    require(not DESTINATION.exists() and not (EVIDENCE / 'delivery-census-r1.json').exists(), 'fresh exact delivery scope required')
    identity = json.loads(capture(['pwsh','-NoProfile','-NonInteractive','-Command',
        "$i=[Security.Principal.WindowsIdentity]::GetCurrent();[pscustomobject]@{name=$i.Name;sid=$i.User.Value}|ConvertTo-Json -Compress"]))
    require(identity['sid'] == HUMAN and identity['name'] == 'TYAI\\tdotn', 'wrong effective human')
    require(capture(['git','rev-parse','HEAD']) == COMMIT, 'candidate commit differs')
    dirty = subprocess.check_output(['git','status','--porcelain'], cwd=CANDIDATE, text=True, encoding='utf-8').rstrip('\r\n')
    allowed_overlay = {'scripts/check-raw-https-clients.py','scripts/tests/test_raw_https_clients.py'}
    require(all(line[:3] == ' M ' and line[3:] in allowed_overlay for line in dirty.splitlines()),
            'unexpected source mutation outside approved raw HTTPS overlay')
    clean_proof = EVIDENCE / 'signed-native-adoption-r2/clean-candidate-artifact-proof.json'
    require(digest(clean_proof) == '2b5f0a3091ff744407df2a3301f8a05337dcefc429f3a1c6353c491cb7416fec', 'original clean proof changed')
    original = json.loads(clean_proof.read_bytes())
    require(original['clean'] is True and original['commit'] == COMMIT, 'original candidate was not clean')
    native_dir = CANDIDATE / 'artifacts/windows-native-package-r1/publish'
    vsix_dir = CANDIDATE / 'artifacts/windows-milestone-vsix-r1/publish'
    local = CANDIDATE / 'artifacts/local-service-installer/20261116-053728'
    native, extension, contracts, signer = (module(name) for name in ('package-native','package-extension','split_receipts','sign-native'))
    n = native.bounded_json(native_dir / 'manifest.json', 4_194_304)
    v = native.bounded_json(vsix_dir / 'manifest.json', 4_194_304)
    contracts.validate_native_candidate(n)
    contracts.validate_vsix(v)
    require(n['commit'] == v['commit'] == COMMIT and n['tree'] == v['tree'] == original['tree'], 'archive source identity differs')
    require(n['signed_distribution'] is True and n['signing']['provenance'] == 'local' and n['dirty_checkout'] is False,
            'native original signed clean provenance differs')
    require(n['source'] == native.runtime_builder().source_identity(), 'approved harness overlay changed native C7 input identity')
    stage = CANDIDATE / 'artifacts/windows-native-package-r1/stage'
    payload, executables = native.validate_runtime(stage,n['version'],n['target'],n['source'])
    require(payload == n['payload'], 'native complete runtime/legal census differs')
    native.verify_archive(native_dir / n['archive'], n['payload'], executables)
    extension.verify_vsix(vsix_dir / v['archive'],v['payload'],version=v['version'],pre_release=v['pre_release'],display_name=v['display_name'])
    native_entries = census(native_dir / n['archive'])
    vsix_entries = census(vsix_dir / v['archive'])
    for directory, document, proofkey in [(native_dir,n,'signed_zip'),(vsix_dir,v,'universal_vsix')]:
        require({p.name for p in directory.iterdir()} == {document['archive'],'manifest.json','SHA256SUMS'}, 'unexpected publish-root file')
        require(digest(directory / document['archive']) == document['archive_sha256'] == original[proofkey]['sha256'], 'archive data changed')
        sums(directory,document['archive'])
    signing = json.loads((CANDIDATE / 'artifacts/windows-signed-native-r1/signing.json').read_bytes())
    adapter = module('local_signing')
    tool = str(adapter.signtool_path())
    signatures = {}
    for name, component in signing['components'].items():
        signer.local_component(component)
        require(component['binary_sha256'] == n['payload'][name], 'native component differs from archive')
        signatures[name] = signer.verify(stage / name,component,tool)
    installer_receipt = native.bounded_json(local / 'receipt.json',4_194_304)
    executable = Path(installer_receipt['installer'])
    component = json.loads((local / 'signing-installer.json').read_bytes())
    signer.local_component(component)
    require(component['commit'] == COMMIT and component['binary_sha256'] == digest(executable)
            == installer_receipt['installer_sha256'] == original['installer_exe']['sha256'], 'signed installer source differs')
    signatures['installer'] = signer.verify(executable,component,tool)
    rows = []
    for category, directory, document in [('native',native_dir,n),('vsix',vsix_dir,v)]:
        rows.extend((category,directory / name) for name in (document['archive'],'manifest.json','SHA256SUMS'))
    rows.append(('installer',executable))
    require(len(rows) == 7, 'exact seven public producer files required')
    for _, source in rows:
        regular(source)
    parent_policy = policy(DESTINATION.parent)
    require(parent_policy['owner'] == HUMAN, 'output parent owner differs')
    planned = [{'source':str(source),'destination':str(DESTINATION/category/source.name),'sha256':digest(source),'bytes':source.stat().st_size} for category,source in rows]
    # All content, census, checksum and signature checks above are read-only.
    DESTINATION.mkdir()
    for category in ('native','installer','vsix'):
        (DESTINATION/category).mkdir()
    copied = []
    for row in planned:
        source,destination = Path(row['source']),Path(row['destination'])
        with source.open('rb') as stream,destination.open('xb') as out:
            shutil.copyfileobj(stream,out,1024*1024)
        require(digest(destination) == digest(source) == row['sha256'] and destination.stat().st_size == row['bytes'], 'delivery copy changed bytes')
        access = policy(destination)
        require(access['owner'] == HUMAN, 'delivery file is not human-owned')
        copied.append({**row,'destination_policy':access})
    require(policy(DESTINATION.parent) == parent_policy, 'existing parent owner/ACL changed')
    require({p.relative_to(DESTINATION).as_posix() for p in DESTINATION.rglob('*') if p.is_file()}
            == {Path(row['destination']).relative_to(DESTINATION).as_posix() for row in copied}, 'unexpected delivery payload')
    report = {'schema':'airk.private.windows-candidate-delivery-census/1','utc':datetime.now(timezone.utc).isoformat(),
        'identity':identity,'script_sha256':digest(Path(__file__)),'source_commit':COMMIT,'original_clean_proof_sha256':digest(clean_proof),
        'current_candidate_git_status_porcelain':dirty,'current_candidate_is_not_relabelled_clean':True,
        'native_c7_source_unchanged':True,'destination':str(DESTINATION),'destination_parent_policy':parent_policy,
        'files':copied,'native_archive_entries':native_entries,'vsix_archive_entries':vsix_entries,'signatures':signatures,
        'profile':'debug','native_qualification_scope':n['qualification'],'vsix_qualification_scope':v['qualification'],
        'installer_manifest_policy':'EXE only: localdriver has no canonical public published installer roster; private receipt/signing proofs stay in original evidence, never copied.',
        'output_access_policy_note':'Public distributable copies inherit existing workspace access rules (including foreign Modify grants); no private fixture eligibility claimed and no ACL/ownership changed.',
        'copied_private_receipts_or_state':False,'model_execution_performed':False,'installation_performed':False,'publication_performed':False,'passed':True}
    with (EVIDENCE/'delivery-census-r1.json').open('x',encoding='utf-8') as stream:
        json.dump(report,stream,indent=2)
        stream.write('\n')
    print(json.dumps({'passed':True,'destination':str(DESTINATION),'files':len(copied),'native_entries':len(native_entries),'vsix_entries':len(vsix_entries),'census_sha256':digest(EVIDENCE/'delivery-census-r1.json')},indent=2))


if __name__ == '__main__':
    main()
