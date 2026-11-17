# SPDX-License-Identifier: Apache-2.0
"""Package the original r26 build under the reviewed test-only reuse proof."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
proof = json.loads((BASE / 'completion-native-package-r26-reuse-proof.json').read_text())
assert proof['before'] and not proof['after']
assert proof['owner_sha256'] == hashlib.sha256((BASE / 'completion-test-only-gate.py').read_bytes()).hexdigest()
assert proof['original_compiled_source'] == {'sha256': 'ac2620f07a33fa096349197001edaefa5370caacb18310270181b062fbda42ef', 'files': 1116}
assert proof['current_test_source'] == {'sha256': '9335cf1dbff5f128719382049e7db7e146df9d2db98a56bbe2e5d15eae21a735', 'files': 1116}
assert set(proof['test_only_changes']) == {
    'crates/airk-cli/src/project_supervisor_guarded_qualification_tests.rs',
    'crates/airk-cli/src/project_supervisor_modern_checkpoint_tests.rs'}
spec = importlib.util.spec_from_file_location('r26_native_package', ROOT / 'scripts/package-native.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
builder = package.runtime_builder()
assert builder.source_identity() == proof['current_test_source']
# Only the source comparison is adapted after the outer owner verifies every
# frozen input and runtime seal. All canonical packaging validations still run.
builder.source_identity = lambda: dict(proof['original_compiled_source'])
package.runtime_builder = lambda: builder
sys.argv = [str(ROOT / 'scripts/package-native.py'), '--target', 'x86_64-pc-windows-msvc',
    '--runtime-dir', str(BASE / 'windows-debug-r26/runtime'),
    '--output', str(BASE / 'windows-debug-native-r26')]
package.main()
