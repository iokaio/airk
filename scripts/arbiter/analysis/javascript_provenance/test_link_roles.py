# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('role_linker',Path(__file__).with_name('link_roles.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

class LinkRoles(unittest.TestCase):
    def test_first_party_label_boundaries(self):
        self.assertEqual(m.source_label('src/explorerWebview/main.tsx'),'packages/vscode/src/explorerWebview/main.tsx')
        self.assertEqual(m.source_label('../admin/src/main.tsx'),'packages/admin/src/main.tsx')
        self.assertEqual(m.source_label('../ui/src/management.ts'),'packages/ui/src/management.ts')
        for label in ['../../outside.ts','src/../../outside.ts','/etc/outside.js','packages/vendor/source.ts']:
            with self.assertRaises(ValueError):m.source_label(label)
    def test_vendor_package_not_authored_source(self):
        self.assertIsNone(m.source_label('node_modules/preact/dist/preact.module.js'))
        self.assertEqual(m.vendor_key('node_modules/preact/dist/preact.module.js'),'node_modules/preact')
        self.assertEqual(m.vendor_key('node_modules/@sigstore/verify/dist/index.js'),'node_modules/@sigstore/verify')
    def test_labels_require_whole_comment_line(self):
        self.assertEqual(m.LABEL.findall('// src/a.ts\n// node_modules/preact/a.js\n'),['src/a.ts','node_modules/preact/a.js'])
        self.assertEqual(m.LABEL.findall('const value="// src/a.ts";\n'),[])

if __name__=='__main__':unittest.main(verbosity=2)
