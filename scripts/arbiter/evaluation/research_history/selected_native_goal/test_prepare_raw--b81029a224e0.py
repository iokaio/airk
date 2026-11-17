# SPDX-License-Identifier: Apache-2.0
import json
import unittest
from pathlib import Path
from prepare_raw_packet import cargo_image
class CargoImageTests(unittest.TestCase):
    def row(self,path):return json.dumps({'reason':'compiler-artifact','target':{'name':'airk_kernel'},'profile':{'test':True},'executable':path})
    def test_exact_actual_kernel_image(self):
        raw=('diagnostic line\n'+self.row('actual-kernel.exe')+'\n').encode()
        self.assertEqual(cargo_image(raw),Path('actual-kernel.exe').resolve())
    def test_missing_image_refuses(self):
        with self.assertRaises(ValueError):cargo_image(b'{}\n')
    def test_ambiguous_images_refuse(self):
        with self.assertRaises(ValueError):cargo_image((self.row('first.exe')+'\n'+self.row('second.exe')).encode())
if __name__=='__main__':unittest.main()
