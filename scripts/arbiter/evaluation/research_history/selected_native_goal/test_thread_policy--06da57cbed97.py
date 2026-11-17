# SPDX-License-Identifier: Apache-2.0
import ast
import os
from pathlib import Path
import unittest
from unittest.mock import patch
import run_bounded
class ThreadPolicyTests(unittest.TestCase):
    def test_all_preimport_thread_limits_exact_one(self):
        with patch.dict(os.environ,{key:'99' for key in run_bounded.THREAD_KEYS},clear=False):
            run_bounded.enforce_thread_environment()
            self.assertEqual({key:os.environ[key]for key in run_bounded.THREAD_KEYS},{key:'1'for key in run_bounded.THREAD_KEYS})
    def test_environment_enforced_before_any_arrow_or_runpy(self):
        tree=ast.parse(Path(run_bounded.__file__).read_text(encoding='utf-8'))
        main=next(x for x in tree.body if isinstance(x,ast.FunctionDef)and x.name=='main')
        policy=next(x.lineno for x in ast.walk(main)if isinstance(x,ast.Call)and isinstance(x.func,ast.Name)and x.func.id=='enforce_thread_environment')
        arrow=next(x.lineno for x in ast.walk(main)if isinstance(x,ast.Import)and any(a.name=='pyarrow'for a in x.names))
        dispatch=next(x.lineno for x in ast.walk(main)if isinstance(x,ast.Call)and isinstance(x.func,ast.Attribute)and x.func.attr=='run_path')
        self.assertLess(policy,arrow);self.assertLess(arrow,dispatch)
if __name__=='__main__':unittest.main()
