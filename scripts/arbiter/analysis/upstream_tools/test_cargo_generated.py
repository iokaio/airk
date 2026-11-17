# SPDX-License-Identifier: Apache-2.0
"""Pure declarative producer/input comparison controls; no Cargo or vendor execution."""
import unittest
import project_cargo_generated as owner

class CargoRoles(unittest.TestCase):
    def test_private_literal_version_and_escape_refusal(self):
        source=b'const PRIVATE: &str = "\\\nmarker$$\n"; PRIVATE.replace("$$", &patch_version); out_dir.join("private.rs")'
        self.assertEqual(owner.private_template(source,'2.4.9'),b'marker9\n')
        with self.assertRaises(ValueError):owner.private_template(source.replace(b'marker',b'\\nmarker'),'2.4.9')
    def test_flag_literal_brace_decoding(self):
        source=b'fn ensure_check_file() { out_dir.join("flag_check.c"); write!(f, "body{{ok}}") } fn is_flag_supported_inner() {}'
        self.assertEqual(owner.flag_template(source),b'body{ok}')
        with self.assertRaises(ValueError):owner.flag_template(source.replace(b'flag_check.c',b'other.c'))
    def test_oid_table_covers_trailing_identifier_space_and_refuses_missing_rows(self):
        database=b'feature\tNAME \t1.2.3\tshort\tdescription\n'
        raw=b"pub const NAME : Oid<'static> = oid!(1.2.3);\npub fn with_feature(mut self)\nself.insert(\n"
        producer=b'fn generate_file() { File::create(&dest_path) }'
        result=owner.oid_role(raw,database,producer)
        self.assertEqual(result,{'named_constants':1,'input_rows':1,'features':1})
        with self.assertRaises(ValueError):owner.oid_role(raw.replace(b'1.2.3',b'1.2.4'),database,producer)

if __name__=='__main__':unittest.main(verbosity=2)
