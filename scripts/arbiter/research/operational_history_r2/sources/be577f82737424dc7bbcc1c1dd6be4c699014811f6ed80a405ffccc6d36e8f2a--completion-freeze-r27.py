# SPDX-License-Identifier: Apache-2.0
"""Extend the retained freeze owner for the standalone distribution inputs."""
import hashlib
import importlib.util
import json
from pathlib import Path

path = Path(__file__).resolve()
spec = importlib.util.spec_from_file_location('r27_original_freeze', path.with_name('completion-freeze.py'))
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
original = owner.observe

def observe():
    value = original()
    value['groups']['harness'].update(owner.snapshot(owner.tracked(
        'installers', 'site', '.github/workflows/github-release.yml') + [path]))
    value['identities']['harness'] = {
        'sha256': hashlib.sha256(json.dumps(value['groups']['harness'], sort_keys=True,
            separators=(',', ':')).encode()).hexdigest(),
        'files': len(value['groups']['harness'])}
    return value

owner.observe = observe
owner.main()
