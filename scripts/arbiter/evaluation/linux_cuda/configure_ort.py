# SPDX-License-Identifier: Apache-2.0
"""Prepare the isolated Linux scientific ORT source and offline CMake build graph."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import pwd
import runpy
import shutil
import stat
import subprocess
import time
import types

from graph_proof import snapshot
from unpack import directory, guarded


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
CPU = REPO / "scripts/provision-static-onnx-runtime.py"
BUILD = REPO / "scripts/cuda-static-runtime/build"
BASE = Path("/home/tyler/airk-builds/arbiter-final-linux-r1")
CAMPAIGN = REPO / "artifacts/v190/final-plan/exec-20261116T052942Z"
CPU_SHA = "a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044"
CONFIG_SHA = "d10eb3ced57d17901e3749f05864e1f230b0d8988558a3ddaa98d72f61c1a863"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_path(windows):
    name = PureWindowsPath(windows)
    if not name.is_absolute() or name.drive.lower() not in ("d:", "c:") or ".." in name.parts:
        raise ValueError("unexpected input location")
    return Path("/mnt") / name.drive[0].lower() / Path(*name.parts[1:])


def audit(root, uid):
    for path in root.rglob("*"):
        if path.is_symlink():
            guarded(path, uid, stat.S_ISLNK)
            if not path.resolve(strict=True).is_relative_to(root):
                raise ValueError("build tree link escapes: " + str(path))
        else:
            guarded(path, uid, lambda mode: stat.S_ISDIR(mode) or stat.S_ISREG(mode))


def exact_edit(path, before, after, count=1):
    data = path.read_bytes()
    if data.count(before) != count:
        raise ValueError("vendor source anchor differs: " + str(path))
    path.write_bytes(data.replace(before, after))


def prepare(root, uid, cpu, inputs):
    cuda_lock = json.loads((BUILD / "r8/cuda-runtime-producer-lock.json").read_bytes())
    spec = CAMPAIGN / "fresh-cuda-configure-cu15-r1.json"
    if sha(spec) != CONFIG_SHA:
        raise ValueError("source archive selection differs")
    recipe = json.loads(spec.read_bytes())
    base = recipe["source_archive"]
    archives = recipe["dependency_archives"]
    for row in [base, *archives.values()]:
        cpu.verify_file(source_path(row["path"]), row)
    cpu.extract_source(source_path(base["path"]), root, cpu.LOCK["source"])
    if sha(root/"source-files.json") != cpu.LOCK["source"]["inventory_sha256"]:
        raise ValueError("original source inventory differs")
    src = root / "source"
    for dep in [*cpu.selected_dependencies("x86_64-unknown-linux-gnu"), *cuda_lock["dependencies"]]:
        original = source_path(archives[dep["name"]]["path"])
        mirror = src / "mirror" / cpu.mirror_relative(dep)
        directory(mirror.parent, root, uid)
        with original.open("rb") as inf, mirror.open("xb") as out:
            shutil.copyfileobj(inf, out, 1 << 20)
        guarded(mirror, uid, stat.S_ISREG)
        cpu.verify_file(mirror, dep)
    r8 = runpy.run_path(str(BUILD / "r8/provision_cuda_runtime.py"))
    r8["safe_frontend"](cpu, source_path(archives["cudnn_frontend"]["path"]), root / "frontend", cuda_lock)
    audit(root, uid)
    path = src / "onnxruntime/core/session/onnxruntime_c_api.cc"
    steps = (("offline", BUILD / "r8/offline-helper.patch"), ("final-placement", BUILD / "r8/final-placement.patch"),
             ("initialized-public-api", BUILD / "r12/initialized-public-api.patch"),
             ("final-graph", BUILD.parent / "final-graph-metadata/collector.patch"))
    patch_pins = {}
    for label, patch in steps:
        raw = patch.read_bytes().replace(b"\r\n", b"\n")
        patch_pins[label] = hashlib.sha256(raw).hexdigest()
        subprocess.run(["/usr/bin/patch", "--batch", "--fuzz=0", "--forward", "-p1"], input=raw,
                       cwd=src, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=True, timeout=10)
    if sha(path) != "db00479a06d0a1e8fadf4cc834ffe5a6da0b745ee2820b06a0821fb1dbeca97b":
        raise ValueError("reviewed static collector source did not reproduce")
    for anchor in (b"using AirkOrtPlacementCallbackV1", b"struct AirkOrtGraphRecordV1"):
        start = b"#if defined(_WIN32)\n" + anchor
        data = path.read_bytes()
        if data.count(start) != 1:
            raise ValueError("collector guard differs")
        begin = data.index(start)
        finish = data.index(b"#endif  // defined(_WIN32)", begin)
        block = data[begin:finish].replace(b"#if defined(_WIN32)", b"#if defined(__linux__) && defined(__x86_64__)")
        block = block.replace(b"__cdecl", b"").replace(b"__declspec(dllexport)", b'__attribute__((visibility("default")))')
        path.write_bytes(data[:begin] + block + data[finish:])
    # Scope preloaded-only lookup to the two native ORT provider names.
    lib = src / "onnxruntime/core/platform/posix/env.cc"
    exact_edit(lib, b"*handle = dlopen(library_filename.c_str(), RTLD_NOW | (global_symbols ? RTLD_GLOBAL : RTLD_LOCAL));",
               b'const auto name = std::filesystem::path(library_filename).filename().string();\n'
               b'    const bool provider = name == "libonnxruntime_providers_cuda.so" || name == "libonnxruntime_providers_shared.so";\n'
               b'    *handle = dlopen(provider ? name.c_str() : library_filename.c_str(), RTLD_NOW | (provider ? RTLD_NOLOAD : 0) | (global_symbols ? RTLD_GLOBAL : RTLD_LOCAL));')
    from bootstrap_projection import project_env
    bootstrap = HERE / "ort_bootstrap.inc"
    prior_env = sha(lib)
    project_env(lib, bootstrap.read_bytes().replace(b"\r\n", b"\n"))
    modified = [path, lib]
    for name, soname in (("cudnn", "libcudnn.so.9"), ("cufft", "libcufft.so.11")):
        loader = src / ("onnxruntime/core/providers/cuda/" + name + "_loader.cc")
        exact_edit(loader, b"dlopen(candidate.c_str(), RTLD_NOW | RTLD_LOCAL)",
                   f'dlopen("{soname}", RTLD_NOW | RTLD_NOLOAD | RTLD_LOCAL)'.encode())
        modified.append(loader)
    for relative in ("include/cudnn_frontend_shim.h", "include/cudnn_frontend/experimental/nvrtc_shim.h"):
        shim = root / "frontend" / relative
        data = shim.read_bytes()
        # These symbols occur in Unix dlopen branches and in Win32 macro invocations.
        # Linux-only projected source leaves all other hosts' production owners intact.
        count = data.count(b", RTLD_NOW)")
        if count < 1:
            raise ValueError("cuDNN frontend dynamic import anchor missing")
        exact_edit(shim, b", RTLD_NOW)", b", RTLD_NOW | RTLD_NOLOAD)", count)
        modified.append(shim)
    return {"archive_sha256": base["sha256"], "baseline": sha(root / "source-files.json"),
            "patches": patch_pins, "unprotected_env_sha256": prior_env, "bootstrap_sha256": sha(bootstrap), "linux_sources": {str(p.relative_to(root)): sha(p) for p in modified}}


def run(args):
    uid = os.getuid()
    if uid != 1000 or os.geteuid() != uid or pwd.getpwuid(uid).pw_name != "tyler" or sha(CPU) != CPU_SHA:
        raise ValueError("unexpected user or process owner")
    os.umask(0o077)
    root = args.output
    if root.parent != BASE or os.path.lexists(root):
        raise ValueError("fresh isolated source/build root required")
    for p in (BASE, BASE.parent, BASE.parent.parent):
        guarded(p, uid, stat.S_ISDIR)
    root.mkdir(0o700)
    guarded(root, uid, stat.S_ISDIR)
    cpu = types.SimpleNamespace(**runpy.run_path(str(CPU)))
    inputs = BASE / "cuda-build-inputs-r1"
    start = time.monotonic()
    initial = prepare(root, uid, cpu, inputs)
    old = BASE / "inputs/static-ort/tools"
    cxx = BASE / "toolchain/bin/cxx"
    tools = {"cmake": old / "cmake/bin/cmake", "ninja": old / "ninja-build/ninja",
             "patch": Path("/usr/bin/patch"), "python": Path("/usr/bin/python3"),
             "compiler": BASE/"toolchain/bin/cc", "cxx": cxx}
    cmd = cpu.configure_argv(root, "x86_64-unknown-linux-gnu", {k: {"path": str(v)} for k,v in tools.items()})
    cmd[cmd.index("-Donnxruntime_USE_CUDA=OFF")] = "-Donnxruntime_USE_CUDA=ON"
    cuda = inputs / "cuda"
    flags = {"onnxruntime_BUILD_CUDA_EP_AS_PLUGIN": "OFF", "CMAKE_CUDA_ARCHITECTURES": "86",
             "CMAKE_CUDA_COMPILER": cuda/"bin/nvcc", "CMAKE_CUDA_HOST_COMPILER": cxx,
             "onnxruntime_CUDA_HOME": cuda, "CUDAToolkit_ROOT": cuda,
             "onnxruntime_CUDNN_HOME": inputs/"cudnn", "CUDNN_PATH": inputs/"cudnn",
             "FETCHCONTENT_SOURCE_DIR_CUDNN_FRONTEND": root/"frontend", "FETCHCONTENT_UPDATES_DISCONNECTED": "ON",
             "onnxruntime_NVCC_THREADS": "1", "onnxruntime_FLASH_NVCC_THREADS": "1",
             "CMAKE_CXX_STANDARD": "20", "CMAKE_POSITION_INDEPENDENT_CODE": "ON",
             "CMAKE_MODULE_LINKER_FLAGS": "-Wl,-soname,libonnxruntime_providers_cuda.so", "CMAKE_SKIP_BUILD_RPATH": "ON"}
    for opt in ("FLASH_ATTENTION", "MEMORY_EFFICIENT_ATTENTION"):
        flags["onnxruntime_USE_"+opt] = "ON"
    for opt in ("TRT_FUSED_ATTENTION", "LEAN_ATTENTION", "FP4_QMOE", "FP8_QMOE", "FPA_INTB_GEMM", "CUDA_NHWC_OPS"):
        flags["onnxruntime_USE_"+opt] = "OFF"
    cmd += ["-D"+k+"="+str(v) for k,v in flags.items()]
    tmp = root / "tmp"
    directory(tmp, root, uid)
    env = {"PATH": str(cuda/"bin")+":"+str(BASE/"toolchain/bin")+":/usr/bin:/bin", "CC": str(BASE/"toolchain/bin/cc"),
           "CXX": str(cxx), "TMPDIR": str(tmp), "HOME": str(BASE.parents[1]), "LC_ALL": "C", "LANG": "C",
           "CMAKE_BUILD_PARALLEL_LEVEL": "2", "PYTHONDONTWRITEBYTECODE": "1",
           "LIBRARY_PATH": str(cuda / "lib")}
    result = cpu.run_owned(cmd, root, root / "configure.log", 120, env)
    audit(root, uid)
    cache = root/"b/Release/CMakeCache.txt"
    ninja = root/"b/Release/build.ninja"
    sealed = snapshot(ninja.parent, {k:str(v) for k,v in tools.items()}, env) if cpu.successful(result) and ninja.is_file() else None
    return {"schema": "airk.private.wsl-cuda-ort-configure/2", "graph_proof": sealed, "source": initial, "root": str(root),
            "environment": env, "tools": {key: str(value) for key,value in tools.items()}, "command": result,
            "cache_sha256": sha(cache) if cache.is_file() else None,
            "ninja_sha256": sha(ninja) if ninja.is_file() else None,
            "configured": cpu.successful(result) and ninja.is_file(), "elapsed_seconds": time.monotonic()-start}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    print(json.dumps(run(p.parse_args()), sort_keys=True))
