# SPDX-License-Identifier: Apache-2.0
"""Qualify real offline MiniLM through native jobs and direct project retrieval."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import time

spec = importlib.util.spec_from_file_location("airk_service_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def embedded_custom_input(fixture, base):
    """Copy verified build inputs for explicit custom-input rejection tests only."""
    # A split pair's airkctl reads the catalog from the owned service's control endpoint.
    scope = ("--service-runtime-dir", fixture.control) if fixture.cli is not None else ()
    catalog = json.loads(fixture.run(*scope, "model-catalog").stdout)
    manifest = catalog["bundled_model"]["manifest"]
    spec = importlib.util.spec_from_file_location("transformer_model_inputs", Path(__file__).with_name("provision-local-model.py"))
    provision = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(provision)
    profiles = [profile for profile in provision.CATALOG["profiles"] if profile["manifest"] == manifest]
    assert len(profiles) == 1, "Embedded selected model differs from the pinned input catalog"
    profile = profiles[0]
    source = Path(os.environ.get("AIRK_BUNDLED_MODEL_DIR", Path(__file__).resolve().parents[1] / "target/local-vector-model")) / profile["id"]
    assert source.is_dir() and not source.is_symlink(), "Verified offline custom-test inputs are missing"
    destination = base / "custom-input"
    destination.mkdir()
    for asset in profile["assets"]:
        path = source / asset["name"]
        provision.verify(path, asset["bytes"], asset["sha256"])
        shutil.copyfile(path, destination / asset["name"])
        provision.verify(destination / asset["name"], asset["bytes"], asset["sha256"])
    (destination / "model.json").write_text(json.dumps(manifest), encoding="utf-8")
    return destination


def assert_embedded_default_not_extracted(control):
    directory = control / "model-files-1.4"
    assert not list(directory.glob("default-*.json")), "Default setup extracted a model reference"
    sets = directory / "sets"
    assert not sets.exists() or not any(sets.iterdir()), "Default setup extracted model assets"


def assert_healthy_embedded_provider(observation):
    owner = observation["provider"]
    assert owner["kind"] == "in_process" and observation["process_id"] is None
    assert observation["available"] is True and observation["loads"] >= 1
    # ProviderStatus omits its optional failure when healthy. Present failures
    # still refuse qualification, as does a provider with closed admission.
    assert owner["admission_open"] is True and owner.get("failure") is None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="Main executable in the verified runtime pair")
    parser.add_argument("--airkctl", type=Path, help="A split pair's own airkctl (verified; derived from the pair when omitted)")
    parser.add_argument("--model-dir", type=Path, help="Explicit custom offline assets; omitted selects the bundled model")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    measurements = []
    with clients.owned_client_fixture() as directory:
        base = Path(directory).resolve(strict=True)
        root, data = base / "workspace", base / "data"
        root.mkdir()
        data.mkdir()
        (root / "auth.txt").write_text("Reset a forgotten password using a time-limited recovery link sent to the verified email address.\n", encoding="utf-8")
        (root / "bread.txt").write_text("Bread dough combines flour and yeast before baking in a hot oven.\n", encoding="utf-8")
        originals = {path: hashlib.sha256(path.read_bytes()).hexdigest() for path in root.iterdir()}
        fixture = clients.Fixture(binary.parent, base, cli=args.airkctl)
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
                # Modern defaults stay embedded. Custom rejection tests use a
                # verified caller-owned copy of offline build inputs; historical
                # payloads instead supply their verified installed model set.
                if fixture.single_image:
                    assert_embedded_default_not_extracted(fixture.control)
                    model_input = embedded_custom_input(fixture, base)
                else:
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
            provider = fixture.rpc("embedding_status")
            expected_pipeline = None
            if fixture.single_image:
                assert_healthy_embedded_provider(provider)
                if not args.model_dir:
                    assert_embedded_default_not_extracted(fixture.control)
                    smoke = json.loads((fixture.runtime / "airk-build.json").read_bytes())["embedded_model_smoke"]
                    assert smoke["source"] == "embedded" and smoke["repeat_equal"] is True
                    expected_pipeline = smoke["pipeline_digest"]
                    assert len(expected_pipeline) == 64 and all(char in "0123456789abcdef" for char in expected_pipeline)
                    assert clients.semantic_readiness(initial)["pipeline_digest"] == expected_pipeline
            for mode in (None, "vector", "hybrid"):
                found = tool("airk_search", {"query": "Recover account access after forgetting a login secret", "limit": 1, **({"mode": mode} if mode else {})})
                assert found["hits"][0]["citation"]["path"] == "auth.txt"
                assert found["generation"] == initial["generation"]
                if expected_pipeline:
                    assert clients.semantic_readiness(found)["pipeline_digest"] == expected_pipeline
                if mode == "vector":
                    assert found["execution"]["served_mode"] == "vector" and found["execution"]["incomplete"] is False
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
            assert not (Path(fixture.projects["transformer"]["data_root"]) / "models" / "local").exists()
            report = {"schema": "airk-transformer-qualification/2", "passed": True,
                      "version": json.loads((fixture.runtime / "airk-runtime.json").read_bytes())["version"],
                      "runtime_manifest_sha256": hashlib.sha256((fixture.runtime / "airk-install.json").read_bytes()).hexdigest(),
                      "provider": provider,
                      "gates": ["embedded default model without extraction" if fixture.single_image and not args.model_dir else "verified offline model inputs",
                                "real in-process inference" if fixture.single_image else "real helper inference", "persistent DiskANN vectors",
                                "direct default/vector/hybrid retrieval", "opt-out and re-enable", "invalid-input rejection",
                                "immutable installed files", "context-bound revision identity", "source preservation"],
                      "measurements": measurements,
                      "limitations": ["Wall time includes native brokerage and inference; no process RSS, physical-I/O or provider-token claim."]}
        finally:
            fixture.close()
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
