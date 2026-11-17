# SPDX-License-Identifier: Apache-2.0
"""Use the existing authenticated loader; no artifact helper execution."""
import importlib.util
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[4]
HELPER_ROOT = REPOSITORY / 'scripts/cuda-native-execution/helpers'
spec = importlib.util.spec_from_file_location('independent_numeric_native_loader', REPOSITORY / 'scripts/cuda-native-execution.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
parent, owner = native.helpers({'ownership_helper_root': str(HELPER_ROOT)})
worker = parent.worker

def private_root(path):
    path = Path(path)
    resolved = path.resolve(strict=True)
    if resolved != path.absolute() or not resolved.is_dir() or not resolved.is_relative_to(REPOSITORY / 'artifacts'):
        raise ValueError('existing canonical private artifact directory required')
    for part in [resolved, *resolved.parents]:
        if part.is_symlink() or getattr(part.lstat(), 'st_file_attributes', 0) & 0x400:
            raise ValueError('aliased private root refused')
    return resolved
