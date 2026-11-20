# SPDX-License-Identifier: Apache-2.0
"""Reopen an approved Windows campaign installation through its installer and native selection."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

TARGET = "x86_64-pc-windows-msvc"
ROOT_NAMES = {"cpu":"windows-managed-cpu-r1", "cuda":"windows-managed-cuda-r1"}


def require(test, message):
    if not test:
        raise ValueError(message)


def path_in(path, root):
    path = Path(path)
    require(path.is_absolute() and path.is_relative_to(root) and path != root
            and path.resolve(strict=True) == path and
            not any(p.is_junction() or p.is_symlink() for p in (path,*path.parents)), "noncanonical campaign install path")
    return path


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()


def normal_path(value):
    # Rust canonicalize may return a local extended-length Windows path.
    text = str(value)
    if text.startswith("\\\\?\\") and len(text) > 6 and text[5:7] == ":\\":
        text = text[4:]
    return Path(text)


def record_path(arm, campaign, personal):
    if "managed_install" not in arm:
        return personal
    spec = arm["managed_install"]
    require(spec["variant"] in ROOT_NAMES, "unknown Windows package variant")
    root = campaign / ROOT_NAMES[spec["variant"]]
    path = path_in(spec["record"], root)
    require(path.name == "install-record.json", "original installer record required")
    return path


def validate_inspection(result, root, install, control, service, package, package_sha):
    policy = result["managed_policy"]
    require(result["payload_verified"] is True and result["state_error"] is None and isinstance(policy,dict) and
            result["approval"] == policy["approval"] and result["approval"]["manifest_sha256"] == result["selection"]["active"] == package_sha
            and result["approval"]["version"] == package["version"], "native installed payload approval differs")
    source = path_in(normal_path(policy["source"]),root)
    selected = path_in(normal_path(result["executable"]),root)
    require(normal_path(policy["install"]) == install and normal_path(result["control_dir"]) == control
            and selected == source / "airk.exe" and sha(selected) == policy["executable_sha256"] == service
            and sha(source / "airk-install.json") == package_sha, "native selected image, source or control differs")
    return source


def same_inspection(original, live):
    def identity(observation):
        text = observation["stdout"]
        require(hashlib.sha256(text.encode("utf-8")).hexdigest() == observation["stdout_sha256"],
                "original runtime inspect stdout changed")
        native = json.loads(text)
        return ([observation[key] for key in ("argv","original_record","record_sha256","source","variant","installer_sha256")],
                [native[key] for key in ("selection","approval","executable","managed_policy","control_dir","state_error","payload_verified")])
    require(identity(original) == identity(live), "native runtime selection or original installation changed")


def verify(arm, campaign, record, program, images):
    spec = arm["managed_install"]
    root = campaign / ROOT_NAMES[spec["variant"]]
    require(record["schema"] == "airk-install-record/1" and record["target"] == TARGET and
            record["installed_by"]["installer"] == "inno" and Path(record["locations"]["program"]) == program,
            "not the original Windows installer record")
    for name in ("program","data","state","cache","logs"):
        path_in(record["locations"][name],root)
    control = path_in(arm.get("installed_control",arm.get("control")),root)
    install = path_in(spec["install_dir"],root)
    proof = spec["installer_receipt"]
    receipt_path = path_in(proof["path"], campaign)
    require(sha(receipt_path) == proof["sha256"], "installer build receipt differs")
    package = json.loads(receipt_path.read_bytes())
    require(package["schema"] == "airk-local-service-installer/1" and package["result"] == "built" and
            package["target"] == TARGET and package["version"] == record["version"], "foreign Windows package")
    stem = "airk-service-" + record["version"] + "-" + TARGET + ("-installer" if package["signed"] else "-UNSIGNED-installer")
    installer = path_in(package["installer"],campaign)
    require(installer.name == stem + ".exe" and sha(installer) == package["installer_sha256"], "foreign installer stem or bytes")
    files = package["files"]
    for role,path in images.items():
        relative = path.relative_to(program).as_posix()
        require(files[relative]["sha256"] == record["executables"][role]["sha256"] and path_in(path,root) == path,
                "installed pair differs from package")
    product = [name for name in files if Path(name).name == "airk-cuda-runtime.json"]
    require(len(product) == (1 if spec["variant"] == "cuda" else 0), "package CUDA variant differs")
    if product:
        descriptor = path_in(program/product[0],root)
        require(sha(descriptor) == files[product[0]]["sha256"] and
                json.loads(descriptor.read_bytes())["schema"] == "airk.native.cuda-runtime-manifest/2", "foreign CUDA product descriptor")
    # runtime inspect may open the existing registry. Resolve the native layout
    # before that command, so a foreign state root is refused before it can be touched.
    layout = json.loads(path_in(install/"runtime-layout.json",root).read_bytes())
    policy = json.loads(path_in(install/"managed-runtime.json",root).read_bytes())
    require(layout["ready"] is True and normal_path(layout["install"]) == normal_path(policy["install"]) == install
            and path_in(normal_path(layout["state"]),root)/"control" == control,
            "foreign or incomplete installed state layout")
    if layout.get("logs"):
        path_in(normal_path(layout["logs"]),root)
    selected = path_in(normal_path(policy["source"]),root)
    require(sha(path_in(selected/"airk.exe",root)) == record["executables"]["airk"]["sha256"]
            and sha(path_in(selected/"airk-install.json",root)) == spec["native_install_manifest_sha256"],
            "foreign managed source or native manifest")
    argv = [str(images["airk"]), "__airk_role", "setup-interim", "runtime", "inspect", "--install-dir", str(install)]
    env = {k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_","ORT_","CUDA_","RUST_LOG"))}
    result = subprocess.run(argv, capture_output=True, check=True, timeout=30, env=env, stdin=subprocess.DEVNULL)
    require(len(result.stdout) <= 4 << 20, "native inspection too large")
    native = json.loads(result.stdout)
    source = validate_inspection(native,root,install,control,record["executables"]["airk"]["sha256"],
                                 package,spec["native_install_manifest_sha256"])
    return {"argv":argv,"stdout_sha256":hashlib.sha256(result.stdout).hexdigest(),"stdout":result.stdout.decode("utf-8"),
            "original_record":str(program/"install-record.json"),"record_sha256":arm["install_record_sha256"],
            "source":str(source),"variant":spec["variant"],"installer_sha256":package["installer_sha256"]}
