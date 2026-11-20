# SPDX-License-Identifier: Apache-2.0
"""Verify a same-build Linux static ORT aggregate and consume Rust's native product projection."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import time

TARGET = "x86_64-unknown-linux-gnu"
SYMBOLS = {"OrtGetApiBase", "airk_ort_bootstrap_linux_cuda_v1", "airk_ort_collect_final_placement_v1",
           "airk_ort_collect_final_graph_metadata_v1"}


def check(condition, reason):
    if not condition:
        raise ValueError(reason)


def valid_hash(text):
    return isinstance(text, str) and re.fullmatch(r"[0-9a-f]{64}", text) and text != "0" * 64


def relative(name):
    check(isinstance(name, str) and name and len(name) <= 1024 and "\\" not in name,
          "invalid Linux artifact name")
    value = PurePosixPath(name)
    check(not value.is_absolute() and value.as_posix() == name and ".." not in value.parts and "." != name,
          "Linux artifact must be a bounded relative path")
    return value


def read_file(path, expected, limit, deadline, *, metadata=False, output=None, expected_bytes=None):
    path = Path(path)
    check(valid_hash(expected) and path.is_absolute() and path.resolve(strict=True) == path,
          "invalid digest or noncanonical Linux artifact path")
    with path.open("rb") as stream:
        before = os.fstat(stream.fileno())
        check(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid() and before.st_nlink == 1
              and 0 < before.st_size <= limit and (expected_bytes is None or before.st_size == expected_bytes),
              "Linux artifact ownership, type or size differs: " + str(path))
        digest = hashlib.sha256()
        data = bytearray() if metadata else None
        while True:
            check(time.monotonic() < deadline, "Linux package verification deadline expired")
            block = stream.read(2 << 20)
            if not block:
                break
            digest.update(block)
            if output is not None:
                output.write(block)
            if data is not None:
                data.extend(block)
        after = path.stat()
        stable = lambda st: (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_ctime_ns)
        check(stable(before) == stable(os.fstat(stream.fileno())) == stable(after)
              and digest.hexdigest() == expected, "Linux artifact changed: " + str(path))
    return json.loads(data) if metadata else before.st_size


def complete(command):
    return (command["exit_code"] == 0 and command["timed_out"] is False and command["log_oversized"] is False
            and command["owned_descendants_settled"] is True and not command["cleanup_errors"])


def verify_handoff(path, expected_sha256, repo_root, *, deadline=None):
    del repo_root
    check(os.getuid() != 0 and os.getuid() == os.geteuid(), "non-root build owner required")
    deadline = time.monotonic() + 600 if deadline is None else deadline
    core = read_file(Path(path).absolute(), expected_sha256, 4 << 20, deadline, metadata=True)
    check(core["success"] is True and core["target"] == TARGET and set(core["symbols"]) == SYMBOLS
          and len(core["symbols"]) == len(SYMBOLS)
          and all(complete(core[name]) for name in ("command", "aggregate_command", "symbol_command")),
          "genuine settled GNU static aggregate required")
    root = Path(core["build_root"])
    check(root.is_absolute() and root.resolve(strict=True) == root and root.stat().st_uid == os.getuid(),
          "invalid native build root")
    for name, receipt_key in (("configure", "configure_input"), ("core_receipt", "component_build_input")):
        pin = core[receipt_key]
        check(pin["sha256"] == core[name], "static producer input pin differs")
        source = read_file(Path(pin["path"]), pin["sha256"], 8 << 20, deadline, metadata=True)
        if name == "configure":
            check(source["configured"] is True and source["root"] == str(root)
                  and source["ninja_sha256"] == core["graph"], "Linux configure lineage differs")
        else:
            check(source["configure"] == core["configure"] and source["built"] is True
                  and source["rules_unchanged"] is True and source["before"] == source["after"]
                  and source["authenticated_inputs_before"] == source["authenticated_inputs_after"]
                  and source["command"] == core["command"], "static component compilation lineage differs")
    read_file(root / "b/Release/build.ninja", core["graph"], 64 << 20, deadline)
    rows = core["inputs"]
    check(10 <= len(rows) <= 256 and len({r["path"] for r in rows}) == len(rows)
          and sum(r["bytes"] for r in rows) <= 2 << 30, "invalid GNU static component inventory")
    for row in rows:
        name = relative(row["path"])
        check(name.parts[:2] == ("b", "Release") and name.suffix == ".a", "unexpected static component")
        read_file(root / name, row["sha256"], 2 << 30, deadline, expected_bytes=row["bytes"])
    image = core["static"]
    check(image["path"] == "aggregate/libonnxruntime.a", "unexpected static ORT aggregate name")
    read_file(root / image["path"], image["sha256"], 2 << 30, deadline, expected_bytes=image["bytes"])
    closure = hashlib.sha256(json.dumps({"components":rows,"archive":image},sort_keys=True,
                                        separators=(",",":")).encode()).hexdigest()
    return {"static_ort":{"build_receipt_sha256":expected_sha256,"target":TARGET,"static_link_closure_sha256":closure,
                          "static_link_supplement_sha256":None,"static_link_recovery_sha256":None,
                          "verified_inputs_sha256":expected_sha256},
            "aggregate_sha256":image["sha256"], "activation_pins_available":False,
            "environment":{"LIBONNXRUNTIME_NO_PKG_CONFIG":"1","ORT_SKIP_DOWNLOAD":"1","ORT_PREFER_DYNAMIC_LINK":"0",
                           "ORT_LIB_PROFILE":"Release","ORT_LIB_LOCATION":str(root / "aggregate")}}


def execution_owner():
    path = Path(__file__).parent / "arbiter/evaluation/linux_cuda/product_projection.py"
    spec = importlib.util.spec_from_file_location("airk_linux_cuda_projection_owner", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def require_native_activation_projection(projection_path, projection_sha256, verified_handoff, *,
                                         native_execution_receipt, repo_root, source_root=None):
    del repo_root
    receipt_path, receipt_sha = native_execution_receipt
    result = execution_owner().verify_execution(receipt_path, receipt_sha, projection_path, projection_sha256)
    root = Path(result["runtime_root"])
    check(source_root is None or root == Path(source_root), "Linux package source root differs")
    deadline = time.monotonic() + 120
    native = read_file(Path(projection_path).absolute(), projection_sha256, 1 << 20, deadline, metadata=True)
    check(native["schema"] == "airk.native.cuda-package-projection/1" and native["activation"] is False
          and native["qualification"] is False and native["static_core_build"] == verified_handoff["static_ort"]["build_receipt_sha256"]
          and native["descriptor"] == result["descriptor"] and native["descriptor"]["name"] == "airk-cuda-runtime.json",
          "Linux native projection differs from its held product observation or static producer")
    rows = native["files"]
    check(4 <= len(rows) <= 128 and sum(r["bytes"] for r in rows) <= 4 << 30, "Linux product inventory cap")
    files = {}
    for row in rows:
        name = str(relative(row["name"]))
        check("/" not in name and name not in files, "invalid native Linux product basename")
        read_file(root / name, row["sha256"], 2 << 30, deadline, expected_bytes=row["bytes"])
        files[name] = row["sha256"]
    check(files == result["files"] and files.get("airk-cuda-runtime.json") == native["descriptor"]["sha256"]
          and result["execution"]["sha256"] == receipt_sha and result["projection"]["sha256"] == projection_sha256,
          "native product custody differs from original complete projection")
    return {"files":files,"source_root":root,"native_projection":result["projection"],
            "native_execution_receipt":result["execution"],"static_core_build":native["static_core_build"],
            "descriptor":native["descriptor"],"activation":False,"qualification":False}


def reverify_native_projection(value, verified_handoff):
    projection = value["native_projection"]
    execution = value["native_execution_receipt"]
    return require_native_activation_projection(projection["path"],projection["sha256"],verified_handoff,
                                                native_execution_receipt=(execution["path"],execution["sha256"]),
                                                repo_root=None,source_root=value["source_root"])


def stage_native_projection(value, destination, verified_handoff):
    observed = reverify_native_projection(value, verified_handoff)
    check(observed == value, "Linux product custody changed before staging")
    root = value["source_root"]
    dest = Path(destination)
    check(dest.resolve(strict=True) == dest and dest.is_dir() and dest.stat().st_uid == os.getuid(),
          "invalid Linux product destination")
    deadline = time.monotonic()+120
    for name, expected in value["files"].items():
        with (dest / name).open("xb") as output:
            read_file(root / name, expected, 2 << 30, deadline, output=output)
        read_file(dest / name, expected, 2 << 30, deadline)
    return value["files"]


def verify_linked_image(path):
    result = subprocess.run(["/usr/bin/readelf","--wide","--dyn-syms",str(path)],check=True,
                            capture_output=True,text=True,timeout=30)
    found = []
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) == 8 and fields[-1] in SYMBOLS:
            check(fields[3:6] == ["FUNC","GLOBAL","DEFAULT"] and fields[6] != "UND",
                  "Linux service has an undefined or hidden CUDA runtime entry point")
            found.append(fields[-1])
    check(set(found) == SYMBOLS and len(found) == len(SYMBOLS),
          "Linux service did not retain the four actual same-image CUDA entry points")
