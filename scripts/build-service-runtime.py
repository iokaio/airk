# SPDX-License-Identifier: Apache-2.0
"""Build and stage a host-native executable pair; never package/publish a VSIX."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time
import tomllib


ROOT = Path(__file__).resolve().parents[1]


def cargo_binary(name, profile, features):
    command = ["cargo", "build", "--locked", "-p", "airk-cli", "--bin", name,
               "--no-default-features", "--message-format=json-render-diagnostics"]
    if profile == "release":
        command.append("--release")
    if features:
        command += ["--features", features]
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8",
                            stdout=subprocess.PIPE, check=True)
    executable = None
    for line in result.stdout.splitlines():
        event = json.loads(line)
        if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == name:
            if event.get("executable"):
                executable = Path(event["executable"])
    if not executable or not executable.is_file():
        raise RuntimeError(f"Cargo did not produce {name}")
    return executable


def check_lightweight_dependencies():
    result = subprocess.check_output([
        "cargo", "tree", "--locked", "-p", "airk-cli", "--no-default-features",
        "--edges", "normal", "--prefix", "none", "--format", "{p}"],
        cwd=ROOT, text=True, encoding="utf-8")
    dependencies = {line.split()[0] for line in result.splitlines() if line.strip()}
    forbidden = sorted(name for name in dependencies if name == "tokenizers" or name.startswith("tract-"))
    if forbidden:
        raise RuntimeError(f"Main executable includes inference dependencies: {forbidden}")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_identity():
    names = subprocess.check_output([
        "git", "-c", f"safe.directory={ROOT.as_posix()}", "ls-files", "-z",
        "--cached", "--others", "--exclude-standard", "--", "Cargo.toml", "Cargo.lock",
        "rust-toolchain.toml", ".cargo", "crates", "scripts/build-service-runtime.py"], cwd=ROOT)
    result = hashlib.sha256()
    files = sorted(set(name for name in names.split(b"\0") if name))
    for name in files:
        path = ROOT / os.fsdecode(name)
        if path.is_symlink() or not path.resolve().is_relative_to(ROOT):
            raise RuntimeError("Source input escapes the runtime workspace")
        result.update(len(name).to_bytes(8, "big"))
        result.update(name)
        result.update(bytes.fromhex(digest(path)) if path.is_file() else b"missing")
    return {"sha256": result.hexdigest(), "files": len(files)}


def publish_staging(staging, destination):
    # A just-executed Windows image can briefly retain a scan/share handle. Retry
    # only this owned staging rename; persistent permission/policy failures stay errors.
    deadline = time.monotonic() + 5
    while True:
        try:
            os.rename(staging, destination)
            return
        except OSError as error:
            if os.name != "nt" or error.winerror not in (5, 32, 33) or time.monotonic() >= deadline:
                raise
            time.sleep(0.1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--profile", choices=["debug", "release"], default="debug")
    args = parser.parse_args()
    destination = args.destination.absolute()
    if destination.exists() or destination.is_symlink():
        raise SystemExit("Destination already exists; select a fresh runtime directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = source_identity()
    check_lightweight_dependencies()
    with tempfile.TemporaryDirectory(prefix=".airk-runtime-stage-", dir=destination.parent) as temporary:
        staging = Path(temporary)
        # Copy helper before the separate feature-minimal main build. Cargo unifies
        # features within one invocation, so a workspace build cannot establish this boundary.
        helper = cargo_binary("airk-embed", args.profile, "embedding-worker")
        shutil.copy2(helper, staging / helper.name)
        main_binary = cargo_binary("airk", args.profile, None)
        shutil.copy2(main_binary, staging / main_binary.name)
        version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
        operating_system = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}.get(platform.system())
        architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
        if not operating_system or not architecture:
            raise RuntimeError("Unsupported host runtime pair target")
        manifest = {"schema": 1, "version": version, "os": operating_system, "arch": architecture,
                    "airk_sha256": digest(staging / main_binary.name),
                    "embed_sha256": digest(staging / helper.name)}
        (staging / "airk-runtime.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        for executable in (main_binary.name, helper.name):
            identity = subprocess.check_output([str(staging / executable), "--version"],
                                               text=True, encoding="utf-8").strip()
            if identity != f"{Path(executable).stem} {version}":
                raise RuntimeError(f"Unexpected executable identity: {identity}")
        if source_identity() != source:
            raise RuntimeError("Native source changed during the paired build; staging was not published")
        receipt = {"schema": 1, "source": source, "profile": args.profile,
                   "runtime": manifest, "rustc": subprocess.check_output(
                       ["rustc", "--version", "--verbose"], text=True, encoding="utf-8").strip()}
        (staging / "airk-build.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        # The final directory is created only after both artifacts and identities verify.
        publish_staging(staging, destination)
    print(f"Staged {version} runtime pair: {destination}")


if __name__ == "__main__":
    main()
