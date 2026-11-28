# SPDX-License-Identifier: Apache-2.0
"""Unit regressions and optional checked Windows model-free fail/edit/pass smoke."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import unittest

import agent_cost_run as runner
import session_verify as verify


class VerificationTest(unittest.TestCase):
    def test_selector_validation(self):
        module = "clients/python/tests/test_sse.py"
        profile = {"kind": "pytest", "modules": [module]}
        self.assertEqual(verify.selection(profile, {}), [module])
        self.assertEqual(verify.selection(profile, {"selectors": [module+"::test_one[ok]"]}), [module+"::test_one[ok]"])
        for bad in ("-q", "../other.py", module+"::test[x/y]", module+"::test_one;cmd"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                verify.selection(profile, {"selectors": [bad]})
        with self.assertRaises(ValueError):
            verify.selection(profile, {"argv": ["other"]})

    def test_access_is_identical_across_arms(self):
        for arm in ("tool", "live", "plain"):
            argv = runner.command("claude", "m", arm, Path("config.json"), 8, product=arm != "plain", verification=True)
            for opt in ("--tools", "--allowedTools"):
                self.assertIn(verify.NAME, argv[argv.index(opt)+1].split(","))
            self.assertIn("Bash", argv[argv.index("--disallowedTools")+1].split(","))
            self.assertIn("run_tests", argv[argv.index("--system-prompt")+1])

    def test_no_verification_preserves_previous_modes(self):
        argv = runner.command("claude", "m", "plain", Path("config.json"), 8)
        self.assertNotIn(verify.NAME, argv[argv.index("--tools")+1])
        self.assertEqual(argv[argv.index("--system-prompt")+1], runner.SYSTEM_PROMPT)


def checked_edit(session, path, contents):
    cmd = ["pwsh", "-NoProfile", "-File", str(verify.HERE/"checked_cost_fixture.ps1"), "-Session", str(session)]
    frame = json.dumps({"tool_input": {"file_path": str(path)}}).encode()
    subprocess.run(cmd + ["-Mode", "BeforeTool"], input=frame, check=True)
    path.write_text(contents, encoding="utf-8")
    subprocess.run(cmd + ["-Mode", "AfterTool"], input=frame, check=True)


def smoke(session, kind, runtime):
    # Caller pre-creates a checked empty session/work and its verification receipt directory.
    before = len(list((session/"verification").glob("*/result.json")))
    work = session/"work"
    if kind == "pytest":
        module = "clients/python/tests/test_offline.py"
        data = {"kind":kind, "modules":[module], "runtime":str(runtime)}
        editable = work/"clients/python/src/example.py"
        checked_edit(session, work/module, "from example import size\ndef test_size():\n    assert size() == 2\n")
        wrong, right = "def size():\n    return 1\n", "def size():\n    return 2\n"
        narrow = module+"::test_size"
    else:
        data = {"kind":kind, "module":"pro/src/WritingForgePro-ui/src/utils/example.ts", "export":"example", "runtime":str(runtime),
                "cases":[{"name":"empty","args":["",""],"expected":[]}, {"name":"same","args":["x","x"],"expected":[]}]}
        editable = work/data["module"]
        wrong, right = ("export function example(a: string,b: string) { return [a]; }",
                        "export function example(a: string,b: string) { return []; }")
        narrow = "empty"
    profile_path = session/"verification-profile.json"
    profile_path.write_text(json.dumps(data),encoding="utf-8")
    checked_edit(session, editable, wrong)
    def run(args):
        req = [{"jsonrpc":"2.0","id":1,"method":"tools/list"},
               {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_tests","arguments":args}}]
        process = subprocess.run([sys.executable,"-B",str(verify.HERE/"session_verify.py"),"--session",str(session),
                                  "--profile",str(profile_path)], input=b"".join((json.dumps(r)+"\n").encode() for r in req),
                                 capture_output=True, timeout=180)
        if process.returncode:
            raise RuntimeError(process.stderr.decode(errors="replace"))
        listed, result = [json.loads(line)["result"] for line in process.stdout.splitlines()]
        assert listed["tools"][0]["name"]=="run_tests"
        return result
    failed = run({"selectors":[narrow]})
    print(failed["content"][0]["text"][-3000:])
    assert failed["isError"] and "1 failed" in failed["content"][0]["text"]
    checked_edit(session,editable,right)
    passed = run({})
    print(passed["content"][0]["text"][-3000:])
    assert not passed["isError"] and "passed" in passed["content"][0]["text"]
    denied = run({"selectors":["--help"]})
    assert denied["isError"] and len(list((session/"verification").glob("*/result.json"))) == before + 2


if __name__ == "__main__":
    if "--smoke" in sys.argv:
        p=argparse.ArgumentParser()
        p.add_argument("--smoke", required=True, type=Path)
        p.add_argument("--kind", required=True, choices=("pytest","node-cases"))
        p.add_argument("--runtime", required=True, type=Path)
        a=p.parse_args()
        smoke(a.smoke, a.kind, a.runtime)
    else:
        unittest.main()