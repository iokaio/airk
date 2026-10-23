# SPDX-License-Identifier: Apache-2.0
"""Exercise an explicitly supplied host runtime pair using only original temporary fixtures."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    runtime = args.runtime.resolve(strict=True)
    binary = runtime / ("airk.exe" if os.name == "nt" else "airk")
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1",
                       HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    with tempfile.TemporaryDirectory(prefix="airk-runtime-smoke-") as temporary:
        root = Path(temporary)
        source = root / "source"
        source.mkdir()
        original = "# Copper beacon\nAn original fixture describes copper beacon maintenance.\n"
        document = source / "memo.md"
        document.write_text(original, encoding="utf-8")
        base = [str(binary), "--root", str(source), "--data-dir", str(root / "data"),
                "--collection", "runtime-pair-fixture"]

        def run(*arguments):
            result = subprocess.run(base + list(arguments), capture_output=True, text=True,
                                    encoding="utf-8", env=environment, timeout=180)
            if result.returncode:
                raise RuntimeError(result.stderr[-3000:])
            return json.loads(result.stdout)

        run("collections", "vectors", "install-bundled")
        print("Offline helper provisioning passed.", flush=True)
        run("index")
        response = run("search", "copper beacon")
        if not response["hits"] or response["hits"][0]["citation"]["path"] != "memo.md":
            raise RuntimeError("Runtime pair did not return its original collection citation")
        if document.read_text(encoding="utf-8") != original:
            raise RuntimeError("Source fixture changed during retrieval")
        print("External-model indexing, hybrid retrieval and source preservation passed.", flush=True)


if __name__ == "__main__":
    main()
