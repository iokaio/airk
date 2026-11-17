# SPDX-License-Identifier: Apache-2.0
"""Synthetic command/refusal controls; no installer, native product or registration."""
import json,runpy,unittest
from pathlib import Path
HERE=Path(__file__).resolve().parent
prep=runpy.run_path(str(HERE/"prepare.py"));runner=runpy.run_path(str(HERE/"run_step.py"))
class Preparation(unittest.TestCase):
    def recipe(self,platform="windows",mode="installer"):
        return prep["recipe"](platform,mode,"D:/verified/payload.exe" if platform=="windows" else "/verified/payload", "D:/fresh journey café" if platform=="windows" else "/private/fresh journey café","dedicated-test-user","isolated-test-host")
    def test_windows_exact_silent_and_quoted_data(self):
        r=self.recipe();self.assertEqual(r["commands"][0]["argv"],[r"D:\verified\payload.exe","--silent",r"/DIR=D:\fresh journey café\program",r"/DATA=D:\fresh journey café\data-location",r"/CACHE=D:\fresh journey café\cache-location",r"/LOG=D:\fresh journey café\installer.log"])
    def test_all_unix_wrappers_literal_silent(self):
        for name in ["linux","mac-arm64","mac-intel"]:
            with self.subTest(name=name):
                command=self.recipe(name)["commands"][0]["argv"];self.assertEqual(command[1],"--silent");self.assertNotIn("--acknowledge-reset",command);self.assertNotIn("--no-login-startup",command)
    def test_portable_public_cli_only(self):
        r=self.recipe(mode="portable");self.assertIn("--portable",r["commands"][0]["argv"]);self.assertEqual(r["commands"][1]["id"],"start");self.assertNotIn("__airk_role",json.dumps(r));self.assertNotIn("runtime activate",json.dumps(r))
    def test_mac_portable_preserves_bundle(self):
        command=self.recipe("mac-arm64","portable")["commands"][0]["argv"];self.assertEqual(command[0],"/verified/payload/runtime/AIRK.app/Contents/MacOS/airkctl")
    def test_protected_account_refuses(self):
        for identity in ["", "0", "S-1-5-21-3678370870-3858642417-826968105-1002"]:
            with self.subTest(identity=identity),self.assertRaises(ValueError):prep["recipe"]("windows","installer","D:/payload.exe","D:/fresh",identity,"host")
    def test_relative_and_overlap_refuse(self):
        for payload,root in [("relative.exe","D:/root"),("D:/payload.exe","relative"),("D:/payload","D:/payload/nested"),("D:/root/payload","D:/root")]:
            with self.subTest(payload=payload,root=root),self.assertRaises(ValueError):prep["recipe"]("windows","installer",payload,root,"dedicated","host")
    def test_future_or_unknown_modes_refuse(self):
        for platform,mode in [("other","installer"),("linux","arbitrary")]:
            with self.subTest(platform=platform,mode=mode),self.assertRaises(ValueError):prep["recipe"](platform,mode,"/payload","/fresh","dedicated","host")
    def observation(self):return {"identity":"dedicated-test-user","hostname":"isolated-test-host","existing_predecessor_roots":[],"registration_conflicts":[]}
    def test_foreign_host_or_user_refuses(self):
        for field in ["identity","hostname"]:
            observed=self.observation();observed[field]="foreign"
            with self.subTest(field=field),self.assertRaises(ValueError):runner["validate_host"](self.recipe(),observed,True)
    def test_existing_roots_or_registration_refuse(self):
        for field in ["existing_predecessor_roots","registration_conflicts"]:
            observed=self.observation();observed[field]=["preserve unrelated installation"]
            with self.subTest(field=field),self.assertRaises(ValueError):runner["validate_host"](self.recipe(),observed,True)
    def test_failed_unknown_or_foreign_previous_step_refuses(self):
        r=self.recipe();r["recipe_sha256"]="a"*64
        for prior in [{"exit_code":11,"timed_out":False,"recipe_sha256":"a"*64},{"exit_code":None,"timed_out":True,"recipe_sha256":"a"*64},{"exit_code":0,"timed_out":False,"recipe_sha256":"b"*64}]:
            with self.subTest(prior=prior),self.assertRaises(ValueError):runner["validate_steps"](r,"setup-receipt",lambda name:prior)
    def test_exact_original_step_required(self):
        r=self.recipe();r["recipe_sha256"]="a"*64
        with self.assertRaises(ValueError):runner["validate_steps"](r,"arbitrary",lambda name:None)
        self.assertEqual(runner["validate_steps"](r,"install",lambda name:None),r["commands"][0])
    def test_process_and_native_removal_not_conflated(self):
        r=self.recipe();self.assertFalse(r["qualified"]);self.assertFalse(r["uninstall_exit_zero_is_completion"]);self.assertEqual(r["commands"][-1]["argv"][-3:],["--yes","--output","json"]);self.assertFalse(r["process_contract"]["shell"])
if __name__=="__main__":unittest.main(verbosity=2)
