# SPDX-License-Identifier: Apache-2.0
"""Local source build handoff and copied public experiment dependency closure."""
import importlib.util
from contextlib import redirect_stdout
from datetime import datetime, timezone
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("local_build",Path(__file__).parents[1] / "local-build.py")
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class LocalBuildTests(unittest.TestCase):
    def test_main_accepts_exact_schema2_main_only_and_records_unknown_git_state(self):
        builder = BUILD.load_script("build-service-runtime")
        guide = (BUILD.ROOT/"scripts/local-build-README.md").read_text()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root/"scripts").mkdir()
            (root/"scripts/local-build-README.md").write_text(guide)
            (root/"Cargo.toml").write_text('[workspace.package]\nversion="1.8.14"\n')
            child_environment = {"VERIFIED_CHILD_ONLY":"yes"}
            def build(command,**options):
                self.assertEqual(options["env"],child_environment)
                output = Path(command[command.index("--destination")+1]); output.mkdir()
                main = output/"airk.exe"; main.write_bytes(b"inert schema2 main fixture")
                metadata = {"schema":2,"version":"1.8.14","os":"windows","arch":"x86_64","airk_sha256":BUILD.digest(main)}
                (output/"airk-runtime.json").write_text(json.dumps(metadata))
                (output/"airk-install.json").write_text(json.dumps({"schema":1,"version":"1.8.14","os":"windows","arch":"x86_64","files":{"airk.exe":metadata["airk_sha256"]}}))
            fixed = datetime(2026,11,7,12,0,tzinfo=timezone.utc)
            with patch.object(BUILD,"ROOT",root), patch.object(BUILD,"load_script",return_value=builder), patch.object(BUILD,"build_environment",return_value=child_environment), patch.object(BUILD,"stage_support") as support, patch.object(BUILD,"git",return_value=None), patch.object(BUILD.platform,"system",return_value="Windows"), patch.object(BUILD.platform,"machine",return_value="AMD64"), patch.object(BUILD.subprocess,"run",side_effect=build), patch.object(BUILD,"datetime",Mock(now=Mock(return_value=fixed))), patch.object(sys,"argv",["local-build.py","--profile","debug"]), redirect_stdout(io.StringIO()):
                BUILD.main()
            output = root/"builds/20261107-120000-windows-x86_64"
            text = (output/"README.md").read_text()
            self.assertIn("runtime/airk.exe",text)
            self.assertIn("working-tree state unavailable",text)
            self.assertFalse((output/"runtime/airk-embed.exe").exists())
            support.assert_called_once_with(output)

    def test_offline_preparation_uses_verified_child_environment_and_preserves_caller(self):
        target = "x86_64-pc-windows-msvc"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            args = SimpleNamespace(download_inputs=False,static_environment_file=root/"existing.json",static_ort=None)
            original = dict(os.environ)
            approved = {"LIBONNXRUNTIME_NO_PKG_CONFIG":"1","ORT_SKIP_DOWNLOAD":"1","ORT_PREFER_DYNAMIC_LINK":"0","ORT_LIB_LOCATION":str(root/"b"),"ORT_LIB_PROFILE":"Release"}
            owner = Mock(); owner.load_environment_file.return_value = approved
            with patch.object(BUILD,"ROOT",root), patch.object(BUILD,"load_script",return_value=owner), patch.object(BUILD.subprocess,"run") as run:
                child = BUILD.build_environment(args,target,root)
            self.assertEqual(dict(os.environ),original)
            self.assertEqual({key:child[key] for key in approved},approved)
            self.assertEqual(run.call_count,1)
            self.assertNotIn("--download",run.call_args.args[0])
            owner.load_environment_file.assert_called_once_with(args.static_environment_file,None,target)
            self.assertTrue(Path(child["AIRK_BUNDLED_MODEL_DIR"]).is_absolute())
            owner.load_environment_file.side_effect = ValueError("tampered input handoff")
            with patch.object(BUILD,"ROOT",root), patch.object(BUILD,"load_script",return_value=owner), patch.object(BUILD.subprocess,"run"):
                with self.assertRaisesRegex(ValueError,"tampered"):
                    BUILD.build_environment(args,target,root)

    def test_only_explicit_download_prepares_pinned_static_tools_and_models(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            owner = Mock(); owner.load_environment_file.return_value = {}
            for online in (False,True):
                args = SimpleNamespace(download_inputs=online,static_environment_file=None,static_ort=root/"static")
                with self.subTest(online=online), patch.object(BUILD,"ROOT",root), patch.object(BUILD,"load_script",return_value=owner), patch.object(BUILD.subprocess,"run") as run:
                    BUILD.build_environment(args,"x86_64-pc-windows-msvc",root)
                commands = [call.args[0] for call in run.call_args_list]
                self.assertEqual(len(commands),2)
                self.assertEqual("--download" in commands[0],online)
                self.assertEqual("--download-tools" in commands[1],online)
                self.assertEqual("--verify" in commands[1],not online)
                if online:
                    self.assertIn("--build",commands[1])
                    self.assertEqual(commands[1][-2:],["--parallel","2"])

    def test_public_experiment_closure_imports_without_the_original_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            recorded = BUILD.stage_support(root)
            manifest = json.loads((root/"tools/input-manifest.json").read_text())
            self.assertEqual(manifest["files"],recorded)
            for relative,row in recorded.items():
                self.assertEqual(BUILD.digest(root/relative),row["sha256"])
            # -I removes checkout/PYTHONPATH help. These execute imports/help only;
            # no AIRK, Cargo, TLS server or runtime installation is launched.
            for relative in ("tools/check-service-clients.py","examples/local_https_client.py"):
                result = subprocess.run([sys.executable,"-I",str(root/relative),"--help"],cwd=root,capture_output=True,text=True,timeout=20)
                self.assertEqual(result.returncode,0,result.stderr)
            for relative in ("tools/check-runtime-upgrade.py","tools/http_fixture.py","tools/owned-runtime-processes.py"):
                script = "import importlib.util; s=importlib.util.spec_from_file_location('copied',"+repr(str(root/relative))+"); m=importlib.util.module_from_spec(s); s.loader.exec_module(m)"
                result = subprocess.run([sys.executable,"-I","-c",script],cwd=root,capture_output=True,text=True,timeout=20)
                self.assertEqual(result.returncode,0,result.stderr)
            self.assertTrue((root/"crates/airk-retrieval/src/model_catalog.json").is_file())
            self.assertFalse((root/"Cargo.toml").exists())

    def test_generated_guide_uses_owned_canonical_https_experiment(self):
        guide = (BUILD.ROOT/"scripts/local-build-README.md").read_text()
        self.assertIn("python tools/check-service-clients.py --runtime-dir runtime --files 1",guide)
        self.assertIn("native-reviewed Index setup",guide)
        self.assertIn("exact instance-bound managed Stop",guide)
        self.assertIn("GET /api/v1/tools",guide)
        for obsolete in ("{{EMBED}}","{{SETUP}}","serve --runtime-dir","@scope mcp","service register --runtime-dir"):
            self.assertNotIn(obsolete,guide)


if __name__ == "__main__":
    unittest.main()
