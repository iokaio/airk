# SPDX-License-Identifier: Apache-2.0
import hashlib, json, os
from pathlib import Path
import stat, subprocess, time, zipfile

owner = Path.cwd() / 'artifacts/mcp-optimizations-1.8.3'
download = owner / 'baseline-ci-artifact'
artifact = json.loads((owner/'baseline-artifact-api.json').read_text())
run = json.loads((owner/'baseline-run-api.json').read_text())
manifest = json.loads((download/'manifest.json').read_text())
def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
assert artifact['digest'] == 'sha256:' + sha(owner/'baseline-actions-artifact.zip')
assert run['id'] == artifact['workflow_run']['id'] == 41326382859
assert run['conclusion'] == 'success' and run['event'] == 'pull_request' and run['run_attempt'] == 1
assert run['repository']['full_name'] == 'iokaio/airk'
assert artifact['id'] == 13091723729 and not artifact['expired']
assert manifest['tree'] == json.loads((owner/'baseline-main-commit-api.json').read_text())['tree']['sha'] == json.loads((owner/'baseline-merge-ref-api.json').read_text())['tree']['sha'] == '810545d5021c6d12496222f0b1b57db798e038b9'
assert manifest['commit'] == '539139af0ff0c0d1305059e5d7393c7777e1b4f5'
assert manifest['version'] == '1.8.2' and manifest['dirty_checkout'] is False
assert sha(download/manifest['archive']) == manifest['archive_sha256']
payload = owner/'baseline-runtime'
with zipfile.ZipFile(download/manifest['archive']) as archive:
    assert set(archive.namelist()) == set(manifest['payload'])
    for info in archive.infolist():
        assert not info.is_dir() and not stat.S_ISLNK(info.external_attr >> 16)
        name=Path(info.filename)
        assert not name.is_absolute() and '..' not in name.parts and ':' not in info.filename
        assert hashlib.sha256(archive.read(info)).hexdigest() == manifest['payload'][info.filename]
    installation = json.loads(archive.read('airk-install.json'))
    assert hashlib.sha256(archive.read('airk-install.json')).hexdigest() == manifest['install_manifest_sha256']
    assert installation['version'] == '1.8.2' and installation['schema'] == 1
    for name, digest in installation['files'].items():
        assert hashlib.sha256(archive.read(name)).hexdigest() == digest
    if not payload.exists():
        payload.mkdir()
        archive.extractall(payload)
for name, digest in manifest['payload'].items():
    assert sha(payload/name) == digest
receipt = {'schema':'airk.mcp-baseline-provenance/1','artifact_id':artifact['id'],'artifact_server_digest':artifact['digest'],
    'ci_run':run['id'],'ci_attempt':run['run_attempt'],'ci_conclusion':run['conclusion'],'ci_head':run['head_sha'],
    'candidate_commit':manifest['commit'],'merged_commit':'44cce94dc296d47c4d9e2e7009f19500b0bf7f6f','tree':manifest['tree'],
    'source':manifest['source'],'native_archive_sha256':manifest['archive_sha256'],
    'install_manifest_sha256':manifest['install_manifest_sha256'],'main_sha256':installation['files']['airk.exe'],
    'helper_sha256':installation['files']['airk-embed.exe'],'qualification':'Passing PR CI unsigned candidate; not published artifact qualification.'}
(owner/'baseline-provenance.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt),flush=True)

environment = dict(os.environ, HTTP_PROXY='http://127.0.0.1:1',HTTPS_PROXY='http://127.0.0.1:1',ALL_PROXY='http://127.0.0.1:1')
binary = payload/'airk.exe'
install = owner/'baseline-install'
state = owner/'baseline-state'
data = owner/'baseline-project-data'
source = owner/'frozen-fixture/source'
assert source.resolve().is_relative_to(owner.resolve())
base = ['--service-install-dir',str(install),'--root',str(source),'--data-dir',str(data),'--collection','mcp-optimization-fixture']
def invoke(label,*arguments,timeout=180):
    result = subprocess.run([str(binary), *map(str,arguments)],cwd=owner,env=environment,capture_output=True,
        encoding='utf-8',timeout=timeout,creationflags=subprocess.CREATE_NO_WINDOW)
    (owner/f'baseline-{label}.stdout.json').write_text(result.stdout,encoding='utf-8')
    (owner/f'baseline-{label}.stderr.log').write_text(result.stderr,encoding='utf-8')
    if result.returncode:
        raise RuntimeError(f'{label} failed: {result.stderr[-2000:]}')
    value=json.loads(result.stdout)
    print(label,json.dumps(value)[:4000],flush=True)
    return value
if not (install/'managed.json').exists() and not (install/'selected.json').exists():
    invoke('managed','runtime','managed','--source',payload,'--install-dir',install,'--state-dir',state,
        '--approved-sha256',manifest['install_manifest_sha256'],'--verification-basis','organization-digest','--lifecycle','user-on-demand')
invoke('prepare-client','runtime','prepare-client','--install-dir',install)
invoke('inspect','runtime','inspect','--install-dir',install)
registration = invoke('init',*base,'init')
invoke('model-setup',*base,'setup-model')
invoke('index',*base,'index')
print('Private baseline installed and index accepted. Readiness observation is a separate step.',flush=True)
