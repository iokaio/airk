# SPDX-License-Identifier: Apache-2.0
"""Build a Linux image's native runtime and immutable exec-only approval entrypoint."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import shutil
import shlex
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("airk_pair_builder", ROOT / "scripts/build-service-runtime.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


def container_source_identity(root=None):
    """Seal actual public COPY inputs; this is not a Git/release attestation."""
    root = Path(root or ROOT).resolve(strict=True)
    source_stage = False
    inputs = set()
    for line in (root / "Dockerfile").read_text(encoding="utf-8").splitlines():
        words = shlex.split(line, comments=True)
        if not words:
            continue
        if words[0].upper() == "FROM":
            if source_stage:
                break
            source_stage = words[-2:] == ["AS", "source"]
        elif source_stage and words[0].upper() == "COPY":
            # Separately verified C++/tool build outputs are bound by static_ort,
            # not misrepresented as original repository COPY inputs.
            if words[1:] == ["--from=static-inference", "/build/static-onnx-runtime", "/build/static-onnx-runtime"]:
                continue
            if len(words) < 3 or any(word.startswith("--") for word in words[1:]):
                raise ValueError("Container source COPY form needs an explicit reviewed inventory")
            inputs.update(words[1:-1])
    if not inputs or not {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates", "Dockerfile", ".dockerignore"}.issubset(inputs):
        raise ValueError("Container source COPY inventory is incomplete")
    files = set()
    for name in inputs:
        path = Path(name)
        if (path.is_absolute() or any(part in ("", ".", "..") for part in path.parts)
                or path.parts[0] not in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "Dockerfile", ".dockerignore", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "crates", "scripts", "third_party", ".cargo", ".config"}):
            raise ValueError("Container source COPY escaped public build inputs")
        copied = root / path
        if not copied.exists() or copied.is_symlink() or getattr(copied.lstat(), "st_file_attributes", 0) & 0x400:
            raise ValueError("Missing or redirected container source COPY input")
        candidates = copied.rglob("*") if copied.is_dir() else (copied,)
        for candidate in candidates:
            if candidate.is_symlink() or getattr(candidate.lstat(), "st_file_attributes", 0) & 0x400:
                raise ValueError("Redirected container source input")
            if candidate.is_file():
                if candidate.stat().st_nlink != 1:
                    raise ValueError("Aliased container source input")
                relative = candidate.relative_to(root).as_posix()
                if any(part.lower() in {"temp", ".airk", ".git", "target", "artifacts", ".codex", ".aws", ".ssh"} or part.lower().startswith(".env") for part in candidate.relative_to(root).parts) or candidate.suffix.lower() in (".pem", ".key"):
                    raise ValueError("Private state cannot enter the container source seal")
                files.add(relative)
    if len(files) > 16_384:
        raise ValueError("Container source inventory exceeds bound")
    result = hashlib.sha256()
    total = 0
    for name in sorted(files):
        path = root / name
        total += path.stat().st_size
        if path.stat().st_size > 64 * 1024 * 1024 or total > 512 * 1024 * 1024:
            raise ValueError("Container source byte budget exceeded")
        encoded = name.encode("utf-8")
        result.update(len(encoded).to_bytes(8, "big")); result.update(encoded)
        result.update(bytes.fromhex(builder.digest(path)))
    return {"sha256": result.hexdigest(), "files": len(files)}


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
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    modern = builder.single_image_version(version)
    source = container_source_identity()
    inputs = builder.verify_static_inputs() if modern else None
    if not modern:
        builder.check_lightweight_dependencies()
        helper = builder.cargo_binary("airk-embed", args.profile, "embedding-worker")
        shutil.copy2(helper, runtime / "airk-embed")
    main_binary = builder.cargo_binary("airk", args.profile, "local-inference" if modern else None)
    shutil.copy2(main_binary, runtime / "airk")
    if args.profile == "debug":
        # DWARF can exceed a gigabyte for the pair and dominates approval hashing.
        # Strip the staged copies before sealing; the build tree stays untouched.
        for executable in builder.runtime_executables("linux", version):
            subprocess.run(["strip", "--strip-debug", str(runtime / executable)], check=True)
    architecture = {"x86_64": "x86_64", "aarch64": "aarch64"}.get(platform.machine().lower())
    if not architecture:
        raise SystemExit("Unsupported Linux container architecture")
    if not modern:
        builder.stage_inference_runtime(runtime, "linux", architecture)
    builder.verify_embedded_models(runtime, "linux", version)
    imports = builder.verify_static_imports(runtime / "airk", "linux") if modern else None
    smoke = builder.verify_model_smoke(runtime, "linux", version) if modern else None
    if modern:
        if builder.verify_static_inputs() != inputs:
            raise RuntimeError("Static ORT compiled inputs changed during container build")
        builder.stage_static_notices(runtime, inputs)
    metadata = builder.write_pair_metadata(runtime, version, "linux", architecture)
    receipt = {
        "schema": 1, "kind": "airk-container-build", "version": version,
        "cargo_lock_sha256": builder.digest(ROOT / "Cargo.lock"),
        "main_features": ["local-inference"] if modern else [],
    }
    if modern:
        receipt.update(static_ort=inputs, static_imports=imports, embedded_model_smoke=smoke)
    else:
        receipt["helper_features"] = ["embedding-worker"]
    (runtime / "airk-container-build.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    build_receipt = {"schema": 1, "source": source, "profile": args.profile, "runtime": metadata,
        "rustc": subprocess.check_output(["rustc", "--version", "--verbose"], text=True, encoding="utf-8").strip(),
        "source_basis": "actual-public-container-copy-inputs/1"}
    if modern:
        build_receipt.update(static_ort=inputs, static_imports=imports, embedded_model_smoke=smoke)
    encoded = json.dumps(build_receipt, indent=2) + "\n"
    if len(encoded.encode("utf-8")) > 16_384:
        raise ValueError("Container build receipt exceeds native metadata bound")
    (runtime / "airk-build.json").write_text(encoded, encoding="utf-8")
    if container_source_identity() != source:
        raise ValueError("Container public source inputs changed during build")
    builder.seal_install_payload(runtime, version, "linux", architecture)
    if container_source_identity() != source:
        raise ValueError("Container public source inputs changed during payload sealing")
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
    print(f"Staged Linux {architecture} container runtime {version}; approval {approved}")


if __name__ == "__main__":
    main()
