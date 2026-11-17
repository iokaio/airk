# SPDX-License-Identifier: Apache-2.0
"""Golden stdout/stderr/exit capture for allowlisted offline CLI commands (step B).

Usage: python capture.py <airk.exe> <output.json>
Every case runs against owned fixture folders recreated under golden/fixture, so
the before and after captures see identical paths and identical initial state.
No case may reach the user's real installation: each one names an explicit
fixture install/control directory or needs none.
"""
import base64, json, os, shutil, subprocess, sys
from pathlib import Path

BASE = Path(__file__).resolve().parent
FX = BASE / "fixture"
EXE = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2])
ID = "01" * 16
SHA = "b" * 64


def fresh():
    if FX.exists():
        shutil.rmtree(FX)
    for leaf in ("install-empty", "control", "src", "data"):
        (FX / leaf).mkdir(parents=True)
    (FX / "measurement.json").write_text("{}", encoding="utf-8")
    # Owner-only, like AIRK's own roots, so commands proceed past the ACL check.
    user = os.environ["USERDOMAIN"] + "\\" + os.environ["USERNAME"]
    for leaf in ("install-empty", "control"):
        subprocess.run(["icacls", str(FX / leaf), "/inheritance:r", "/grant:r", user + ":(OI)(CI)F"], check=True, capture_output=True)


def cases():
    i, c, m = str(FX / "install-empty"), str(FX / "control"), str(FX / "missing")
    return [
        (["--version"], None),
        (["--help"], None),
        (["runtime", "--help"], None),
        (["service", "--help"], None),
        (["runtime", "bogus"], None),
        (["runtime", "inspect"], None),
        (["runtime", "location", "--channel", "stable"], None),
        (["runtime", "self-test"], None),
        (["runtime", "inspect", "--install-dir", i], None),
        (["runtime", "snapshot", "--install-dir", i], None),
        (["runtime", "diagnose", "--install-dir", m], None),
        (["runtime", "diagnose", "--install-dir", i], None),
        (["runtime", "inventory", "--install-dir", i], None),
        (["runtime", "inspect-candidate", "--install-dir", i, "--runtime-sha256", SHA], None),
        (["runtime", "replace-installation", "--install-dir", m, "--candidate-sha256", SHA], None),
        (["runtime", "http-endpoint", "--install-dir", i], None),
        (["runtime", "http-connection-info", "--install-dir", i, "--connection-id", ID], None),
        (["runtime", "lifecycle", "--install-dir", i], "{}"),
        (["runtime", "abandon-lifecycle", "--install-dir", i, "--request-id", ID], None),
        (["--root", str(FX / "src"), "--data-dir", str(FX / "data"), "--collection", "default", "service", "prepare-registration"], None),
        (["service", "logs", "--runtime-dir", c, "--limit", "32", "--format", "json"], None),
        (["service", "logs", "--runtime-dir", m, "--format", "text"], None),
        (["service", "rpc", "--runtime-dir", m, "--stdin"], '"health"'),
        (["service", "record-error", "--runtime-dir", m, "--operation", "fixture"], "fixture error"),
        (["service", "legacy", "list", "--runtime-dir", m], None),
        (["tools", "list", "--json"], None),
        (["tools", "help", "airk_search"], None),
        (["--service-install-dir", m, "tools", "call", "airk_status"], None),
        (["--service-install-dir", m, "usage", "import", str(FX / "measurement.json"), "--preview"], None),
        (["model-catalog"], None),
    ]


def run(argv, stdin, error_format):
    env = {k: v for k, v in os.environ.items() if not k.startswith("AIRK_")}
    if error_format:
        env["AIRK_ERROR_FORMAT"] = "json"
    done = subprocess.run([str(EXE), *argv], input=(stdin or "").encode(), capture_output=True, cwd=FX, env=env, timeout=120)
    return {"argv": argv, "stdin": stdin, "error_format": error_format, "exit": done.returncode,
            "stdout_b64": base64.b64encode(done.stdout).decode(), "stderr_b64": base64.b64encode(done.stderr).decode()}


results = []
for argv, stdin in cases():
    for error_format in (False, True):
        fresh()
        results.append(run(argv, stdin, error_format))
fresh()
OUT.write_text(json.dumps({"schema": "airk-golden-cli/1", "executable_name": EXE.name, "cases": results}, indent=1) + "\n")
print(json.dumps({"cases": len(results), "exits": sorted({r["exit"] for r in results})}))
