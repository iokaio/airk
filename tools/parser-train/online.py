# SPDX-License-Identifier: Apache-2.0
"""Acquire small pinned public query datasets; raw downloads never enter Git."""
import hashlib
import json
from pathlib import Path
import urllib.request

from retention import ROOT, WORK, retain

HERE = Path(__file__).parent
SOURCES = [
    {'id': 'codesearchnet-queries', 'repository': 'github/CodeSearchNet', 'branch': 'master',
     'license': 'MIT', 'files': ['LICENSE', 'README.md', 'resources/queries.csv'],
     'attribution': 'Husain, Wu, Gazit, Allamanis and Brockschmidt (2019), CodeSearchNet Challenge; GitHub.'},
    {'id': 'clinc150', 'repository': 'clinc/oos-eval', 'branch': 'master',
     'license': 'CC-BY-3.0', 'files': ['LICENSE', 'README.md', 'data/data_full.json'],
     'attribution': 'Larson et al. (2019), An Evaluation Dataset for Intent Classification and Out-of-Scope Prediction, EMNLP-IJCNLP.'},
]


def fetch(url):
    request = urllib.request.Request(url, headers={'User-Agent': 'AIRK-offline-training-input-acquisition'})
    with urllib.request.urlopen(request, timeout=90) as response:
        content = response.read(16 * 1024 * 1024 + 1)
    if len(content) > 16 * 1024 * 1024:
        raise ValueError('Online training input exceeds 16 MiB limit')
    return content


def main():
    lock_file = HERE / 'online.lock.json'
    prior = json.loads(lock_file.read_text(encoding='utf-8')) if lock_file.exists() else {'sources': []}
    pins = {source['id']: source for source in prior['sources']}
    sources = []
    for source in SOURCES:
        revision = pins.get(source['id'], {}).get('revision')
        if not revision:
            revision = json.loads(fetch(f"https://api.github.com/repos/{source['repository']}/commits/{source['branch']}"))['sha']
        if len(revision) != 40 or any(c not in '0123456789abcdef' for c in revision):
            raise ValueError('Invalid public input revision')
        files = []
        for name in source['files']:
            url = f"https://raw.githubusercontent.com/{source['repository']}/{revision}/{name}"
            destination = WORK / 'online' / (source['id'] + '-' + revision[:12]) / name
            expected = next((f['sha256'] for f in pins.get(source['id'], {}).get('inputs', []) if f['path'] == name), None)
            content = destination.read_bytes() if destination.exists() else fetch(url)
            digest = hashlib.sha256(content).hexdigest()
            if expected and digest != expected:
                raise ValueError('Pinned online input changed; existing input was preserved')
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists():
                with destination.open('xb') as stream:
                    stream.write(content)
            retain(destination)
            files.append({'path': name, 'url': url, 'sha256': digest, 'bytes': len(content),
                          'local_path': destination.relative_to(ROOT).as_posix()})
        sources.append({**source, 'revision': revision, 'inputs': files})
        print(f"Pinned {source['id']}@{revision}: {source['license']}; {len(files)} inputs retained", flush=True)
    lock_file.write_text(json.dumps({'schema': 1, 'sources': sources}, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
