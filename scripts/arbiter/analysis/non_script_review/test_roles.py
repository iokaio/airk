# SPDX-License-Identifier: Apache-2.0
"""Pure positive-role and refusal controls; no historical program execution."""
import unittest
import roles

class RoleTests(unittest.TestCase):
    def test_exact_synthetic_source_not_arbitrary_same_name(self):
        path='artifacts/editor-qualification/example/workspace-a/fixture.ts'
        body=b'export function WindowAlphaEvidence() { return 42; }\n'
        self.assertEqual(roles.role(path,body)[0],'generated_indexed_fixture')
        self.assertIsNone(roles.role(path,body+b'private extra content'))
        self.assertIsNone(roles.role(path,body.replace(b'42',b'43')))
    def test_stub_requires_source_semantics_not_name(self):
        path='artifacts/advanced-split/installer-wrapper-r1/a/stub.cs'
        self.assertIsNone(roles.role(path,b'class Stub {}'))
        self.assertEqual(roles.role(path,b'class Stub __airk_role observed-operation.json inert')[0],'generated_inert_installer_payload')
    def test_generated_asset_table_boundary(self):
        body=b'pub type EmbeddedProfileFiles; pub static PROFILES;'
        self.assertEqual(roles.role('artifacts/a/build/airk-host-x/out/bundled_model.rs',body)[0],'generated_native_embedded_asset_table')
        self.assertIsNone(roles.role('artifacts/a/handwritten/bundled_model.rs',body))
    def test_measurement_fixture_exact_all_rows(self):
        body=''.join(f'/// Original measurement fixture.\npub fn fixture_0_{i}(value: usize) -> usize {{ value + {i} }}\n' for i in range(256)).encode()
        path='artifacts/readiness-1817/host-fixtures/example/fixture_0.rs'
        self.assertEqual(roles.role(path,body)[1],'crates/airk-kernel/src/usage.rs')
        self.assertIsNone(roles.role(path,body[:-1]));self.assertIsNone(roles.role(path.replace('fixture_0','fixture_32'),body))
    def test_indexed_detail_role_requires_literal_hash(self):
        path='artifacts/detail-harness-fixtures/one/one.rs'
        raw=b'pub fn amber_unique_probe() { other(); }\r\n'
        self.assertEqual(roles.role(path,raw)[0],'observed_indexed_harness_input')
        self.assertIsNone(roles.role(path,raw+b'private extra'))
    def test_unknown_file_stays_unresolved(self):
        self.assertIsNone(roles.role('artifacts/unknown/source.rs',b'fn unknown(){}'))

if __name__=='__main__':unittest.main(verbosity=2)
