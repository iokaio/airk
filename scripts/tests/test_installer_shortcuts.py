# SPDX-License-Identifier: Apache-2.0
"""Real cmd launcher forwarding using an existing Python image, without builds."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("shortcuts_test",ROOT / "scripts/installer_shortcuts.py")
shortcuts=importlib.util.module_from_spec(spec);spec.loader.exec_module(shortcuts)


class ShortcutProcess(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "win32", "actual Windows cmd test")
    def test_cmd_preserves_arguments_streams_and_exit_status(self):
        with tempfile.TemporaryDirectory(prefix="rk space é›ª ! ",dir=ROOT / "artifacts") as directory:
            root=Path(directory);runtime=root / "runtime";runtime.mkdir()
            shutil.copyfile(sys.executable,runtime / "airkctl.exe")
            for dll in Path(sys.base_prefix).glob("python*.dll"):
                shutil.copyfile(dll,runtime / dll.name)
            name,body,_=shortcuts.portable("1.9.0","x86_64-pc-windows-msvc")
            launcher=root / name;launcher.write_bytes(body)
            # Inputs are individually quoted, as a shell caller must quote metacharacters.
            arguments=["two words","","é›ª","amp&value","bang!value","percent%value"]
            command='"'+' '.join('"'+part+'"' for part in [str(launcher),"-B","-S",str(ROOT / "scripts/tests/shortcut_process_probe.py"),*arguments])+'"'
            result=subprocess.run('"'+os.environ.get("COMSPEC","cmd.exe")+'" /d /e:off /s /c '+command,
                input="input stream",text=True,encoding="utf-8",capture_output=True,timeout=30,
                env={**os.environ,"ERRORLEVEL":"99","PYTHONHOME":sys.base_prefix,"PYTHONIOENCODING":"utf-8","PYTHONDONTWRITEBYTECODE":"1"})
            self.assertEqual(result.returncode,37,result.stderr)
            self.assertEqual(json.loads(result.stdout),{"arguments":arguments,"stdin":"input stream"})
            self.assertEqual(result.stderr.strip(),"shortcut stderr")

if __name__ == "__main__": unittest.main()
