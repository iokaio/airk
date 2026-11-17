# SPDX-License-Identifier: Apache-2.0
"""Pure preconditions from the actual R4 wrapper, no native process/network."""
from pathlib import Path
import ast
import os
import tempfile
import unittest

source=Path(__file__).with_name('capture_with_catalog_r4.py')
tree=ast.parse(source.read_text(encoding='utf-8'))
function=next(item for item in tree.body if isinstance(item,ast.FunctionDef) and item.name=='prepare_owned_paths')
namespace={'Path':Path,'os':os}
exec(compile(ast.Module(body=[function],type_ignores=[]),str(source),'exec'),namespace)
prepare=namespace['prepare_owned_paths']

class Preflight(unittest.TestCase):
    def test_only_sources_and_data_exist_native_state_control_remain_absent(self):
        with tempfile.TemporaryDirectory(prefix='airk-i3-path-proof-') as path:
            owner=Path(path).resolve();roots,data,control=prepare(owner)
            self.assertEqual(control,owner/'state/control')
            self.assertTrue(all(p.is_dir() for p in [*roots,data]))
            self.assertFalse((owner/'state').exists())
            self.assertFalse(control.exists())
            self.assertEqual({p.name for p in owner.iterdir()},{'source-a','source-b','data'})
            for a in [*roots,data]:
                self.assertFalse(a.is_relative_to(control));self.assertFalse(control.is_relative_to(a))

    def test_unknown_nonempty_state_is_refused_without_repair_or_other_creation(self):
        with tempfile.TemporaryDirectory(prefix='airk-i3-foreign-state-') as path:
            owner=Path(path).resolve();control=owner/'state/control';control.mkdir(parents=True)
            sentinel=control/'foreign';sentinel.write_bytes(b'preserve exact unknown state')
            with self.assertRaisesRegex(ValueError,'must be absent'):prepare(owner)
            self.assertEqual(sentinel.read_bytes(),b'preserve exact unknown state')
            self.assertFalse((owner/'data').exists());self.assertFalse((owner/'source-a').exists())

if __name__=='__main__':unittest.main()
