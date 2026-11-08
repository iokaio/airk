# SPDX-License-Identifier: Apache-2.0
"""Build an installation-free AIRK runtime for this platform under builds/.

Shared by build.ps1 and build.sh. The output folder name starts with a UTC
timestamp (YYYYMMDD-HHMMSS) so folders sort chronologically. Nothing is
installed: no PATH, profile, login-startup or per-user runtime changes.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
from datetime import datetime, timezone
from pathlib import Path
import platform
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
OPERATING_SYSTEMS = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}
ARCHITECTURES = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}

TARGETS = {("windows", "x86_64"): "x86_64-pc-windows-msvc",
           ("linux", "x86_64"): "x86_64-unknown-linux-gnu",
           ("macos", "x86_64"): "x86_64-apple-darwin",
           ("macos", "aarch64"): "aarch64-apple-darwin"}
# Exact public dependency closure for the copied canonical experiment. The model
# catalog also supports the explicit optional predecessor helper; no private or
# generated checkout state is copied.
SUPPORT_FILES = {
    "scripts/check-service-clients.py": "tools/check-service-clients.py",
    "scripts/check-runtime-upgrade.py": "tools/check-runtime-upgrade.py",
    "scripts/http_fixture.py": "tools/http_fixture.py",
    "scripts/owned-runtime-processes.py": "tools/owned-runtime-processes.py",
    "crates/airk-retrieval/src/model_catalog.json": "crates/airk-retrieval/src/model_catalog.json",
    "examples/local_https_client.py": "examples/local_https_client.py",
}


def load_script(name):
    specification = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()


def stage_support(output):
    recorded = {}
    for source_name, destination_name in SUPPORT_FILES.items():
        source, destination = ROOT / source_name, output / destination_name
        metadata = source.lstat()
        if (not source.is_file() or source.is_symlink() or metadata.st_nlink != 1
                or getattr(metadata,"st_file_attributes",0)&0x400 or metadata.st_size > 1024*1024):
            raise RuntimeError("Local experiment support must be bounded public regular files")
        expected = digest(source)
        destination.parent.mkdir(parents=True,exist_ok=True)
        with source.open("rb") as reader, destination.open("xb") as writer:
            shutil.copyfileobj(reader,writer,1024*1024)
        if digest(destination) != expected or digest(source) != expected:
            raise RuntimeError("Local experiment support changed during its copy")
        recorded[destination_name] = {"source":source_name,"sha256":expected}
    (output / "tools/input-manifest.json").write_text(json.dumps({"schema":1,"files":recorded},indent=2)+"\n",encoding="utf-8")
    return recorded


def build_environment(args,target,output):
    environment = dict(os.environ)
    models = Path(environment.get("AIRK_BUNDLED_MODEL_DIR",ROOT / "target/local-vector-model"))
    if not models.is_absolute():
        raise RuntimeError("AIRK_BUNDLED_MODEL_DIR must be absolute")
    model_command = [sys.executable,str(ROOT / "scripts/provision-local-model.py"),"--destination",str(models)]
    if args.download_inputs:
        model_command.append("--download")
    subprocess.run(model_command,cwd=ROOT,env=environment,check=True)
    environment["AIRK_BUNDLED_MODEL_DIR"] = str(models)
    handoff = args.static_environment_file
    if handoff is None:
        handoff = output / "static-environment.json"
        destination = args.static_ort or ROOT / "target/static-onnx-runtime" / target
        command = [sys.executable,str(ROOT / "scripts/provision-static-onnx-runtime.py"),"--destination",str(destination),"--target",target,"--environment-file",str(handoff)]
        command += ["--download","--download-tools","--build","--parallel","2"] if args.download_inputs else ["--verify"]
        subprocess.run(command,cwd=ROOT,env=environment,check=True)
    verified = load_script("static-runtime-environment").load_environment_file(handoff,args.static_ort,target)
    environment.update(verified)
    return environment


def git(*args):
    try:
        return subprocess.check_output(["git", "-c",f"safe.directory={ROOT.as_posix()}",*args], cwd=ROOT, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--profile", choices=["release", "debug"], default="release")
    parser.add_argument("--static-ort",type=Path,help="Verified native static ORT destination (default target/static-onnx-runtime/<target>)")
    parser.add_argument("--static-environment-file",type=Path,help="Existing complete verified static input handoff")
    parser.add_argument("--download-inputs",action="store_true",help="Explicitly download pinned models/tools/source and build static ORT")
    args = parser.parse_args()
    if args.download_inputs and args.static_environment_file:
        parser.error("Choose an existing static handoff or explicit input provisioning")
    if args.static_ort is not None:
        args.static_ort = args.static_ort.absolute()
    if args.static_environment_file is not None:
        args.static_environment_file = args.static_environment_file.absolute()
    operating_system = OPERATING_SYSTEMS.get(platform.system())
    architecture = ARCHITECTURES.get(platform.machine().lower())
    target = TARGETS.get((operating_system,architecture))
    if not target:
        raise SystemExit(f"Unsupported build host: {platform.system()} {platform.machine()}")
    built = datetime.now(timezone.utc)
    name = f"{built:%Y%m%d-%H%M%S}-{operating_system}-{architecture}"
    output = ROOT / "builds" / name
    if output.exists():
        raise SystemExit(f"Build folder already exists: {output}")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    builder = load_script("build-service-runtime")
    if not builder.single_image_version(version):
        raise SystemExit("Use the version-matched historical local-build script for schema-1 runtimes")
    output.mkdir(parents=True)
    environment = build_environment(args,target,output)
    # The runtime builder stages, verifies and seals the complete offline payload.
    subprocess.run([sys.executable, str(ROOT / "scripts" / "build-service-runtime.py"),
                    "--destination", str(output / "runtime"), "--profile", args.profile], cwd=ROOT, env=environment, check=True)
    relative_binary = builder.runtime_executables(operating_system,version)[0]
    runtime = output / "runtime"
    manifest = json.loads((runtime / "airk-install.json").read_text(encoding="utf-8"))
    metadata = json.loads((runtime / "airk-runtime.json").read_text(encoding="utf-8"))
    if (any(manifest.get(name) != value for name,value in (("schema",1),("version",version),("os",operating_system),("arch",architecture)))
            or not builder.runtime_metadata_matches(metadata,version,operating_system,architecture,manifest["files"])
            or metadata["schema"] != 2 or digest(runtime / relative_binary) != metadata["airk_sha256"]):
        raise SystemExit("The runtime builder did not produce the exact current single-main inventory")
    forbidden = {"airk-embed","airk-embed.exe","airk-launcher","airk-launcher.exe","airk-service","airk-service.exe","airk-index","airk-index.exe","airk-source","airk-source.exe","onnxruntime.dll","libonnxruntime.so","libonnxruntime.dylib"}
    if any(Path(name).name in forbidden for name in manifest["files"]):
        raise SystemExit("Current runtime inventory contains a retired sidecar or alias")
    stage_support(output)
    binary = "runtime/" + relative_binary
    status = git("status","--porcelain")
    values = {"VERSION":version,"PLATFORM":f"{operating_system}-{architecture}","BUILT":f"{built:%Y-%m-%d %H:%M:%S} UTC",
              "COMMIT":git("rev-parse","HEAD") or "unknown","DIRTY":" (working-tree state unavailable)" if status is None else " (with uncommitted changes)" if status else "",
              "PROFILE":args.profile,"BIN":binary}
    readme = (ROOT / "scripts" / "local-build-README.md").read_text(encoding="utf-8")
    for key, value in values.items():
        readme = readme.replace("{{" + key + "}}", value)
    (output / "README.md").write_text(readme, encoding="utf-8", newline="\n")
    print(f"Local AIRK {version} build ({args.profile}): {output}")
    print(f"Run the owned canonical experiment: {sys.executable} {output / 'tools/check-service-clients.py'} --runtime-dir {output / 'runtime'} --files 1; see README.md")


if __name__ == "__main__":
    main()
