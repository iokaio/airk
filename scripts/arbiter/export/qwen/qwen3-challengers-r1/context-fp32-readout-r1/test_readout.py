# SPDX-License-Identifier: Apache-2.0
import ast,sys,unittest
from pathlib import Path
import derive_numeric_fixture as m
class Shape:
 def __init__(self,*s):self.shape=s
class Controls(unittest.TestCase):
 def test_exact_shape(self):m.validate_shape(Shape(1,128),Shape(1,8),Shape(1,128,2048),Shape(8,2048),128,8)
 def test_refuse_wrong_shape(self):
  for h in [Shape(1,127,2048),Shape(1,128,2560),Shape(128,2048)]:
   with self.assertRaises(ValueError):m.validate_shape(Shape(1,128),Shape(1,8),h,Shape(8,2048),128,8)
 def test_no_backbone_or_runtime_import(self):
  self.assertNotIn('torch',sys.modules);self.assertNotIn('onnxruntime',sys.modules)
  for p in Path(__file__).parent.glob('*.py'):ast.parse(p.read_text())
if __name__=='__main__':unittest.main()
