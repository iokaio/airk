# SPDX-License-Identifier: Apache-2.0
import unittest,json
from native_json_projection import compact_literal,array_member
class MechanicalProjection(unittest.TestCase):
 def test_spaces_inside_string_are_preserved(self):self.assertEqual(compact_literal(b' [ { "note" : "a b  c" } ] '),b'[{"note":"a b  c"}]')
 def test_escapes_and_native_integer_lexemes_are_preserved(self):
  raw=rb' [ {"quoted":"a \" b", "slash":"a\\b", "unicode":"\u00e9", "bytes":18446744073709551615} ] '
  expected=rb'[{"quoted":"a \" b","slash":"a\\b","unicode":"\u00e9","bytes":18446744073709551615}]'
  self.assertEqual(compact_literal(raw),expected);self.assertEqual(json.loads(raw),json.loads(expected))
 def test_nested_or_string_assets_does_not_replace_root_member(self):self.assertEqual(array_member(b'{"other":{"assets":[1]},"note":"assets", "assets" : [2, 3]}','assets'),b'[2,3]')
 def test_duplicate_root_asset_keys_refuse(self):
  with self.assertRaises(ValueError):array_member(b'{"assets":[1],"assets":[2]}','assets')
 def test_nonarray_or_invalid_json_refuses(self):
  for raw in [b'{"assets":null}',b'{"assets":[] } trailing',b'{"assets":["unterminated]}']:
   with self.assertRaises(ValueError):array_member(raw,'assets')
if __name__=='__main__':unittest.main()
