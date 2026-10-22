# SPDX-License-Identifier: Apache-2.0
"""Exercise real local ONNX embeddings through public AIRK indexing/vector/hybrid commands."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, help="optional custom/provisioned model; defaults to the embedded offline model")
    parser.add_argument("--report", type=Path, help="retain direct process resource measurements for successful CLI calls")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    measurements = []
    measurement = None
    if args.report:
        spec = importlib.util.spec_from_file_location("benchmark_scale", Path(__file__).with_name("benchmark-scale.py"))
        measurement = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(measurement)
    with tempfile.TemporaryDirectory(prefix="airk-transformer-public-") as directory:
        base = Path(directory)
        root, data = base / "workspace", base / "data"
        root.mkdir()
        local = data / "models" / "local"
        (root / "auth.txt").write_text("Reset a forgotten password using a time-limited recovery link sent to the verified email address.\n", encoding="utf-8")
        (root / "bread.txt").write_text("Bread dough combines flour and yeast before baking in a hot oven.\n", encoding="utf-8")
        command = [str(binary), "--root", str(root), "--data-dir", str(data)]
        def run(*arguments, success=True):
            if measurement is not None and success:
                label = "-".join([str(len(measurements)), *arguments[:2]]).replace(" ", "_")[:80]
                value, metrics = measurement.measured(command + list(arguments), base, label, timeout=300)
                measurements.append(metrics)
                return value
            result = subprocess.run(command + list(arguments), capture_output=True, text=True, encoding="utf-8", timeout=300)
            if success and result.returncode:
                raise AssertionError(result.stderr)
            if not success:
                if result.returncode == 0:
                    raise AssertionError("invalid model unexpectedly indexed")
                return None
            return json.loads(result.stdout)
        policy = run("collections", "vectors", "show")
        if not policy["enabled"] or policy["default_search_mode"] != "hybrid":
            raise AssertionError("fresh collection did not default to hybrid")
        if args.model_dir:
            run("collections", "vectors", "install", str(args.model_dir.resolve()))
        else:
            run("collections", "vectors", "install-bundled")
        indexed = run("index")
        status = run("status")
        if not status["local_model_ready"] or status["indexed_vectors"] != 2 or "diskann" not in status["vector_backend"]:
            raise AssertionError("real transformer index was not ready")
        for mode in (None, "vector", "hybrid"):
            selection = ("--mode", mode) if mode else ()
            found = run("search", "Recover account access after forgetting a login secret", *selection, "--limit", "1")
            if found["hits"][0]["citation"]["path"] != "auth.txt" or found["generation"] != indexed["generation"]:
                raise AssertionError("real transformer CLI retrieval gate failed")
        # Broken newly provisioned tokenizer must not alter published model copies.
        original = (local / "tokenizer.json").read_bytes()
        (local / "tokenizer.json").write_bytes(b"{}")
        run("index", success=False)
        if run("status")["generation"] != indexed["generation"]:
            raise AssertionError("failed model update advanced generation")
        found = run("search", "forgotten password recovery", "--mode", "vector", "--limit", "1")
        if found["hits"][0]["citation"]["path"] != "auth.txt":
            raise AssertionError("published model was corrupted by new invalid assets")
        (local / "tokenizer.json").write_bytes(original)
        manifest = json.loads((local / "model.json").read_text())
        manifest["max_tokens"] = 128
        (local / "model.json").write_text(json.dumps(manifest), encoding="utf-8")
        changed = run("index")
        if changed["generation"] <= indexed["generation"] or changed["indexed"] != 0:
            raise AssertionError("tokenizer context change failed to advance model revision while reusing extraction")
        run("collections", "vectors", "set", "--enabled", "false")
        if run("search", "forgotten", "--limit", "1")["hits"][0]["citation"]["path"] != "auth.txt":
            raise AssertionError("opt-out did not preserve default lexical retrieval")
        run("search", "forgotten", "--mode", "vector", success=False)
        run("index")
        if run("status")["indexed_vectors"] != 0:
            raise AssertionError("opt-out rebuild retained vectors")
        run("collections", "vectors", "set", "--enabled", "true")
        run("index")
        if run("status")["indexed_vectors"] != 2:
            raise AssertionError("re-enable failed to restore vectors")
        report = {"passed": True, "gates": ["quota-managed model installation", "real inference", "persistent DiskANN vectors", "CLI default/vector/hybrid", "opt-out and re-enable", "invalid-asset rollback", "immutable model copies", "context-bound revision identity"]}
        if args.report:
            report.update(measurements=measurements, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                          cache_conditions="fresh processes; uncontrolled OS caches; tiny original two-document fixture")
            args.report.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report))


if __name__ == "__main__":
    main()
