# SPDX-License-Identifier: Apache-2.0
"""Reuse the established freeze inventory with the current candidate version."""
import importlib.util
from pathlib import Path
import tomllib

path = Path(__file__).resolve()
spec = importlib.util.spec_from_file_location('r30_freeze', path.with_name('completion-freeze.py'))
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
original = owner.observe


def observe():
    value = original()
    value['version'] = tomllib.loads((owner.ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    for name, paths in (
        ('harness', owner.tracked('installers', 'site', '.github/workflows/github-release.yml') + [path]),
    ):
        value['groups'][name].update(owner.snapshot(paths))
        import hashlib
        import json
        files = value['groups'][name]
        value['identities'][name] = {
            'sha256': hashlib.sha256(json.dumps(files, sort_keys=True, separators=(',', ':')).encode()).hexdigest(),
            'files': len(files),
        }
    return value


owner.observe = observe
owner.main()
