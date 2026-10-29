# SPDX-License-Identifier: Apache-2.0
"""Build a Linux image's native pair and separate immutable approval entrypoint."""
import argparse
import importlib.util
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("airk_pair_builder", ROOT / "scripts/build-service-runtime.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    args = parser.parse_args()
    if platform.system() != "Linux":
        raise SystemExit("Container runtime pairs must be built and checked on Linux")
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    runtime = destination / "runtime"
    runtime.mkdir()
    builder.check_lightweight_dependencies()
    helper = builder.cargo_binary("airk-embed", args.profile, "embedding-worker")
    shutil.copy2(helper, runtime / "airk-embed")
    main_binary = builder.cargo_binary("airk", args.profile, None)
    shutil.copy2(main_binary, runtime / "airk")
    if args.profile == "debug":
        # DWARF can exceed a gigabyte for the pair and dominates approval hashing.
        # Strip the staged copies before sealing; the build tree stays untouched.
        for executable in ("airk", "airk-embed"):
            subprocess.run(["strip", "--strip-debug", str(runtime / executable)], check=True)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    architecture = {"x86_64": "x86_64", "aarch64": "aarch64"}.get(platform.machine().lower())
    if not architecture:
        raise SystemExit("Unsupported Linux container architecture")
    builder.stage_inference_runtime(runtime, "linux", architecture)
    builder.verify_embedded_models(runtime, "linux")
    builder.write_pair_metadata(runtime, version, "linux", architecture)
    (runtime / "airk-container-build.json").write_text(json.dumps({
        "schema": 1, "kind": "airk-container-build", "version": version,
        "cargo_lock_sha256": builder.digest(ROOT / "Cargo.lock"),
        "main_features": [], "helper_features": ["embedding-worker"],
    }, indent=2) + "\n", encoding="utf-8")
    builder.seal_install_payload(runtime, version, "linux", architecture)
    approved = builder.digest(runtime / "airk-install.json")
    # This fixed approval is outside the provider payload and is owned by the
    # immutable image. Workspace files/environment cannot nominate another pin.
    entrypoint = destination / "airk-container"
    entrypoint.write_text("#!/bin/sh\nexec /usr/lib/airk/airk runtime container-host --source /usr/lib/airk "
                          f"--approved-sha256 {approved} \"$@\"\n", encoding="utf-8")
    entrypoint.chmod(0o755)
    result = json.loads(subprocess.check_output([str(runtime / "airk"), "runtime", "self-test"], text=True))
    if result.get("self_test") != "passed" or result.get("version") != version:
        raise SystemExit("Container pair self-test failed")
    print(f"Staged Linux {architecture} container pair {version}; approval {approved}")


if __name__ == "__main__":
    main()
