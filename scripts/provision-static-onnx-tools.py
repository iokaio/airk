# SPDX-License-Identifier: Apache-2.0
"""Owned build-only CMake/Ninja provisioning. No system installation or PATH edits."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import posixpath
import re
import shutil
import sys
import tarfile
import time
import types
import zipfile

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "scripts/static-onnx-tools-lock.json"
LOCK = json.loads(LOCK_PATH.read_text())
MAX_MEMBERS = 100000
MAX_EXPANDED = 1024 * 1024 * 1024
MAX_FILE = 128 * 1024 * 1024


def core_module():
    spec = importlib.util.spec_from_file_location("airk_static_tools_core", ROOT / "scripts/provision-static-onnx-runtime.py")
    core = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(core)
    return core


def validate_lock(core):
    if (LOCK.get("schema") != 1 or LOCK.get("cmake_version") != "3.31.6" or LOCK.get("ninja_version") != "1.12.1"
            or LOCK.get("ninja_commit") != "2daa09ba270b0a43e1929d29b073348aa985dfaa"
            or set(LOCK["cmake"]) != {"Windows", "Linux", "Darwin"}):
        raise ValueError("Pinned static build tools version/target roster differs")
    for system, row in LOCK["cmake"].items():
        core.archive_url(row["url"])
        if (not row["url"].startswith("https://github.com/Kitware/CMake/releases/download/v3.31.6/")
                or not 0 < row["bytes"] <= 100 * 1024 * 1024 or not re.fullmatch(r"[0-9a-f]{64}", row["sha256"])):
            raise ValueError("Pinned CMake archive identity differs")
        core.relative_name(row["archive_root"])
        core.relative_name(row["program"])
    ninja = LOCK["ninja"]
    if (ninja["url"] != "https://codeload.github.com/ninja-build/ninja/tar.gz/" + LOCK["ninja_commit"]
            or ninja["archive_root"] != "ninja-" + LOCK["ninja_commit"] or not 0 < ninja["bytes"] <= 2 * 1024 * 1024
            or not re.fullmatch(r"[0-9a-f]{64}", ninja["sha256"])):
        raise ValueError("Pinned Ninja source identity differs")


def fetch(archive, row, download, core, ninja=False):
    # The common downloader admits codeload specifically for ORT's source;
    # Ninja's separately pinned exact-commit source uses the same HTTPS policy.
    if not ninja:
        core.fetch(archive, row, download)
        return
    if archive.exists():
        core.verify_file(archive, row)
        return
    if not download:
        raise ValueError("Missing offline pinned Ninja source archive")
    core.fetch(archive, row, download, source=True)


def archive_entries(archive, row, core, destination=None):
    """Validate archive bytes; only authenticated CMake vendor links are retained."""
    core.verify_file(archive, row)
    is_zip = archive.name.endswith(".zip")
    container = zipfile.ZipFile(archive) if is_zip else tarfile.open(archive, mode="r|gz")
    members = container.infolist() if is_zip else container
    entries = []
    seen = set()
    expanded = 0
    try:
        for ordinal, member in enumerate(members):
            if ordinal >= MAX_MEMBERS:
                raise ValueError("Pinned build-tool member count exceeds budget")
            name = core.relative_name(member.filename.rstrip("/") if is_zip else member.name)
            if name.parts[0] != row["archive_root"]:
                raise ValueError("Pinned build-tool archive root differs")
            if len(name.parts) == 1:
                if not (member.is_dir() if is_zip else member.isdir()) or "" in seen:
                    raise ValueError("Invalid or duplicate build-tool archive root")
                seen.add("")
                continue
            relative = PurePosixPath(*name.parts[1:]).as_posix()
            key = relative.casefold() if os.name == "nt" else relative
            if key in seen:
                raise ValueError("Duplicate build-tool archive member")
            seen.add(key)
            mode = member.external_attr >> 16 if is_zip else member.mode
            directory = member.is_dir() if is_zip else member.isdir()
            link = ((mode & 0o170000) == 0o120000) if is_zip else member.issym() or member.islnk()
            regular = (not directory and (mode & 0o170000) in (0, 0o100000)) if is_zip else member.isfile()
            size = member.file_size if is_zip else member.size
            if directory:
                entries.append({"path": relative, "kind": "directory"})
                if destination is not None:
                    core.child(destination, relative).mkdir(parents=True, exist_ok=True)
                continue
            if link:
                if size > 4096:
                    raise ValueError("Build-tool archive link exceeds budget")
                if is_zip:
                    target = container.read(member).decode("utf-8", errors="strict")
                else:
                    target = member.linkname
                if target.startswith("/") or "\\" in target or ":" in target or any(ord(char) < 32 for char in target):
                    raise ValueError("Unsafe internal build-tool archive link")
                # Validate the link reference but omit it. CMake's actual program
                # and module paths below are regular archive members.
                combined = posixpath.normpath(posixpath.join(str(name.parent), target))
                resolved = core.relative_name(combined)
                if resolved.parts[0] != row["archive_root"]:
                    raise ValueError("Build-tool archive link escapes its root")
                vendor = row.get("program", "").startswith("CMake.app/") and (
                    relative.startswith("CMake.app/Contents/Frameworks/Qt") or relative == "CMake.app/Contents/bin/cmake-gui")
                if vendor and not (is_zip or member.issym()):
                    raise ValueError("CMake vendor hard links are unsupported")
                entries.append({"path": relative, "kind": "vendor_symlink" if vendor else "omitted_link", "target": target})
                continue
            if not regular or not 0 <= size <= MAX_FILE:
                raise ValueError("Unsupported build-tool archive member type/size")
            expanded += size
            if expanded > MAX_EXPANDED:
                raise ValueError("Build-tool archive expansion exceeds budget")
            stream = container.open(member) if is_zip else container.extractfile(member)
            if destination is None:
                with stream:
                    sha = core.stream_digest(stream, "sha256").hexdigest()
            else:
                path = core.child(destination, relative)
                path.parent.mkdir(parents=True, exist_ok=True)
                digest = hashlib.sha256()
                copied = 0
                with stream, path.open("xb") as output:
                    while content := stream.read(1024 * 1024):
                        copied += len(content)
                        if copied > size:
                            raise ValueError("Build-tool archive member exceeds declared size")
                        output.write(content)
                        digest.update(content)
                if copied != size:
                    raise ValueError("Build-tool archive member size differs")
                sha = digest.hexdigest()
                if os.name != "nt":
                    path.chmod(0o755 if mode & 0o111 else 0o644)
            entries.append({"path": relative, "kind": "file", "bytes": size, "sha256": sha,
                            "executable": bool(mode & 0o111)})
    finally:
        container.close()
    entries = sorted(entries, key=lambda entry: entry["path"])
    validate_vendor_links(entries)
    return entries


def extract(archive, destination, row, core):
    if destination.exists():
        entries = archive_entries(archive, row, core)
        complete_vendor_links(destination, entries, core)
        verify_tree(destination, entries, core)
        return entries
    if not archive.name.endswith(".zip"):
        # Validate and copy each gzip member as it arrives. Sorting before lookup
        # forces backward decompression; stream mode makes that impossible.
        entries = archive_entries(archive, row, core, destination=destination)
        destination.mkdir(parents=True, exist_ok=True)
        complete_vendor_links(destination, entries, core)
        verify_tree(destination, entries, core)
        return entries
    entries = archive_entries(archive, row, core)
    destination.mkdir(parents=True)
    is_zip = archive.name.endswith(".zip")
    container = zipfile.ZipFile(archive) if is_zip else tarfile.open(archive, "r:gz")
    try:
        for entry in entries:
            if entry["kind"] in ("omitted_link", "vendor_symlink"):
                continue
            path = core.child(destination, entry["path"])
            if entry["kind"] == "directory":
                path.mkdir(parents=True, exist_ok=True)
                continue
            path.parent.mkdir(parents=True, exist_ok=True)
            name = row["archive_root"] + "/" + entry["path"]
            stream = container.open(name) if is_zip else container.extractfile(name)
            with stream, path.open("xb") as output:
                shutil.copyfileobj(stream, output, 1024 * 1024)
            core.verify_file(path, entry)
            if os.name != "nt":
                path.chmod(0o755 if entry["executable"] else 0o644)
    finally:
        container.close()
    complete_vendor_links(destination, entries, core)
    verify_tree(destination, entries, core)
    return entries


def validate_vendor_links(entries):
    links = {entry["path"]: entry["target"] for entry in entries if entry["kind"] == "vendor_symlink"}
    known = {entry["path"] for entry in entries if entry["kind"] in ("file", "directory", "vendor_symlink")}
    known |= {parent.as_posix() for name in known for parent in PurePosixPath(name).parents}
    for name, target in links.items():
        pending = posixpath.normpath(posixpath.join(posixpath.dirname(name), target)).split("/")
        resolved = []
        visited = set()
        while pending:
            resolved.append(pending.pop(0))
            candidate = "/".join(resolved)
            if candidate in links:
                if candidate in visited or len(visited) >= 32:
                    raise ValueError("CMake vendor link cycle or depth exceeded")
                visited.add(candidate)
                redirected = posixpath.normpath(posixpath.join(posixpath.dirname(candidate), links[candidate]))
                pending = redirected.split("/") + pending
                resolved = []
        final = "/".join(resolved)
        if not final.startswith("CMake.app/") or final not in known or any(piece in ("", ".", "..") for piece in resolved):
            raise ValueError("CMake vendor link escapes or misses its authenticated bundle")


def complete_vendor_links(destination, entries, core):
    # Audit every original regular byte and existing link before any additions.
    verify_tree(destination, entries, core, allow_missing_vendor_links=True)
    for entry in entries:
        if entry["kind"] != "vendor_symlink":
            continue
        path = Path(destination).joinpath(*PurePosixPath(entry["path"]).parts)
        core.plain(path.parent)
        if not path.is_symlink():
            os.symlink(entry["target"], path)


def verify_tree(destination, entries, core, allow_missing_vendor_links=False):
    validate_vendor_links(entries)
    expected = {entry["path"] for entry in entries if entry["kind"] == "file"}
    links = {entry["path"]: entry for entry in entries if entry["kind"] == "vendor_symlink"}
    actual = set()
    seen_links = set()
    for path in destination.rglob("*"):
        relative = path.relative_to(destination).as_posix()
        if path.is_symlink():
            core.plain(path.parent)
            if relative not in links or os.readlink(path) != links[relative]["target"]:
                raise ValueError("Unapproved or changed CMake vendor link")
            seen_links.add(relative)
            continue
        core.plain(path)
        if path.is_file():
            actual.add(relative)
    for name in links:
        path = destination / name
        if path.exists() and not path.is_symlink():
            raise ValueError("CMake vendor link path has been replaced by another type")
    if actual != expected or not allow_missing_vendor_links and seen_links != set(links):
        raise ValueError("Owned pinned build-tool tree has missing/extra files or links")
    for entry in entries:
        if entry["kind"] == "file":
            path = core.child(destination, entry["path"])
            core.verify_file(path, entry)
            if os.name != "nt" and bool(path.stat().st_mode & 0o111) != entry["executable"]:
                raise ValueError("Pinned build-tool executable mode changed")
    if not allow_missing_vendor_links:
        for name in links:
            resolved = (destination / name).resolve(strict=True)
            if not resolved.is_relative_to(destination / "CMake.app"):
                raise ValueError("CMake vendor link resolved outside its bundle")
            core.plain(resolved)


def cmake_cli_closure(entries, row):
    files = {entry["path"] for entry in entries if entry["kind"] == "file"}
    prefix = "CMake.app/Contents/" if row["program"].startswith("CMake.app/") else ""
    required = {row["program"], *(prefix + "share/cmake-3.31/Modules/" + name for name in
                 ("CMakeDetermineSystem.cmake", "CMakeDetermineCXXCompiler.cmake", "CMakeInitializeConfigs.cmake"))}
    if not required.issubset(files):
        raise ValueError("Pinned CMake CLI/module closure is incomplete")
    for entry in entries:
        if entry["kind"] == "omitted_link" and not (entry["path"].startswith("CMake.app/Contents/Frameworks/Qt") or entry["path"] == "CMake.app/Contents/bin/cmake-gui"):
            raise ValueError("Pinned CMake omission affects more than the unused GUI closure")


def setup_owner(destination, target, core, create=False):
    destination = core.owned_destination(destination, target)
    tools = core.child(destination, "tools")
    owner = {"schema": 1, "owner": "AIRK-static-build-tools", "target": target, "lock_sha256": core.digest(LOCK_PATH)}
    if (tools / "owner.json").exists():
        if core.read_json(tools / "owner.json") != owner:
            raise ValueError("Different static tools owner/lock")
    elif not create:
        raise ValueError("Static build tools have no ownership receipt")
    elif tools.exists() and any(tools.iterdir()):
        raise ValueError("Refusing to adopt a foreign nonempty build-tools directory")
    else:
        tools.mkdir(parents=True, exist_ok=True)
        core.write_json(tools / "owner.json", owner)
    return tools


def ninja_commands(root, target, cmake, options, core):
    source, build = root / "ninja-source", root / "ninja-build"
    windows = core.TARGETS[target][0] == "Windows"
    command = [str(cmake), "-S", str(source), "-B", str(build), "-G", "NMake Makefiles" if windows else "Unix Makefiles",
               "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTING=OFF", "-DNINJA_BUILD_BINARY=ON", "-DRE2C=OFF",
               "-DNINJA_PYTHON=" + str(Path(sys.executable).resolve())]
    tools = None
    if windows:
        candidates = [options.vs_root] if options.vs_root else [Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Microsoft Visual Studio/2022" / edition for edition in ("Enterprise", "Professional", "Community", "BuildTools")]
        vs = next((Path(value).resolve() for value in candidates if value and (Path(value) / "VC/Auxiliary/Build/vcvars64.bat").is_file()), None)
        if vs is None:
            raise ValueError("Ninja source build needs existing pinned VS2022 tools")
        compiler_root = vs / "VC/Tools/MSVC" / options.msvc_toolset / "bin/Hostx64/x64"
        if not (compiler_root / "nmake.exe").is_file() or not (compiler_root / "cl.exe").is_file():
            raise ValueError("Ninja source build needs pinned MSVC toolset")
        tools = {"vcvars": {"path": str(vs / "VC/Auxiliary/Build/vcvars64.bat")}, "windows_sdk": options.windows_sdk, "msvc_toolset": options.msvc_toolset}
        command += ["-DCMAKE_MAKE_PROGRAM=" + str(compiler_root / "nmake.exe")]
    else:
        make = core.program_path(None, "make")
        compiler = core.program_path(options.cxx, "c++")
        command += ["-DCMAKE_MAKE_PROGRAM=" + str(make), "-DCMAKE_CXX_COMPILER=" + str(compiler)]
        if core.TARGETS[target][1] == "x86_64":
            command += ["-DCMAKE_CXX_FLAGS=-march=x86-64 -mtune=generic"]
        if core.TARGETS[target][0] == "Darwin":
            command += ["-DCMAKE_OSX_ARCHITECTURES=" + ("arm64" if core.TARGETS[target][1] == "aarch64" else "x86_64"), "-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3"]
    return command, [str(cmake), "--build", str(build), "--target", "ninja", "--config", "Release", "--parallel", "2"], tools


def archive_api():
    return types.SimpleNamespace(extract=extract, archive_entries=archive_entries, verify_tree=verify_tree)


def provision_tools(destination, target, download, options, core=None):
    core = core or core_module()
    validate_lock(core)
    if target != core.host_target():
        raise ValueError("Build-tool preparation supports native host targets only")
    root = setup_owner(destination, target, core, create=True)
    if (root / "tool-receipt.json").exists():
        return verify_tools(destination, target, core)
    if (root / "ninja-build").exists():
        raise ValueError("Incomplete Ninja build retained; no automatic retry")
    system = core.TARGETS[target][0]
    cmake_row = LOCK["cmake"][system]
    cmake_archive = root / "downloads" / cmake_row["name"]
    fetch(cmake_archive, cmake_row, download, core)
    cmake_entries = extract(cmake_archive, root / "cmake", cmake_row, core)
    cmake_cli_closure(cmake_entries, cmake_row)
    cmake = core.child(root / "cmake", cmake_row["program"])
    signature = None
    if system == "Darwin":
        signature_log = core.probe_log(root, "cmake-signature")
        signature = core.run_owned(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(root / "cmake/CMake.app")], root, signature_log, 30)
        core.write_json(signature_log.parent / "result.json", signature)
        if not core.successful(signature):
            raise ValueError("Pinned CMake vendor signature failed; original bundle/logs retained")
    source_cmake = None
    if getattr(options, "build_cmake_source", False):
        source_cmake = core.load_script("provision-static-cmake-source.py").build(root, target, download, core, archive_api())
        cmake = Path(source_cmake["program"])
    cmake_record = core.probe_version(cmake, LOCK["cmake_version"], root, "cmake")
    ninja_archive = root / "downloads/ninja-source.tar.gz"
    fetch(ninja_archive, LOCK["ninja"], download, core, ninja=True)
    ninja_entries = extract(ninja_archive, root / "ninja-source", LOCK["ninja"], core)
    configure, build, windows_tools = ninja_commands(root, target, cmake, options, core)
    attempt = {"schema": 1, "target": target, "result": "running", "tools_lock_sha256": core.digest(LOCK_PATH),
               "configure_argv": configure, "build_argv": build, "commands": [], "cmake": cmake_record,
               "cmake_entries": cmake_entries, "ninja_source_entries": ninja_entries,
               "python": core.tool_record(Path(sys.executable).resolve()), "windows_tools": windows_tools}
    if source_cmake is not None:
        attempt["cmake_source"] = source_cmake
    if signature is not None:
        attempt["cmake_signature"] = signature
    attempt["cmake_vendor_bundle_seal_preserved"] = not any(entry["kind"] == "omitted_link" for entry in cmake_entries)
    core.write_json(root / "attempt.json", attempt)
    environment = dict(os.environ)
    for key in list(environment):
        if key.startswith("CMAKE_") or key in {"CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "PYTHONPATH", "PYTHONHOME"}:
            environment.pop(key)
    environment["PYTHONNOUSERSITE"] = "1"
    environment["PATH"] = str(Path(sys.executable).resolve().parent) + os.pathsep + environment.get("PATH", "")
    if system == "Darwin":
        environment["MACOSX_DEPLOYMENT_TARGET"] = "13.3"
    started = time.monotonic()
    try:
        for stage, argv in (("ninja-configure", configure), ("ninja-build", build)):
            actual = core.windows_command(argv, windows_tools, root, stage) if windows_tools else argv
            result = core.run_owned(actual, root, root / "logs" / (stage + ".log"), 600, environment)
            result["stage"] = stage
            attempt["commands"].append(result)
            core.write_json(root / "attempt.json", attempt, replace=True)
            if not core.successful(result):
                raise ValueError("Pinned Ninja source build failed; owned logs/inputs retained")
        core.verify_cmake_cache(root / "ninja-build/CMakeCache.txt", configure)
        verify_tree(root / "ninja-source", ninja_entries, core)
        binary = root / "ninja-build" / ("ninja.exe" if system == "Windows" else "ninja")
        attempt["ninja"] = core.probe_version(binary, LOCK["ninja_version"], root, "ninja")
        attempt["result"] = "pinned_build_tools_prepared"
        attempt["elapsed_seconds"] = round(time.monotonic() - started, 3)
        core.write_json(root / "attempt.json", attempt, replace=True)
        core.write_json(root / "tool-receipt.json", attempt)
    except Exception as error:
        attempt.update(result="failed", error=str(error), elapsed_seconds=round(time.monotonic() - started, 3))
        core.write_json(root / "attempt.json", attempt, replace=True)
        raise
    return verify_tools(destination, target, core)


def verify_tools(destination, target, core=None):
    core = core or core_module()
    validate_lock(core)
    root = setup_owner(destination, target, core)
    receipt = core.read_json(root / "tool-receipt.json")
    if receipt.get("schema") != 1 or receipt.get("target") != target or receipt.get("result") != "pinned_build_tools_prepared" or receipt.get("tools_lock_sha256") != core.digest(LOCK_PATH):
        raise ValueError("Pinned build-tools receipt identity/result differs")
    cmake_row = LOCK["cmake"][core.TARGETS[target][0]]
    cmake_entries = archive_entries(root / "downloads" / cmake_row["name"], cmake_row, core)
    ninja_entries = archive_entries(root / "downloads/ninja-source.tar.gz", LOCK["ninja"], core)
    if receipt["cmake_entries"] != cmake_entries or receipt["ninja_source_entries"] != ninja_entries:
        raise ValueError("Pinned build-tool source inventory changed")
    cmake_cli_closure(cmake_entries, cmake_row)
    if core.TARGETS[target][0] == "Darwin":
        signature = receipt.get("cmake_signature")
        if (not isinstance(signature, dict) or not core.successful(signature)
                or signature.get("argv") != ["/usr/bin/codesign", "--verify", "--deep", "--strict", str(root / "cmake/CMake.app")]):
            raise ValueError("Pinned CMake signature proof is missing or differs")
    if receipt.get("cmake_vendor_bundle_seal_preserved") != (not any(entry["kind"] == "omitted_link" for entry in cmake_entries)):
        raise ValueError("Pinned CMake bundle omission disclosure changed")
    verify_tree(root / "cmake", cmake_entries, core)
    verify_tree(root / "ninja-source", ninja_entries, core)
    bound_tool_paths(root, target, receipt, core)
    for key in ("cmake", "ninja"):
        core.verify_file(receipt[key]["path"], receipt[key])
        if receipt[key]["version"] != LOCK[key + "_version"]:
            raise ValueError("Pinned build-tool version differs")
    if [command["stage"] for command in receipt["commands"]] != ["ninja-configure", "ninja-build"] or not all(core.successful(command) for command in receipt["commands"]):
        raise ValueError("Pinned Ninja build did not pass/settle")
    verify_ninja_policy(root, target, receipt, core)
    core.verify_file(receipt["python"]["path"], receipt["python"], system_tool=True)
    core.verify_cmake_cache(root / "ninja-build/CMakeCache.txt", receipt["configure_argv"])
    return {"receipt": receipt, "receipt_path": str(root / "tool-receipt.json"),
            "cmake": receipt["cmake"]["path"], "ninja": receipt["ninja"]["path"]}


def bound_tool_paths(root, target, receipt, core):
    row = LOCK["cmake"][core.TARGETS[target][0]]
    cmake = core.child(root / "cmake", row["program"])
    if "cmake_source" in receipt:
        source_cmake = core.load_script("provision-static-cmake-source.py").verify(root, target, core, archive_api())
        if source_cmake != receipt["cmake_source"]:
            raise ValueError("CMake source provenance changed")
        cmake = Path(source_cmake["program"])
    expected = {"cmake": cmake,
                "ninja": core.child(root, "ninja-build/" + ("ninja.exe" if core.TARGETS[target][0] == "Windows" else "ninja"))}
    for name, path in expected.items():
        if Path(os.path.abspath(receipt[name]["path"])) != path:
            raise ValueError("Prepared build tool redirected outside its exact owned path: " + name)
    return expected


def verify_ninja_policy(root, target, receipt, core):
    configure = receipt["configure_argv"]
    system, arch = core.TARGETS[target]
    expected_prefix = [receipt["cmake"]["path"], "-S", str(root / "ninja-source"), "-B", str(root / "ninja-build"), "-G", "NMake Makefiles" if system == "Windows" else "Unix Makefiles"]
    if configure[:7] != expected_prefix:
        raise ValueError("Pinned Ninja source/build/generator identity changed")
    definitions = {}
    for flag in configure[7:]:
        if not flag.startswith("-D") or "=" not in flag:
            raise ValueError("Unsupported Ninja configure argument")
        key, value = flag[2:].split("=", 1)
        if key in definitions:
            raise ValueError("Duplicate Ninja configure definition")
        definitions[key] = value
    expected = {"CMAKE_BUILD_TYPE": "Release", "BUILD_TESTING": "OFF", "NINJA_BUILD_BINARY": "ON", "RE2C": "OFF", "NINJA_PYTHON": receipt["python"]["path"]}
    for key, value in expected.items():
        if definitions.get(key) != value:
            raise ValueError("Pinned Ninja build policy differs: " + key)
    platform_keys = {"CMAKE_MAKE_PROGRAM"}
    if system == "Windows":
        if receipt["windows_tools"]["windows_sdk"] != "10.0.26100.0" or receipt["windows_tools"]["msvc_toolset"] != "14.44.35207":
            raise ValueError("Pinned Ninja MSVC toolchain changed")
        make_path = definitions.get("CMAKE_MAKE_PROGRAM", "").replace("\\", "/")
        if not make_path.endswith("/MSVC/14.44.35207/bin/Hostx64/x64/nmake.exe"):
            raise ValueError("Pinned Ninja native build tool differs")
    else:
        platform_keys.add("CMAKE_CXX_COMPILER")
        if arch == "x86_64":
            platform_keys.add("CMAKE_CXX_FLAGS")
            if definitions.get("CMAKE_CXX_FLAGS") != "-march=x86-64 -mtune=generic":
                raise ValueError("Pinned Ninja CPU baseline changed")
        if system == "Darwin":
            platform_keys.update({"CMAKE_OSX_ARCHITECTURES", "CMAKE_OSX_DEPLOYMENT_TARGET"})
            if definitions.get("CMAKE_OSX_ARCHITECTURES") != ("arm64" if arch == "aarch64" else "x86_64") or definitions.get("CMAKE_OSX_DEPLOYMENT_TARGET") != "13.3":
                raise ValueError("Pinned Ninja Mac architecture/minimum changed")
    if set(definitions) != set(expected) | platform_keys:
        raise ValueError("Pinned Ninja configure input catalog changed")
    expected_build = [receipt["cmake"]["path"], "--build", str(root / "ninja-build"), "--target", "ninja", "--config", "Release", "--parallel", "2"]
    if receipt["build_argv"] != expected_build:
        raise ValueError("Pinned Ninja actual build command changed")


def main():
    core = core_module()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--target", choices=core.TARGETS, default=core.host_target())
    parser.add_argument("--download", action="store_true")
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--build-cmake-source", action="store_true", help="Use the separately pinned Mac arm64 source-tool provenance")
    parser.add_argument("--vs-root", type=Path)
    parser.add_argument("--cxx", type=Path)
    parser.add_argument("--msvc-toolset", default="14.44.35207", choices=["14.44.35207"])
    parser.add_argument("--windows-sdk", default="10.0.26100.0", choices=["10.0.26100.0"])
    args = parser.parse_args()
    if args.verify and (args.download or args.build_cmake_source):
        parser.error("--verify cannot download or select a source build")
    core.owned_destination(args.destination, args.target, create=not args.verify)
    result = verify_tools(args.destination, args.target, core) if args.verify else provision_tools(args.destination, args.target, args.download, args, core)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
