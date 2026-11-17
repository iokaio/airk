# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, subprocess
from pathlib import Path
root = Path('D:/code/Github/airk')
base = root / 'artifacts/v190'
work = base / 'p10-worktree'
accepted = 'c2584cb05dfdeba6e948b91e4964479a07f9d3fc'
git = ['git', '-c', 'safe.directory=' + work.as_posix(), '-c', 'core.autocrlf=true', '-C', str(work)]
assert subprocess.check_output([*git, 'branch', '--show-current']).decode().strip() != 'feat/finetuning'
def inventory():
    names = subprocess.check_output([*git, 'ls-files', '--cached', '--others', '--exclude-standard', '-z'])
    return {name: hashlib.sha256((work / name).read_bytes()).hexdigest()
        for name in names.decode().strip('\0').split('\0') if name}
before = inventory()
subprocess.run([*git, 'reset', '--mixed', '--quiet', accepted], check=True)
after = inventory()
assert before == after
subprocess.run([*git, 'diff', '--quiet'], check=True)
spec = importlib.util.spec_from_file_location('builder', work / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
identity = builder.source_identity()
assert identity == {'sha256':'fadf30fbd45c92ccca0c384eb10e4802f12d1e7af47d07741c251d140c4e45f5', 'files':945}
receipt = base / 'p11-reanchor-r1.json'
assert not receipt.exists()
receipt.write_text(json.dumps({'head':accepted, 'files':len(before), 'all_public_bytes_preserved':True,
    'source':identity, 'target':'Persistent p10-target and same worktree path retained'}, indent=2)+'\n')
print('Reanchored accepted P10; all public bytes preserved; existing Cargo path/target retained')
