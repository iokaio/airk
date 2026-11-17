# SPDX-License-Identifier: Apache-2.0
"""Exact owned fixture trust lifecycle; no replay of an uncertain mutation."""
import argparse
import hashlib
import json
from pathlib import Path
import ssl
import subprocess
import os


def write(owner, name, value):
    with (owner / name).open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2)
        output.write("\n")


def command(intent, action):
    return [intent["binary"], "runtime", "http-trust", "--install-dir", intent["install"],
            "--action", action, "--expected-ca-sha256", intent["ca_sha256"]]


def result_record(result):
    return {"exit_code": result.returncode, "stdout": result.stdout[:65536],
            "stderr": result.stderr[:65536], "output_truncated": len(result.stdout) > 65536 or len(result.stderr) > 65536}


def install(fixture, owner):
    owner = owner.resolve(strict=True)
    assert fixture.install.resolve(strict=True) == owner / "native-service"
    ca = fixture.control / "http-1/ca.pem"
    digest = hashlib.sha256(ssl.PEM_cert_to_DER_cert(ca.read_text())).hexdigest()
    intent = {"schema": 1, "owner": str(owner), "install": str(fixture.install.resolve()),
              "binary": str(fixture.binary.resolve(strict=True)),
              "binary_sha256": hashlib.sha256(fixture.binary.read_bytes()).hexdigest(), "ca_sha256": digest}
    # Retain before dispatch, including unknown outcomes. Outer finally always
    # invokes removal once against this exact intent, never a discovered issuer.
    write(owner, "code-trust-intent.json", intent)
    record = {"command": command(intent, "install"), "mutation_dispatches": 1}
    try:
        result = fixture.run(*record["command"][1:], check=False)
        record.update(result_record(result))
        assert result.returncode == 0 and not record["output_truncated"], record
        status = json.loads(result.stdout)
        record["status"] = status
        assert status["ca_sha256"] == digest and status["trusted"] and status["owned"]
        assert status["outcome"] == "installed", status
    except BaseException as error:
        record["error"] = str(error)[:4096]
        raise
    finally:
        write(owner, "code-trust-install.json", record)
    return intent


def execute(argv):
    return subprocess.run(argv, capture_output=True, text=True, encoding="utf-8", timeout=60,
                          creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)


def remove(owner, invoke=execute):
    owner = owner.resolve(strict=True)
    intent_path = owner / "code-trust-intent.json"
    if not intent_path.exists():
        write(owner, "code-trust-remove.json", {"mutation_dispatches": 0, "outcome": "no_install_dispatch"})
        return
    intent = json.loads(intent_path.read_text())
    record = {"mutation_dispatches": 0, "command": command(intent, "remove")}
    try:
        assert intent["owner"] == str(owner) and Path(intent["install"]) == owner / "native-service"
        assert hashlib.sha256(Path(intent["binary"]).read_bytes()).hexdigest() == intent["binary_sha256"]
        record["mutation_dispatches"] = 1
        result = invoke(record["command"])
        record.update(result_record(result))
        assert result.returncode == 0 and not record["output_truncated"], record
        status = json.loads(result.stdout)
        record["status"] = status
        assert status["ca_sha256"] == intent["ca_sha256"]
        assert not status["owned"] and not status["present"] and not status["trusted"]
        assert status["outcome"] in ("removed", "already_absent"), status
    except BaseException as error:
        record["error"] = str(error)[:4096]
        raise
    finally:
        write(owner, "code-trust-remove.json", record)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--remove-owner", type=Path, required=True)
    remove(parser.parse_args().remove_owner)
