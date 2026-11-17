# SPDX-License-Identifier: Apache-2.0
"""Existing full compiled Node suite; set AIRK_TEST_BINARY inside the p11 wrapper."""
import argparse, hashlib, json, os, re, subprocess, sys, time
from pathlib import Path
BASE = Path(__file__).resolve().parent
WORK = BASE.parent.parent
def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve(strict=True)
    binary = runtime / "airk.exe"
    output = args.output.absolute()
    assert os.name == "nt" and output.is_relative_to(BASE) and output != BASE
    with output.open("x", encoding="utf-8") as receipt:
        result = {"status": "failed", "runtime": str(runtime)}
        try:
            assert binary.is_file() and not binary.is_symlink()
            install = json.loads((runtime / "airk-install.json").read_text(encoding="utf-8"))
            assert install["version"] == "1.8.8" and sha(binary) == install["files"]["airk.exe"]
            tests = sorted((WORK / "packages/vscode/out/test").glob("*.test.js"))
            assert tests
            environment = os.environ.copy()
            environment["AIRK_TEST_BINARY"] = str(binary)
            command = ["node", "--test", "--test-reporter=tap", "--test-concurrency=2", *map(str, tests)]
            log = output.with_suffix(".node.log")
            started = time.monotonic()
            with log.open("x", encoding="utf-8") as stream:
                code = subprocess.run(command, cwd=WORK, env=environment, stdout=stream,
                    stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW).returncode
            text = log.read_text(encoding="utf-8", errors="strict")
            counts = {name: int(number) for name, number in re.findall(
                r"^# (tests|pass|fail|cancelled|skipped|todo) (\d+)\s*$", text, re.M)}
            result.update(command=command, exit_code=code, seconds=time.monotonic()-started,
                AIRK_TEST_BINARY=str(binary), counts=counts, log=str(log), log_sha256=sha(log),
                install_manifest_sha256=sha(runtime/"airk-install.json"),
                test_files={str(path.relative_to(WORK)):sha(path) for path in tests})
            assert code == 0 and counts.get("tests",0)>0 and counts.get("pass",0)>0
            assert counts.get("fail")==0 and counts.get("cancelled")==0
            result["status"]="passed"
        except BaseException as error:
            result["error"]=repr(error)
        finally:
            json.dump(result,receipt,indent=2);receipt.write("\n")
    print(json.dumps(result),flush=True)
    return 0 if result["status"]=="passed" else 1
if __name__=="__main__":
    sys.exit(main())
