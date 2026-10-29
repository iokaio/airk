# SPDX-License-Identifier: Apache-2.0
"""Qualify real offline MiniLM through native jobs and direct project retrieval."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import time

spec = importlib.util.spec_from_file_location("airk_service_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="Main executable in the verified runtime pair")
    parser.add_argument("--model-dir", type=Path, help="Explicit custom offline assets; omitted selects the bundled helper model")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    measurements = []
    with tempfile.TemporaryDirectory(prefix="airk-native-transformer-") as directory:
        base = Path(directory).resolve(strict=True)
        root, data = base / "workspace", base / "data"
        root.mkdir()
        data.mkdir()
        (root / "auth.txt").write_text("Reset a forgotten password using a time-limited recovery link sent to the verified email address.\n", encoding="utf-8")
        (root / "bread.txt").write_text("Bread dough combines flour and yeast before baking in a hot oven.\n", encoding="utf-8")
        originals = {path: hashlib.sha256(path.read_bytes()).hexdigest() for path in root.iterdir()}
        fixture = clients.Fixture(binary.parent, base)
        assert fixture.binary == binary
        def tool(name, arguments=None, deadline=None):
            if name == "airk_status":
                return fixture.published_status("transformer", deadline)
            return fixture.tool("transformer", name, {"response_mode": "full", **(arguments or {})})
        def ready():
            deadline = time.monotonic() + 300
            while True:
                status = tool("airk_status", deadline=deadline)
                readiness = clients.semantic_readiness(status)
                assert readiness["state"] != "failed", "Semantic work explicitly failed"
                if readiness["state"] == "ready" and readiness["completed_artifacts"] == 2:
                    assert status["indexed_vectors"] == 2 and "diskann" in status["vector_backend"]
                    return status
                assert time.monotonic() < deadline, readiness
                time.sleep(0.1)
        def measured(label, action):
            started = time.monotonic()
            value = action()
            measurements.append({"phase": label, "elapsed_ms": round((time.monotonic() - started) * 1000)})
            return value
        try:
            fixture.start()
            fixture.register("transformer", root, data)
            policy = json.loads(fixture.run(*fixture.arguments("transformer"), "settings", "vector_status").stdout)
            assert policy["enabled"] and policy["default_search_mode"] == "hybrid"
            if args.model_dir:
                model_input = args.model_dir.resolve(strict=True)
                measured("custom_model_import", lambda: fixture.configure("transformer", {"operation": "install_model", "directory": str(model_input)}))
            else:
                measured("offline_bundled_setup", lambda: fixture.job("transformer", "setup-model"))
                # These are this fixture's own installed files. Copy to an explicit
                # input directory for later custom-model tests; never mutate the shared set.
                references = list((fixture.control / "model-files-1.4").glob("default-*.json"))
                assert len(references) == 1
                selected = json.loads(references[0].read_bytes())
                identity = selected["shared"]["set"]
                assert len(identity) == 64 and all(char in "0123456789abcdef" for char in identity)
                installed = fixture.control / "model-files-1.4" / "sets" / identity
                model_input = base / "custom-input"
                model_input.mkdir()
                assert set(selected["assets"]) == {"model.json", "model.onnx", "tokenizer.json", "LICENSE"}
                for name, asset in selected["assets"].items():
                    source = installed / name
                    assert source.stat().st_size == asset["bytes"]
                    assert hashlib.sha256(source.read_bytes()).hexdigest() == asset["sha256"]
                    shutil.copyfile(source, model_input / name)
            # Always edit an owned input copy, including when the caller supplies assets.
            if args.model_dir:
                copied = base / "custom-input"
                shutil.copytree(model_input, copied)
                model_input = copied
            measured("index", lambda: fixture.job("transformer", "index"))
            initial = measured("semantic_readiness", ready)
            for mode in (None, "vector", "hybrid"):
                found = tool("airk_search", {"query": "Recover account access after forgetting a login secret", "limit": 1, **({"mode": mode} if mode else {})})
                assert found["hits"][0]["citation"]["path"] == "auth.txt"
                assert found["generation"] == initial["generation"]
            original_tokenizer = (model_input / "tokenizer.json").read_bytes()
            (model_input / "tokenizer.json").write_bytes(b"{}")
            fixture.configure("transformer", {"operation": "install_model", "directory": str(model_input)}, expected="failed")
            assert tool("airk_status")["generation"] == initial["generation"]
            assert tool("airk_search", {"query": "forgotten password recovery", "mode": "vector", "limit": 1})["hits"][0]["citation"]["path"] == "auth.txt"
            assert (model_input / "tokenizer.json").read_bytes() == b"{}"
            (model_input / "tokenizer.json").write_bytes(original_tokenizer)
            manifest = json.loads((model_input / "model.json").read_bytes())
            manifest["max_tokens"] = 128
            (model_input / "model.json").write_text(json.dumps(manifest), encoding="utf-8")
            # 1.6 deliberately accepts only the CPU-selected pinned INT8 profile.
            # Refuse edited preprocessing before changing policy or publication;
            # never report success and silently substitute the default later.
            fixture.configure("transformer", {"operation": "install_model", "directory": str(model_input)}, expected="failed")
            assert tool("airk_status")["generation"] == initial["generation"]
            assert tool("airk_search", {"query": "forgotten password recovery", "mode": "vector", "limit": 1})["hits"][0]["citation"]["path"] == "auth.txt"
            fixture.configure("transformer", {"operation": "vectors", "enabled": False})
            fixture.job("transformer", "index")
            assert tool("airk_status")["indexed_vectors"] == 0
            assert tool("airk_search", {"query": "forgotten", "limit": 1})["hits"][0]["citation"]["path"] == "auth.txt"
            rejected = fixture.run(*fixture.arguments("transformer"), "tools", "call", "airk_search", "--args",
                                   json.dumps({"query": "forgotten", "mode": "vector"}), check=False)
            assert rejected.returncode != 0 and json.loads(rejected.stdout)["isError"]
            fixture.configure("transformer", {"operation": "vectors", "enabled": True})
            fixture.job("transformer", "index")
            ready()
            assert all(hashlib.sha256(path.read_bytes()).hexdigest() == expected for path, expected in originals.items())
            assert not (data / "models" / "local").exists()
            report = {"schema": "airk-transformer-qualification/2", "passed": True,
                      "version": json.loads((fixture.runtime / "airk-runtime.json").read_bytes())["version"],
                      "runtime_manifest_sha256": hashlib.sha256((fixture.runtime / "airk-install.json").read_bytes()).hexdigest(),
                      "gates": ["shared offline model files", "real helper inference", "persistent DiskANN vectors",
                                "direct default/vector/hybrid retrieval", "opt-out and re-enable", "invalid-input rejection",
                                "immutable installed files", "context-bound revision identity", "source preservation"],
                      "measurements": measurements,
                      "limitations": ["Wall time includes brokerage and helper work; no process RSS, physical-I/O or provider-token claim."]}
        finally:
            fixture.close()
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
