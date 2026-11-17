# SPDX-License-Identifier: Apache-2.0
"""One authorized accepted activation resume; no force cleanup or identity changes."""
import hashlib, json, pathlib, subprocess, time
base = pathlib.Path(__file__).resolve().parent
new = base / 'windows-debug-r8' / 'runtime'
cli = new / 'airk.exe'
expected_new = '3de8a00951667fb8bfd097991f795c0f2aba0c4238e7e0581abc789867da52fc'
expected_cli = 'f9cdb76fab4c5d58b9033d101645bf09a592cb830e946279b97480aba03249d8'
candidate = '7249f2608dc246f58f90d5ad7e03cc98aada3d7f32e5fc4ae20bef0ffe9bcd06'
predecessor = 'be73a42fb700d058dd4b2719a672d82c7a82ad9e0e5eb527529c2f23ac4ae30a'
operation = 'c78d6731052b652b74fbfee0f53db87b'
def digest(path):
    with path.open('rb') as source: return hashlib.file_digest(source,'sha256').hexdigest()
assert digest(new/'airk-install.json') == expected_new
assert digest(cli) == expected_cli
receipt = json.loads((base/'windows-debug-schema1-upgrade-result-r12.json').read_text(encoding='utf-8-sig'))
fixture = pathlib.Path(receipt['fixture_root']).resolve(strict=True)
install = fixture/'install'
before = json.loads((install/'activation.json').read_text())
assert before['phase'] == 'drain' and before['candidate'] == candidate and before['checkpoint'] == operation
assert json.loads((install/'selected.json').read_text())['active'] == predecessor
for identity in [candidate,predecessor]:
    payload = install/'runtimes'/identity
    assert digest(payload/'airk-install.json') == identity
    manifest = json.loads((payload/'airk-install.json').read_text())
    assert digest(payload/'airk.exe') == manifest['files']['airk.exe']
args = [str(cli),'runtime','activate','--install-dir',str(install),'--runtime-sha256',candidate]
out = base/'preserved-schema1-r12-recovery.json'
assert not out.exists(), 'Refusing duplicate recovery attempt/evidence overwrite'
result = {'schema':1,'operation':operation,'fixture':str(fixture),'new_cli_sha256':expected_cli,
          'selected_before':predecessor,'accepted_candidate':candidate,'argv':args,'activation_before':before}
out.write_text(json.dumps(result,indent=2)+'\n')
started = time.monotonic()
child = subprocess.Popen(args,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                         creationflags=subprocess.CREATE_NO_WINDOW)
try:
    stdout, stderr = child.communicate(timeout=420)
except subprocess.TimeoutExpired:
    # Do not kill an accepted activation coordinator in the middle of a receipt
    # transition. Record this exact owned child; parent must diagnose it.
    result.update(outcome='timeout_unknown',child_pid=child.pid,elapsed_seconds=time.monotonic()-started)
    out.write_text(json.dumps(result,indent=2)+'\n')
    raise
assert len(stdout)+len(stderr) <= 16*1024*1024
(base/'preserved-schema1-r12-recovery.stdout').write_bytes(stdout)
(base/'preserved-schema1-r12-recovery.stderr').write_bytes(stderr)
result.update(exit_code=child.returncode,elapsed_seconds=time.monotonic()-started,
              stdout_sha256=hashlib.sha256(stdout).hexdigest(),stderr_sha256=hashlib.sha256(stderr).hexdigest(),
              selected_after=json.loads((install/'selected.json').read_text()),
              activation_pending=(install/'activation.json').exists())
out.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
