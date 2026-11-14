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


def docker_instructions(recipe):
    """Read bounded logical Docker instructions without parsing unrelated shell code.

    Only the default Docker escape character is supported by this source sealer.
    Heredocs need their own reviewed parser before they can carry COPY authority.
    Full-line comments disappear during continuation; inline '#' remains data.
    """
    if len(recipe.encode("utf-8")) > 512 * 1024:
        raise ValueError("Container recipe exceeds source seal byte bound")
    pending = ""
    for ordinal, physical in enumerate(recipe.splitlines()):
        if ordinal >= 8192:
            raise ValueError("Container recipe exceeds source seal line bound")
        line = physical.strip()
        if not line:
            continue
        if line.startswith("#"):
            directive = line[1:].strip().lower().replace(" ", "")
            if directive.startswith("escape=") and directive != "escape=\\":
                raise ValueError("Container recipe escape directive needs an explicit reviewed parser")
            continue
        continued = line.endswith("\\")
        pending += line[:-1] if continued else line
        if len(pending.encode("utf-8")) > 64 * 1024:
            raise ValueError("Container recipe instruction exceeds source seal byte bound")
        if continued:
            continue
        # Docker also permits tabs between the opcode and arguments.
        fields = pending.split(None, 1)
        instruction, arguments = fields[0].upper(), fields[1] if len(fields) == 2 else ""
        if "<<" in arguments:
            raise ValueError("Container recipe heredoc needs an explicit reviewed parser")
        yield instruction, arguments
        pending = ""
    if pending:
        raise ValueError("Container recipe ends inside a continued instruction")


def public_input_manifest(root):
    """Exact external files; only the existing three owning trees are recursive."""
    path = root / "scripts/container-inputs.json"
    for candidate in (path, *path.parents):
        if candidate == root:
            break
        if candidate.is_symlink() or getattr(candidate.lstat(), "st_file_attributes", 0) & 0x400:
            raise ValueError("Redirected container public input manifest")
    with path.open("rb") as stream:
        encoded = stream.read(1024 * 1024 + 1)
    if len(encoded) > 1024 * 1024:
        raise ValueError("Container public input manifest exceeds byte budget")
    manifest = json.loads(encoded)
    if (not isinstance(manifest, dict) or manifest.get("schema") != 1
            or set(manifest) != {"schema", "trees", "files"}
            or not isinstance(manifest["files"], list) or not isinstance(manifest["trees"], list)
            or any(not isinstance(name, str) for name in manifest["files"] + manifest["trees"])
            or len(set(manifest["files"])) != len(manifest["files"])
            or len(set(manifest["trees"])) != len(manifest["trees"])
            or len(manifest["files"]) > 16_384
            or not set(manifest["trees"]).issubset({"crates", "third_party/models", "third_party/licenses",
                                                       "fixtures/split/parity", "fixtures/split/ui_pilot"})):
        raise ValueError("Container public input manifest differs from its reviewed schema")
    for name in manifest["files"]:
        path = Path(name)
        if (not name or not path.parts or path.anchor or path.as_posix() != name
                or any(part in ("", ".", "..") for part in path.parts)
                or any(char in name for char in "\\*?[$:")
                or any(ord(char) < 32 for char in name)):
            raise ValueError("Container public build inputs require exact relative file paths")
    return set(manifest["files"]), set(manifest["trees"])


def container_source_identity(root=None):
    """Seal actual public COPY inputs; this is not a Git/release attestation."""
    root = Path(root or ROOT).resolve(strict=True)
    source_stage = False
    inputs = set()
    with (root / "Dockerfile").open("rb") as stream:
        recipe = stream.read(512 * 1024 + 1).decode("utf-8")
    for instruction, arguments in docker_instructions(recipe):
        if instruction == "FROM":
            if source_stage:
                break
            words = shlex.split(arguments)
            source_stage = len(words) >= 3 and words[-2].upper() == "AS" and words[-1].lower() == "source"
        elif source_stage and instruction == "ADD":
            raise ValueError("Container source ADD needs an explicit reviewed inventory")
        elif source_stage and instruction == "COPY":
            # COPY is Docker syntax, not a shell: inline '#' is an input token,
            # and unreviewed flags/JSON/expansion must not silently shrink its seal.
            if arguments.lstrip().startswith("["):
                raise ValueError("Container source COPY form needs an explicit reviewed inventory")
            words = shlex.split(arguments)
            # Separately verified C++/tool build outputs are bound by static_ort,
            # not misrepresented as original repository COPY inputs.
            if words == ["--from=static-inference", "/build/static-onnx-runtime", "/build/static-onnx-runtime"]:
                continue
            if len(words) < 2 or any(word.startswith("--") for word in words):
                raise ValueError("Container source COPY form needs an explicit reviewed inventory")
            if any(any(char in word for char in "$\\*?[") for word in words[:-1]):
                raise ValueError("Container source COPY expansion needs an explicit reviewed inventory")
            inputs.update(words[:-1])
    if not inputs or not {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates", "Dockerfile", ".dockerignore", "scripts/container-inputs.json"}.issubset(inputs):
        raise ValueError("Container source COPY inventory is incomplete")
    public_files, public_trees = public_input_manifest(root)
    files = set()
    for name in inputs:
        path = Path(name)
        if (path.is_absolute() or any(part in ("", ".", "..") for part in path.parts)
                or path.as_posix() not in public_files | public_trees):
            raise ValueError("Container source COPY escaped public build inputs")
        copied = root / path
        if not copied.exists() or copied.is_symlink() or getattr(copied.lstat(), "st_file_attributes", 0) & 0x400:
            raise ValueError("Missing or redirected container source COPY input")
        if copied.is_dir() != (name in public_trees):
            raise ValueError("Container public build inputs require exact files outside owning trees")
        for parent in copied.parents:
            if parent == root:
                break
            if parent.is_symlink() or getattr(parent.lstat(), "st_file_attributes", 0) & 0x400:
                raise ValueError("Redirected container source input")
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
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if not builder.single_image_version(version):
        # This tree no longer contains the airk-embed helper target. Historical
        # pair payloads are built from their tagged sources and build scripts.
        raise SystemExit(f"Version {version} needs a historical runtime pair; build it from its tagged source")
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    runtime = destination / "runtime"
    runtime.mkdir()
    source = container_source_identity()
    inputs = builder.verify_static_inputs()
    split = builder.split_pair_version(version)
    admin_inventory = None
    admin_block = None
    if split:
        admin = builder.load_script("build-admin-assets")
        inventory_path = ROOT / "artifacts/admin/admin-assets.json"
        admin_inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
        admin_block = admin.block_from_inventory(admin_inventory)
        builder.check_lightweight_dependencies(unified=True)
        main_binary, cli_binary = builder.cargo_pair(args.profile)
        shutil.copy2(cli_binary, runtime / "airkctl")
    else:
        main_binary = builder.cargo_binary("airk", args.profile, "local-inference")
    shutil.copy2(main_binary, runtime / "airk")
    if args.profile == "debug":
        # Strip only staged copies; both images remain bound by final metadata.
        for executable in builder.runtime_executables("linux", version):
            subprocess.run(["strip", "--strip-debug", str(runtime / executable)], check=True)
    architecture = {"x86_64": "x86_64", "aarch64": "aarch64"}.get(platform.machine().lower())
    if not architecture:
        raise SystemExit("Unsupported Linux container architecture")
    receipt = {"schema": 1, "kind": "airk-container-build", "version": version,
               "cargo_lock_sha256": builder.digest(ROOT / "Cargo.lock"),
               "source": source, "source_basis": "actual-public-container-copy-inputs/1",
               "main_features": ["local-inference"], "static_ort": inputs}
    def container_receipt(build):
        # Retain the container observation fields as well as the canonical pair
        # receipt. Write before the install inventory binds every payload file.
        receipt.update({key: build[key] for key in ("static_imports", "embedded_model_smoke")})
        (runtime / "airk-container-build.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    builder.qualify_staged_runtime(runtime, version, "linux", architecture, args.profile,
                                   source, inputs, admin_inventory, admin_block,
                                   observe_source=container_source_identity, before_seal=container_receipt)
    approved = builder.digest(runtime / "airk-install.json")
    # The image owns the fixed approval, outside the provider payload. Neither
    # workspace files nor environment can nominate a replacement approval.
    selector = "__airk_role setup-interim " if split else ""
    entrypoint = destination / "airk-container"
    entrypoint.write_text("#!/bin/sh\nexec /usr/lib/airk/airk " + selector
                          + "runtime container-host --source /usr/lib/airk "
                          + f"--approved-sha256 {approved} \"$@\"\n", encoding="utf-8")
    entrypoint.chmod(0o755)
    command = [str(runtime / "airk"), *(["__airk_role", "setup-interim"] if split else []), "runtime", "self-test"]
    result = json.loads(subprocess.check_output(command, text=True))
    if result.get("self_test") != "passed" or result.get("version") != version:
        raise SystemExit("Container pair self-test failed")
    print(f"Staged Linux {architecture} container runtime {version}; approval {approved}")


if __name__ == "__main__":
    main()
