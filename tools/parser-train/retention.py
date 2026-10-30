# SPDX-License-Identifier: Apache-2.0
"""Keep immutable local training artifacts outside Cargo's disposable target tree."""
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / 'target/parser-train'
ARCHIVE = ROOT / '.parser-train'
CATEGORIES = ('data', 'vocabulary', 'models', 'reports', 'corpora', 'online')


def retain(file):
    file = Path(file)
    relative = file.resolve().relative_to(WORK.resolve())
    if relative.parts[0] not in CATEGORIES or file.is_symlink():
        raise ValueError('Only generated training artifacts may enter the local archive')
    if relative.parts[0] == 'corpora' and not file.name.endswith('.manifest.json'):
        raise ValueError('Source samples are excluded from the retained artifact archive')
    content = file.read_bytes()
    digest = hashlib.sha256(content).hexdigest()
    destination = ARCHIVE / digest / relative
    for parent in [ARCHIVE, ARCHIVE / digest, destination.parent]:
        parent.mkdir(parents=True, exist_ok=True)
        if parent.is_symlink():
            raise ValueError('Retained training directory is redirected')
    if destination.exists():
        if destination.is_symlink() or hashlib.sha256(destination.read_bytes()).hexdigest() != digest:
            raise ValueError('Existing retained artifact differs; it was preserved')
    else:
        with destination.open('xb') as stream:
            stream.write(content)
    return destination


if __name__ == '__main__':
    count = 0
    for category in CATEGORIES:
        for file in sorted((WORK / category).glob('*')):
            if file.is_file():
                retain(file)
                count += 1
    print(f'Retained {count} generated artifacts under ignored .parser-train/; clones and indexes excluded.')
