# SPDX-License-Identifier: Apache-2.0
"""Qualify fresh setup from a separately supplied pre-cutover runtime with isolated source/data paths."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time
import uuid

spec = importlib.util.spec_from_file_location("service_clients", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def qualify(previous, current):
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="airk-storage-recovery-") as temporary:
        owner = Path(temporary).resolve()
        source, data = owner / "source", owner / "data"
        source.mkdir()
        data.mkdir()
        originals = {}
        for number in range(8):
            text = "from normalizer import normalize\n\n" + "\n".join(
                f"def polar_case_{number}_{function}(value: str) -> str:\n    return normalize(value)\n"
                for function in range(32)
            )
            path = source / f"fixture_{number}.py"
            path.write_text(text, encoding="utf-8")
            originals[path] = hashlib.sha256(path.read_bytes()).hexdigest()
        old = clients.Fixture(previous, owner, job_timeout=600)
        try:
            old.start()
            old.register("pressure", source, data)
            old_job = old.wait(old.enqueue("pressure", "index"))
            old_epoch = old.rpc("request_epoch")
            old_namespace = old_epoch["namespace"]
        finally:
            old.close()
        new = clients.Fixture(current, owner, job_timeout=600)
        try:
            new.start()
            new.register("pressure", source, data)
            recovered = new.wait(new.enqueue("pressure", "index"))
            assert recovered["state"] == "succeeded", recovered
            assert new.rpc("request_epoch")["namespace"] != old_namespace
            stale={"enqueue":{"project":new.projects["pressure"]["binding"],"kind":"index","caller":uuid.uuid4().hex,"key":{"namespace":old_namespace,"epoch":old_epoch["epoch"],"nonce":uuid.uuid4().hex}}}
            refused=new.run("service","rpc","--runtime-dir",new.control,"--stdin",input=json.dumps(stale),check=False)
            refusal=json.loads(refused.stdout)
            assert refusal["status"]=="failure",refusal
            search=new.tool("pressure","airk_search",{"query":"polar_case_0_0","mode":"lexical","response_mode":"full"})
            assert any(hit["citation"]["path"]=="fixture_0.py" for hit in search["hits"]),search
            status=new.published_status("pressure")
            assert status["storage_sizes"]["current_index_file"]=="current.airk",status
            assert not (new.control/"service-1.4.sqlite3").exists()
            for path, expected in originals.items():
                assert hashlib.sha256(path.read_bytes()).hexdigest() == expected
            return {
                "schema": 1,
                "previous_version": json.loads((previous / "airk-runtime.json").read_text())["version"],
                "version": json.loads((current / "airk-runtime.json").read_text())["version"],
                "files": 8, "functions_per_file": 32,
                "previous_job_state": old_job["state"],
                "fresh_setup": True, "fresh_registration": True,
                "recovery_state": recovered["state"], "source_bytes_preserved": True,
                "fresh_search_verified":True,"stale_request_rejected":True,"native_current_file":"current.airk",
                "elapsed_seconds": round(time.monotonic() - started, 3),
            }
        finally:
            new.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous-runtime-dir", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    report = qualify(args.previous_runtime_dir.resolve(strict=True), args.runtime_dir.resolve(strict=True))
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
