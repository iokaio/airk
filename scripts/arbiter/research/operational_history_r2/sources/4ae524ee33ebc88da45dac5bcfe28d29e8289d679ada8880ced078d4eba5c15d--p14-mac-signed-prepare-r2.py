# SPDX-License-Identifier: Apache-2.0
"""Prepare a current quota candidate signed Mac batch from immutable existing public archive. No SSH."""
import argparse, ast, hashlib, json, subprocess
from pathlib import Path

BASE = Path(__file__).resolve().parent
EXPECTED = {'sha256': '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11', 'files': 960}
SEED = 'p14-native-quota-r1'
PYTHON = '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13'
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def read(p): return json.loads(Path(p).read_text(encoding='utf-8'))
def write(p, data):
    with Path(p).open('xb') as output: output.write(data)

def prepare(label, minimum_free_bytes=6 << 30):
    import re
    assert re.fullmatch(r'p14-native-signed-quota-[a-z0-9-]+', label)
    output = BASE / (label + '-signed-prepared.json')
    if output.exists():
        result = read(output)
        assert result['minimum_free_bytes'] == minimum_free_bytes
        for path, digest in result['pins'].items(): assert sha(path) == digest, path
        return result
    snapshot = read(BASE / (SEED + '-snapshot.json'))
    archive = BASE / (SEED + '.tar.gz')
    assert snapshot['native_source'] == EXPECTED and sha(archive) == snapshot['archive_sha256']
    # Rehydrate the real accepted commit object, not a newly authored fixture commit.
    git = ['git', '-c', 'safe.directory=' + BASE.parents[1].as_posix()]
    commit = subprocess.check_output([*git, 'cat-file', 'commit', snapshot['entry_commit']], cwd=BASE.parents[1])
    assert hashlib.sha1(b'commit ' + str(len(commit)).encode() + b'\0' + commit).hexdigest() == snapshot['entry_commit']
    tree_id = commit.splitlines()[0].decode().removeprefix('tree ')
    tree = subprocess.check_output([*git, 'cat-file', 'tree', tree_id], cwd=BASE.parents[1])
    assert hashlib.sha1(b'tree ' + str(len(tree)).encode() + b'\0' + tree).hexdigest() == tree_id
    runner = (BASE / (SEED + '-runner.py')).read_text(encoding='utf-8')
    anchor = "    subprocess.run(['git', 'add', '--all'], cwd=source, check=True)"
    assert runner.count(anchor) == 1
    runner = runner.replace(anchor, anchor + "\n    # Exact accepted commit/tree metadata only; dirty quota candidate snapshot remains independently bound.\n    original_commit = (run / 'head.commit').read_bytes()\n    original_tree = (run / 'head.tree').read_bytes()\n    expected_tree = original_commit.splitlines()[0].decode().removeprefix('tree ')\n    actual_tree = subprocess.check_output(['git', 'hash-object', '-t', 'tree', '-w', '--stdin'], input=original_tree, cwd=source).decode().strip()\n    assert actual_tree == expected_tree\n    actual_commit = subprocess.check_output(['git', 'hash-object', '-t', 'commit', '-w', '--stdin'], input=original_commit, cwd=source).decode().strip()\n    assert actual_commit == record['entry_commit']\n    subprocess.run(['git', 'update-ref', 'HEAD', actual_commit], cwd=source, check=True)\n    assert subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=source).decode().strip() == expected_tree")
    ast.parse(runner)
    plan = read(BASE / (SEED + '-plan.json'))
    assert len(plan['steps']) == 5 and sum(row.get('expected_passing_tests', 0) for row in plan['steps']) == 5
    plan['steps'].append({'name': 'fresh-release', 'command': [PYTHON, 'scripts/build-service-runtime.py', '--profile', 'release', '--destination', '{source}/artifacts/p14-mac-release-r1']})
    plan['not_run'] = ['Candidate fmt+four native selections/five tests then fresh Mac release producer here; Developer ID/copied launcher/installed lifecycle are separately recorded by signed owner. No notarization/editor/platform performance claim.']
    run = '/Users/tylerjensen/airk-builds/' + label
    inputs = '/Users/tylerjensen/airk-builds/cargo-clean-69d305e3933f/inputs'
    tools = '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools'
    assets = {'snapshot.json': BASE / (SEED + '-snapshot.json'), 'source.tar.gz': archive,
              'owner.py': BASE / 'p14-mac-signed-owner-r2.py'}
    generated = {'runner.py': runner.encode(), 'plan.json': (json.dumps(plan, indent=2) + '\n').encode(), 'head.commit': commit, 'head.tree': tree}
    for name, data in generated.items():
        path = BASE / (label + '-' + name)
        write(path, data); assets[name] = path
    config = {'source': EXPECTED, 'minimum_free_bytes': minimum_free_bytes,
              'keychain_helper': '/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/artifacts/p6a-keychain-status',
              'environment': {'CARGO_BUILD_JOBS': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_NET_OFFLINE': 'true',
                  'CARGO_TARGET_DIR': run + '/target', 'MACOSX_DEPLOYMENT_TARGET': '13.3', 'PYTHONUTF8': '1',
                  'GIT_CONFIG_GLOBAL': run + '/private-census/owned-ignore-controls/global.gitconfig',
                  'AIRK_BUNDLED_MODEL_DIR': inputs + '/local-vector-model',
                  'AIRK_BUNDLED_RUNTIME_DIR': inputs + '/local-onnx-runtime',
                  'AIRK_TEST_MODEL_DIR': inputs + '/local-vector-model/minilm-l6-v2-s8-v1',
                  'AIRK_PRIVATE_CENSUS_BASE': run + '/private-census'},
              'path_prefix': [tools, tools + '/node-v24.15.0-darwin-arm64/bin', '/Users/tylerjensen/.cargo/bin'],
              'transferred_hashes': {name: sha(path) for name, path in assets.items()}}
    config_path = BASE / (label + '-sign-config.template.json')
    write(config_path, (json.dumps(config, indent=2) + '\n').encode())
    pins = {str(p): sha(p) for p in [*assets.values(), config_path, Path(__file__), BASE / 'p14-mac-signed-control-r2.py']}
    result = {'prepared_only': True, 'label': label, 'remote': run, 'source': EXPECTED,
              'minimum_free_bytes': minimum_free_bytes, 'assets': {k: str(v) for k, v in assets.items()},
              'config_template': str(config_path), 'pins': pins,
              'source_commit_basis': 'Exact existing accepted commit object restored; no new commit/signature/author. Native full quota candidate snapshot source independently verified.',
              'cleanup': 'Always cargo clean exact new target after build and again in outer finally; source/runtime/signing evidence retained.'}
    write(output, (json.dumps(result, indent=2) + '\n').encode())
    return result

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument('--label', default='p14-native-signed-quota-r1')
    args = parser.parse_args(); print(json.dumps(prepare(args.label), indent=2))
