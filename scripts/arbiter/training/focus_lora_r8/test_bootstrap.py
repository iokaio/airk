# SPDX-License-Identifier: Apache-2.0
"""Pure Python startup provenance controls; no child or cache writes."""
import unittest
from launch_owned import verify_bootstrap

PREFIX=r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\f1-fresh-unwritten-pycache-r1'


class BootstrapControls(unittest.TestCase):
    def test_actual_sealed_fresh_prefix_and_no_writes_required(self):
        plan={'environment':{'PYTHONPYCACHEPREFIX':PREFIX,'PYTHONDONTWRITEBYTECODE':'1'}}
        result=verify_bootstrap(plan,PREFIX,True,path_exists=lambda _:False)
        self.assertEqual(result['actual_pycache_prefix'],PREFIX)
        self.assertTrue(result['actual_dont_write_bytecode'])
        self.assertTrue(result['prefix_absent'])
    def test_old_existing_or_write_enabled_bootstrap_refuses(self):
        plan={'environment':{'PYTHONPYCACHEPREFIX':PREFIX,'PYTHONDONTWRITEBYTECODE':'1'}}
        for actual,flag,exists in [(None,True,False),(r'C:\old-cache',True,False),(PREFIX,False,False),(PREFIX,True,True)]:
            with self.subTest(actual=actual,flag=flag,exists=exists),self.assertRaises(ValueError):
                verify_bootstrap(plan,actual,flag,path_exists=lambda _:exists)
        plan['environment']['PYTHONDONTWRITEBYTECODE']='0'
        with self.assertRaises(ValueError):verify_bootstrap(plan,PREFIX,True,path_exists=lambda _:False)

if __name__=='__main__':unittest.main(verbosity=2)
