# SPDX-License-Identifier: Apache-2.0
"""Prepare, launch and collect one full Neo source snapshot using existing private SSH settings."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile

base = Path(__file__).resolve().parent
root = base.parents[1]
action, label = sys.argv[1:]
assert action in ('prepare', 'launch', 'status', 'collect')
assert re.fullmatch(r'upgrade-1[.]8[.]9-full-[a-z0-9-]+', label)
run = base / label
remote = '/Users/tylerjensen/airk-builds/' + label

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def save(name, value):
    (run / name).write_text(json.dumps(value, indent=2) + '\n')

git = ['git', '-c', 'safe.directory=' + root.as_posix()]
if action == 'prepare':
    run.mkdir()
    excluded = {'.git', '.codex', '.agents', '.aws', '.mcp.json', '.ssh', 'temp', 'target', 'artifacts', 'node_modules', '__pycache__'}
    raw = subprocess.check_output([*git, 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root)
    names = sorted({os.fsdecode(name) for name in raw.split(b'\0') if name and not (set(Path(os.fsdecode(name)).parts) & excluded)})
    files = {}
    with tarfile.open(run / 'source.tar.gz', 'x:gz') as archive:
        for name in names:
            path = root / name
            assert not path.is_symlink() and path.resolve().is_relative_to(root)
            if not path.is_file():
                continue
            files[name] = sha(path)
            archive.add(path, arcname=name, recursive=False)
    assert all(sha(root / name) == digest for name, digest in files.items())
    head = subprocess.check_output([*git, 'rev-parse', 'HEAD'], cwd=root).decode().strip()
    commit = subprocess.check_output([*git, 'cat-file', 'commit', head], cwd=root)
    tree = commit.splitlines()[0].decode().removeprefix('tree ')
    (run / 'head.commit').write_bytes(commit)
    (run / 'head.tree').write_bytes(subprocess.check_output([*git, 'cat-file', 'tree', tree], cwd=root))
    spec = importlib.util.spec_from_file_location('runtime_builder', root / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    save('snapshot.json', {'entry_commit': head, 'source_state': 'original branch HEAD metadata with explicit current live file hashes; uncommitted changes retained', 'native_source': builder.source_identity(), 'archive_sha256': sha(run / 'source.tar.gz'), 'files': files})
    save('plan.json', {'runner_arguments': [], 'version': '1.8.9', 'scope': 'Full workspace clippy/build/Nextest with four routine campaigns restored/doctests, matched debug runtime+clients, full extension+Python suites, notices and whitespace', 'incremental': 0, 'target_dir': remote + '/target', 'cleanup': 'Always preserve timings and cargo clean exact owned target after compiler descendants exit'})
    for name in ('runner.py', 'owner.py'):
        (run / name).write_bytes((base / name).read_bytes())
    print(json.dumps({'prepared': str(run), 'public_files': len(files), 'native_source': builder.source_identity(), 'archive_sha256': sha(run / 'source.tar.gz')}))
    sys.exit(0)

connection = json.loads((root / 'artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'))
peer = connection['user'] + '@' + connection['host']
ssh = ['ssh', '-i', connection['identity_path'], *connection['ssh_options'], peer]
scp = ['scp', '-i', connection['identity_path'], *connection['ssh_options']]
if action == 'launch':
    review_path = run / 'preparation-review.json'
    if review_path.is_file():
        review = json.loads(review_path.read_text())
        for name, expected in review['pins'].items():
            assert sha(run / name) == expected, 'Prepared batch artifact changed: ' + name
        spec = importlib.util.spec_from_file_location('runtime_builder', root / 'scripts/build-service-runtime.py')
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        assert builder.source_identity() == review['native_source'], 'Live native source changed after freeze'
        for name, expected in review['extension_and_packaging_source']['inputs'].items():
            assert sha(root / name) == expected, 'Live extension/packaging source changed after freeze: ' + name
    subprocess.run([*ssh, 'umask 077; mkdir ' + remote], check=True)
    transfer = ['source.tar.gz', 'snapshot.json', 'plan.json', 'runner.py', 'owner.py', 'head.commit', 'head.tree']
    transfer.extend(name for name in ('baseline.json', 'selection.json') if (run / name).is_file())
    for name in transfer:
        subprocess.run([*scp, str(run / name), peer + ':' + remote + '/' + name], check=True)
    subprocess.run([*ssh, 'cd ' + remote + ' && nohup /Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13 owner.py > owner.log 2>&1 < /dev/null &'], check=True)
    save('launch.json', {'remote': remote, 'launched': True})
elif action == 'status':
    subprocess.run([*ssh, 'cat ' + remote + '/phase.txt 2>/dev/null; tail -8 ' + remote + '/owner.log; test ! -f ' + remote + '/cleanup.json || cat ' + remote + '/cleanup.json'], check=True)
else:
    # Download every available report, log, JUnit and timing before asserting success.
    subprocess.run([*scp, '-r', peer + ':' + remote + '/cargo-timings', str(run)], check=False)
    for pattern in ('*.json', '*.log', '*.xml'):
        subprocess.run([*scp, peer + ':' + remote + '/' + pattern, str(run)], check=False)
    runtime = remote + '/runtime-debug'
    if (run / 'runtime-reuse.json').is_file():
        runtime = json.loads((run / 'runtime-reuse.json').read_text())['original_runtime']
        assert runtime == '/Users/tylerjensen/airk-builds/upgrade-1.8.9-full-r5/runtime-debug'
    for name in ('airk-build.json', 'airk-runtime.json', 'airk-install.json'):
        subprocess.run([*scp, peer + ':' + runtime + '/' + name, str(run / ('runtime-' + name))], check=False)
    # These reports are explicitly bounded test failure envelopes/transport
    # diagnostics, never fixture source, private indexes, control state or logs.
    subprocess.run([*scp, '-r', peer + ':' + remote + '/native-diagnostics', str(run)], check=False)
    cleanup = json.loads((run / 'cleanup.json').read_text())
    print(json.dumps(cleanup))
    assert cleanup['group_absent'] and cleanup['cargo_clean_exit_code'] == 0 and cleanup['target_removed']
    subprocess.run([*ssh, 'test ! -e ' + remote + '/target'], check=True)
    assert cleanup['test_exit_code'] == 0, 'Failure evidence retained; full qualification did not pass'
