# SPDX-License-Identifier: Apache-2.0
import hashlib, json, shutil, subprocess, sys
from pathlib import Path
root = Path(__file__).resolve().parents[2]; base = Path(__file__).resolve().parent
out = base / 'windows-r1'; manual = root / 'artifacts/manual-test-1.8.8-branch-review-r1'
def sha(path):
    with path.open('rb') as stream: return hashlib.file_digest(stream, 'sha256').hexdigest()
def read(path): return json.loads(path.read_text(encoding='utf-8'))
commit = read(base / 'local-commit-r1.json')
native_manifest = out / 'native/publish/manifest.json'
extension_manifest = out / 'extension/publish/manifest.json'
extension = read(extension_manifest)
assert extension['archive'] == 'airk-1.8.8-win32-x64.vsix'
source = extension_manifest.parent / extension['archive']
assert sha(source) == extension['archive_sha256']
assert manual.is_dir() and manual.resolve().is_relative_to(root / 'artifacts')
destination = manual / source.name
assert not destination.exists()
with source.open('rb') as incoming, destination.open('xb') as outgoing:
    shutil.copyfileobj(incoming, outgoing, 4 << 20)
assert sha(destination) == extension['archive_sha256']
pins = base / 'final-vsix-source-pins-r1.json'
assert sha(pins) == 'f93b8196e936e073ad86a87252cb97f3b03d9581aaf35c6a5803e1b518216d77'
runtime = out / 'release-runtime'
seal = read(out / 'prepackage-runtime-seal-r1.json')
assert sha(runtime / 'airk-build.json') == seal['airk_build_sha256']
report_path = base / 'final-vsix-verification-r1.json'
command = [sys.executable, '-B', '-X', 'utf8', str(base / 'verify-final-vsix-r1.py'),
    '--root', str(root), '--runtime', str(runtime), '--runtime-build-sha256', seal['airk_build_sha256'],
    '--native-manifest', str(native_manifest), '--native-manifest-sha256', sha(native_manifest),
    '--extension-manifest', str(extension_manifest), '--extension-manifest-sha256', sha(extension_manifest),
    '--manual-copy', str(destination), '--source-pins', str(pins), '--source-pins-sha256', sha(pins),
    '--commit', commit['commit'], '--tree', commit['tree'], '--output', str(report_path)]
subprocess.run(command, cwd=root, check=True)
report = read(report_path)
assert report['status'] == 'passed' and report['vsix_sha256'] == sha(destination)
with (manual / 'SHA256SUMS.txt').open('x', encoding='utf-8', newline='\n') as stream:
    stream.write(report['vsix_sha256'] + '  ' + destination.name + '\n')
with report_path.open('rb') as incoming, (manual / 'verification.json').open('xb') as outgoing:
    shutil.copyfileobj(incoming, outgoing)
readme = manual / 'README.txt'
text = readme.read_text(encoding='utf-8')
old = 'The VSIX and SHA256SUMS.txt will be placed beside this file after validation.'
assert text.count(old) == 1
text = text.replace(old, 'This folder contains the verified VSIX, SHA256SUMS.txt and verification.json.')
text += '\nBuild identity\nCommit: ' + commit['commit'] + '\nNative source: ' + commit['native_source']['sha256']
text += '\nVSIX SHA256: ' + report['vsix_sha256']
text += '\n\nValidation\nWindows: 2681 native tests passed (51 ignored); Node 830 passed/4 skipped; Python 471 passed/9 skipped.\n'
text += 'Formatting, Clippy, lightweight/main/workspace builds, actual client/model and isolated install gates passed.\n'
text += 'Mac Neo: 37 selected native and 15 Node tests passed on the recorded earlier production-equivalent source; cargo clean completed.\n'
text += 'Package byte verification covers 675 runtime files and 77 compiled production outputs. Manual acceptance on another machine remains yours to perform.\n'
readme.write_text(text, encoding='utf-8', newline='\n')
print(json.dumps({'status': 'ready_for_manual_test', 'vsix': str(destination), 'bytes': destination.stat().st_size,
    'sha256': report['vsix_sha256'], 'commit': commit['commit'], 'manual_files': sorted(p.name for p in manual.iterdir())}))
