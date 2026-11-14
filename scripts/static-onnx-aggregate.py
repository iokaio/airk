# SPDX-License-Identifier: Apache-2.0
"""Additive genuine archive packaging for rc10's monolithic static layout.

The original CMake build and receipts remain unchanged. This creates a real
archive from the source build's runtime target dependency closure, not empty
libraries pretending obsolete Abseil components still exist.
"""
import copy
import os
from pathlib import Path
import re
import time

RECEIPT = "static-aggregate-receipt.json"
ATTEMPT = "static-aggregate-attempt.json"
MAX_ARCHIVE = 2 * 1024 * 1024 * 1024


def runtime_closure(text, components, windows, relative_name):
    if len(text.encode()) > 64 * 1024 * 1024:
        raise ValueError("Ninja static target graph exceeds bound")
    suffix = ".lib" if windows else ".a"
    prefix = "" if windows else "lib"
    rules = {}
    for line in text.replace("$\n", "").splitlines():
        match = re.fullmatch(r"build ([^:]+): (?:CXX|C)_STATIC_LIBRARY_LINKER[^ ]* (.*)", line)
        if not match:
            continue
        name = match[1].replace("\\", "/")
        # Generated library target paths are relative, single safe tokens. No
        # interpretation of arbitrary Ninja commands or object-file paths.
        if any(character.isspace() for character in name) or "$" in name:
            raise ValueError("Unsupported Ninja archive target spelling")
        relative_name(name)
        if not name.endswith(suffix) or name in rules:
            raise ValueError("Duplicate/foreign Ninja archive target")
        dependencies = match[2].split("||", 1)[1].split() if "||" in match[2] else []
        selected = []
        for dependency in dependencies:
            if not dependency.endswith(suffix):
                continue
            dependency = dependency.replace("\\", "/")
            relative_name(dependency)
            if "$" in dependency:
                raise ValueError("Unsupported Ninja archive dependency")
            selected.append(dependency)
        rules[name] = selected
    roots = [prefix + "onnxruntime_" + component + suffix for component in components]
    # These already selected pinned CPU dependencies remain explicit runtime
    # roots, even when upstream's target graph treats their use as optional.
    roots += ["_deps/re2-build/" + prefix + "re2" + suffix,
              "_deps/pytorch_cpuinfo-build/" + prefix + "cpuinfo" + suffix]
    selected, visiting = set(), set()

    def visit(name):
        if name in visiting:
            raise ValueError("Cyclic static target dependency")
        if name in selected:
            return
        if name not in rules:
            raise ValueError("Missing genuine runtime static target: " + name)
        visiting.add(name)
        for dependency in rules[name]:
            visit(dependency)
        visiting.remove(name)
        selected.add(name)

    for name in roots:
        visit(name)
    if not 10 <= len(selected) <= 256:
        raise ValueError("Runtime static closure count exceeds bound")
    return sorted(selected)


def plan(destination, target, result, core):
    windows = core.TARGETS[target][0] == "Windows"
    build = destination / "b/Release"
    ninja = core.plain(build / "build.ninja", regular=True)
    names = runtime_closure(ninja.read_text(encoding="utf-8"), core.STATIC_COMPONENTS, windows, core.relative_name)
    rows = []
    for name in names:
        path = core.plain(core.child(build, name), regular=True)
        size = path.stat().st_size
        if not 64 < size <= 1024 * 1024 * 1024:
            raise ValueError("Runtime static input archive size differs")
        with path.open("rb") as source:
            if source.read(8) != b"!<arch>\n":
                raise ValueError("Runtime static input is not an archive")
        rows.append({"path": path.relative_to(destination).as_posix(), "bytes": size, "sha256": core.digest(path)})
    if sum(row["bytes"] for row in rows) > MAX_ARCHIVE:
        raise ValueError("Aggregate static inputs exceed 2 GiB bound")
    return {"schema": 1, "target": target, "ort_version": core.VERSION,
            "original_build_receipt_sha256": core.digest(destination / "build-receipt.json"),
            "ninja_sha256": core.digest(ninja), "inputs": rows,
            "consumer": "ort-sys-2.0.0-rc.10", "layout": "genuine-monolithic-static",
            "qualification": {"final_symbol_link": False, "actual_model": False, "cross_platform": False}}


def output_row(destination, target, core):
    name = "onnxruntime.lib" if core.TARGETS[target][0] == "Windows" else "libonnxruntime.a"
    path = core.plain(destination / "aggregate" / name, regular=True)
    size = path.stat().st_size
    if not 65536 < size <= MAX_ARCHIVE:
        raise ValueError("Genuine aggregate archive size differs")
    with path.open("rb") as archive:
        if archive.read(8) != b"!<arch>\n":
            raise ValueError("Aggregate output is not a genuine static archive")
    return {"path": path.relative_to(destination).as_posix(), "bytes": size, "sha256": core.digest(path)}


def invocation(destination, target, result, selected, core):
    system = core.TARGETS[target][0]
    paths = [str(core.child(destination, row["path"])) for row in selected["inputs"]]
    if any(any(character in path for character in '\r\n"') for path in paths):
        raise ValueError("Unsafe archive input path")
    if system == "Windows":
        tool = Path(result["receipt"]["toolchain"]["compiler"]["path"]).with_name("lib.exe")
        response = destination / "aggregate/inputs.rsp"
        text = "\n".join('"' + path + '"' for path in paths) + "\n"
        argv = [str(tool), "/NOLOGO", "/OUT:" + str(destination / "aggregate/onnxruntime.lib"), "@" + str(response)]
    elif system == "Darwin":
        tool = Path("/usr/bin/libtool")
        response = destination / "aggregate/inputs.filelist"
        text = "\n".join(paths) + "\n"
        argv = [str(tool), "-static", "-o", str(destination / "aggregate/libonnxruntime.a"), "-filelist", str(response)]
    elif system == "Linux":
        tool = Path("/usr/bin/ar")
        response = destination / "aggregate/inputs.mri"
        output = str(destination / "aggregate/libonnxruntime.a")
        if any(character.isspace() for path in [output, *paths] for character in path):
            raise ValueError("GNU MRI archive paths must not contain whitespace")
        text = "CREATE " + output + "\n" + "".join("ADDLIB " + path + "\n" for path in paths) + "SAVE\nEND\n"
        # Fixed shell grammar; both paths are literal positional arguments.
        # The owned runner retains the complete process group through ar exit.
        argv = ["/bin/sh", "-c", 'exec "$1" -M < "$2"', "airk-static-aggregate", str(tool), str(response)]
    else:
        raise ValueError("Unsupported native archive packaging host")
    return {"tool": tool, "response": response, "text": text, "argv": argv}


def verify(destination, target, result, core):
    receipt = core.read_json(destination / RECEIPT)
    expected = plan(destination, target, result, core)
    if (receipt.get("plan") != expected or receipt.get("result") != "genuine_aggregate_not_yet_final_link_or_model_qualified"
            or not core.successful(receipt.get("command", {}))):
        raise ValueError("Static aggregate provenance differs")
    core.verify_file(receipt["tool"]["path"], receipt["tool"], system_tool=True)
    request = invocation(destination, target, result, expected, core)
    expected_native = request["argv"]
    if target.endswith("windows-msvc"):
        expected_native = [str(Path(os.environ.get("SystemRoot", "C:/Windows")) / "System32/cmd.exe"), "/d", "/c", str(destination / "commands/aggregate.cmd")]
    if (receipt["tool"]["path"] != str(request["tool"]) or receipt.get("argv") != request["argv"]
            or receipt["command"].get("argv") != expected_native
            or core.plain(request["response"], regular=True).read_bytes() != request["text"].encode()
            or receipt.get("response_sha256") != core.digest(request["response"])):
        raise ValueError("Static aggregate command or exact input response file changed")
    if receipt.get("output") != output_row(destination, target, core):
        raise ValueError("Static aggregate bytes changed")
    value = copy.deepcopy(result)
    value["static_aggregate"] = {"path": RECEIPT, "sha256": core.digest(destination / RECEIPT), "receipt": receipt}
    value["environment"]["ORT_LIB_LOCATION"] = str(destination / "aggregate")
    value["environment"]["ORT_LIB_PROFILE"] = "Release"
    return value


def aggregate(destination, target, core):
    destination = core.owned_destination(destination, target)
    original = core.verify_original_build(destination, target)
    if (destination / RECEIPT).exists():
        return verify(destination, target, original, core)
    if (destination / ATTEMPT).exists() or (destination / "aggregate").exists():
        raise ValueError("Existing incomplete aggregate retained; no automatic retry/overwrite")
    selected = plan(destination, target, original, core)
    request = invocation(destination, target, original, selected, core)
    library_tool = core.plain(request["tool"], regular=True)
    tool = {"path": str(library_tool), "bytes": library_tool.stat().st_size, "sha256": core.digest(library_tool)}
    output = destination / "aggregate"
    output.mkdir()
    response = request["response"]
    with response.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(request["text"])
    argv = request["argv"]
    receipt = {"schema": 1, "result": "running", "plan": selected, "tool": tool,
               "argv": argv, "response_sha256": core.digest(response)}
    core.write_json(destination / ATTEMPT, receipt)
    started = time.monotonic()
    try:
        actual = core.windows_command(argv, original["receipt"]["toolchain"], destination, "aggregate") if core.TARGETS[target][0] == "Windows" else argv
        environment = {key:value for key,value in os.environ.items() if key not in {"LD_PRELOAD","LD_LIBRARY_PATH","DYLD_LIBRARY_PATH","DYLD_INSERT_LIBRARIES"}}
        receipt["command"] = core.run_owned(actual, destination, destination / "logs/aggregate.log", 300, environment)
        if not core.successful(receipt["command"]):
            raise ValueError("Static aggregate command failed or native descendants are unsettled")
        if selected != plan(destination, target, original, core):
            raise ValueError("Original runtime archive closure changed during aggregation")
        receipt["output"] = output_row(destination, target, core)
        receipt["result"] = "genuine_aggregate_not_yet_final_link_or_model_qualified"
        receipt["elapsed_seconds"] = round(time.monotonic() - started, 3)
        core.write_json(destination / ATTEMPT, receipt, replace=True)
        core.write_json(destination / RECEIPT, receipt)
    except Exception as error:
        receipt.update(result="failed", error=str(error), elapsed_seconds=round(time.monotonic() - started, 3))
        core.write_json(destination / ATTEMPT, receipt, replace=True)
        raise
    return verify(destination, target, original, core)
