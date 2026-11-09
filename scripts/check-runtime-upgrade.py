# SPDX-License-Identifier: Apache-2.0
"""Upgrade a real older runtime with a connected launcher/MCP client in isolated state."""
import argparse
from contextlib import closing
import hashlib
import importlib.util
import json
import ntpath
import os
from pathlib import Path
import platform
import queue
import re
import sqlite3
import stat
import subprocess
import sys
import tempfile
import threading
import time
import uuid

# The Darwin-only exit contract is also tested on Windows, whose signal module
# does not expose SIGKILL. This is the native Darwin signal number, not an exit
# cause inferred from an arbitrary negative process status.
DARWIN_SIGKILL = 9


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def runtime_binary(runtime, platform=sys.platform):
    name = "AIRK.app/Contents/MacOS/airk" if platform == "darwin" else "airk.exe" if platform == "win32" else "airk"
    path = runtime / name
    if not path.is_file() or path.is_symlink() or getattr(path.lstat(),"st_file_attributes",0)&0x400:
        raise RuntimeError("Expected the platform's actual sealed runtime main")
    return path


def clean_break_required(predecessor, candidate):
    def version(value):
        if not isinstance(value, str) or not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", value):
            raise RuntimeError("Upgrade fixture requires numeric verified runtime versions")
        return tuple(map(int, value.split(".")))
    return version(predecessor) < (1, 8, 16) <= version(candidate)


def validate_clean_break_rebuild(diagnosis, originals, old_namespace, old_jobs, candidate, predecessor):
    """Reject a partial or reused index roster before qualifying any reset."""
    rebuild = diagnosis.get("rebuild")
    if diagnosis.get("rebuild_error") is not None:
        raise RuntimeError("Native clean-break observation failed: " + str(diagnosis["rebuild_error"]))
    if isinstance(rebuild, dict):
        if rebuild.get("failure") is not None or rebuild.get("stage") in ("failed", "unavailable"):
            raise RuntimeError("Native clean-break operation failed or became unavailable: " + json.dumps(rebuild.get("failure")))
        cleanup = rebuild.get("cleanup")
        if isinstance(cleanup, dict) and (cleanup.get("state") == "blocked" or cleanup.get("failure") is not None):
            raise RuntimeError("Native clean-break cleanup blocked: " + json.dumps(cleanup.get("failure")))
    if (diagnosis.get("rebuild_error") is not None or not isinstance(rebuild, dict)
            or rebuild.get("schema") != 1 or rebuild.get("mode") != "clean_break"
            or rebuild.get("provenance") != "native_rebuild_from_saved_configuration"
            or rebuild.get("candidate") != candidate or rebuild.get("predecessor") != predecessor
            or rebuild.get("original_namespace") != old_namespace
            or rebuild.get("index_scope") is not None
            or rebuild.get("failure") is not None
            or rebuild.get("stage") not in ("accepted", "preparing", "indexing", "ready")
            or not isinstance(rebuild.get("cleanup"), dict)
            or type(rebuild["cleanup"].get("complete")) is not bool
            or rebuild["cleanup"].get("state") not in ("waiting_for_indexes", "removing", "complete")
            or rebuild["cleanup"].get("failure") is not None
            or rebuild["cleanup"]["complete"] != (rebuild["cleanup"]["state"] == "complete")
            or rebuild["cleanup"]["complete"] and rebuild["stage"] != "ready"):
        raise RuntimeError("Clean-break diagnostic lacks complete native reset authority or records failure")
    rows = rebuild.get("indexes")
    expected = {item["registration"]["binding"]: item for item in originals}
    if (len(expected) != len(originals) or type(rebuild.get("indexes_complete")) is not bool
            or type(rebuild.get("expected_indexes")) is not int or rebuild["expected_indexes"] != len(expected)):
        raise RuntimeError("Clean-break observation omitted its complete saved-index count qualification")
    if not rebuild["indexes_complete"]:
        if (rows != [] or rebuild["stage"] not in ("accepted", "preparing") or rebuild["cleanup"]["complete"]):
            raise RuntimeError("Clean-break diagnostic attempted to qualify a partial saved-index roster")
        return None
    if (not isinstance(rows, list) or len(rows) != len(expected)
            or {row.get("original", {}).get("binding") for row in rows if isinstance(row, dict)} != set(expected)):
        raise RuntimeError("Clean-break diagnostic omitted, duplicated or added a saved index")
    new_bindings, new_jobs = set(), set()
    for row in rows:
        previous = row.get("original", {})
        fixture = expected[previous.get("binding")]
        if (previous != fixture["registration"] or row.get("sources") != fixture.get("sources", {})
                or row.get("original_paused") is not fixture["paused"]
                or type(row.get("watching")) is not bool or type(row.get("paused")) is not bool or row.get("failure") is not None
                or row.get("readiness") not in ("pending", "paused", "ready")):
            raise RuntimeError("Clean-break diagnostic changed original source selection or supported settings")
        replacement = row.get("replacement")
        job = row.get("job")
        if replacement is not None:
            if (replacement.get("root") != previous["root"] or replacement.get("collection") != previous["collection"]
                    or not isinstance(replacement.get("data_root"), str)
                    or location_key(replacement["data_root"]) == location_key(previous["data_root"])
                    or not re.fullmatch(r"[0-9a-f]{64}", replacement.get("binding", ""))
                    or replacement["binding"] in expected or replacement["binding"] in new_bindings):
                raise RuntimeError("Clean-break replacement reused an old index or changed source scope")
            new_bindings.add(replacement["binding"])
        if job is not None:
            if (replacement is None or job.get("project") != replacement["binding"] or job.get("kind") != "index"
                    or not re.fullmatch(r"[0-9a-f]{64}", job.get("id", ""))
                    or job["id"] in old_jobs or job["id"] in new_jobs):
                raise RuntimeError("Clean-break index job reused accepted predecessor work")
            new_jobs.add(job["id"])
            if job.get("state") in ("failed", "canceled"):
                raise RuntimeError("Native clean-break index job failed: " + json.dumps(job.get("outcome")))
            if job.get("state") not in ("pending", "running", "succeeded"):
                raise RuntimeError("Native clean-break index job has an unknown state")
            if job["state"] == "succeeded":
                outcome = job.get("outcome") or {}
                revision = outcome.get("revision")
                if (outcome.get("state") != "succeeded" or not isinstance(revision, str) or len(revision) > 126
                        or not re.fullmatch(r"airk-r1:[0-9a-f]{32}:(?:0|[1-9][0-9]*):[0-9a-f]{64}", revision)
                        or int(revision.split(":")[2]) > (1 << 64)-1
                        or revision == fixture["indexed_job"].get("outcome", {}).get("revision")):
                    raise RuntimeError("Clean-break index job has no fresh canonical publication")
            elif job.get("outcome") is not None:
                raise RuntimeError("Nonterminal clean-break index job incorrectly carries an outcome")
        if rebuild["cleanup"]["complete"]:
            if (row.get("readiness") != "ready" or row.get("paused") is not fixture["paused"] or row.get("watching") is not fixture["watch"]
                    or job is None or job.get("state") != "succeeded"
                    or job.get("outcome", {}).get("state") != "succeeded"
                    or job["outcome"].get("revision") is None
                    or job["outcome"]["revision"] == fixture["indexed_job"].get("outcome", {}).get("revision")):
                raise RuntimeError("Clean-break index has no fresh successful publication and restored pause preference")
    namespace = rebuild.get("successor_namespace")
    if namespace is not None and (not re.fullmatch(r"[0-9a-f]{32}", namespace) or namespace == old_namespace):
        raise RuntimeError("Clean-break successor reused the predecessor namespace")
    if rebuild["cleanup"]["complete"] and (namespace is None or len(new_bindings) != len(expected) or len(new_jobs) != len(expected)):
        raise RuntimeError("Clean-break ready diagnostic has an incomplete successor roster")
    # Native Index readiness precedes the background preference restoration and
    # owned cleanup turn. Its Ready label is never substituted for that receipt.
    return rebuild if rebuild["cleanup"]["complete"] else None


def location_key(value):
    value = os.fspath(value)
    if sys.platform == "win32":
        if value.startswith("\\\\?\\UNC\\"):
            value = "\\\\" + value[8:]
        elif value.startswith("\\\\?\\"):
            value = value[4:]
        return ntpath.normcase(ntpath.normpath(value))
    return os.path.normpath(value)


def validate_clean_break_baseline(marker, rebuild, control, install):
    """Match persisted authority after strict resolution of existing locations."""
    if (marker.get("schema") != 1 or marker.get("state_contract") != "airk-state/1"
            or marker.get("baseline_version") != "1.8.16" or marker.get("namespace") != rebuild["successor_namespace"]
            or location_key(Path(marker.get("control", "")).resolve(strict=True)) != location_key(control.resolve(strict=True))
            or location_key(Path(marker.get("install", "")).resolve(strict=True)) != location_key(install.resolve(strict=True))):
        raise RuntimeError("Clean-break successor lacks its exact persisted baseline contract")
    return marker


def validate_clean_break_cleanup(rebuild, originals, old_control, candidate, predecessor):
    """Consume original native retirement receipts; never repeat cleanup for evidence."""
    cleanup = rebuild.get("cleanup", {})
    observation = cleanup.get("observation")
    expected_roots = {location_key(item["registration"]["data_root"]) for item in originals}
    expected_collections = {location_key(Path(item["registration"]["data_root"]) / "collections-1.4" / item["registration"]["collection"])
        for item in originals}
    if (cleanup.get("complete") is not True or cleanup.get("state") != "complete"
            or cleanup.get("failure") is not None or not isinstance(observation, dict)
            or observation.get("complete") is not True
            or observation.get("phase") != "complete"
            or observation.get("scope") != "owned_predecessor_native_state_indexes_and_executables"
            or location_key(observation.get("old_control", "")) != location_key(old_control)
            or observation.get("collections_total") != len(originals)
            or not isinstance(observation.get("retired_collections"), list)
            or len(observation["retired_collections"]) != len(expected_collections)
            or {location_key(path) for path in observation["retired_collections"]} != expected_collections
            or not isinstance(observation.get("retired_data_roots"), list)
            or len(observation["retired_data_roots"]) != len(expected_roots)
            or {location_key(path) for path in observation["retired_data_roots"]} != expected_roots
            or not isinstance(observation.get("data_roots"), list)
            or len(observation["data_roots"]) != len(expected_roots)
            or {location_key(path) for path in observation["data_roots"]} != expected_roots):
        raise RuntimeError("Clean-break cleanup has no complete exact original storage-retirement receipt")
    launchers, runtimes = observation.get("launchers", {}), observation.get("runtimes", {})
    def processes(value, scope):
        return (isinstance(value, dict) and value.get("scope") == scope
            and type(value.get("remaining")) is int and value["remaining"] == 0
            and all(isinstance(value.get(field), list) and len(value[field]) <= 20
                for field in ("observations", "image_roster", "image_exclusions")))
    retired = runtimes.get("retired_runtimes")
    if (launchers.get("schema") != 1 or launchers.get("active") != candidate
            or launchers.get("legacy_launchers_retired") is not True
            or "launcher" not in launchers or launchers["launcher"] is not None
            or "universal_launcher" not in launchers or launchers["universal_launcher"] is not None
            or not processes(launchers.get("processes"), "verified_launcher_images")
            or runtimes.get("active") != candidate or runtimes.get("complete") is not True
            or runtimes.get("selected_service_restarted") is not False
            or not isinstance(retired, list) or not 1 <= len(retired) <= 4
            or any(not isinstance(item, str) or not re.fullmatch(r"[0-9a-f]{64}", item) for item in retired)
            or len(set(retired)) != len(retired) or predecessor not in retired or candidate in retired
            or not processes(runtimes.get("processes"), "verified_superseded_runtime_images")):
        raise RuntimeError("Clean-break cleanup omitted the original complete runtime/launcher retirement proof")
    return observation


def observe_source_inputs(source_inputs):
    """Observe every generated source even when setup failed before registration."""
    observations = []
    for item in source_inputs:
        observation = dict(item)
        try:
            path = Path(item["source_file"])
            observation["preserved"] = path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == item["source_sha256"]
        except Exception as error:
            observation.update(preserved=False, failure=repr(error))
        observations.append(observation)
    return {"count": len(observations), "preserved": all(item["preserved"] for item in observations) if observations else None,
            "observations": observations}


def catalog_page_items(rpc, kind, index_id=None):
    """Read a complete, revision-pinned bounded catalog page sequence."""
    after, revision, namespace, rows, pages = None, None, None, [], []
    for _ in range(32):
        response = rpc({"catalog_page": {"request": {"kind": kind, "index_id": index_id, "project_id": None,
            "after": after, "expected_catalog_revision": revision, "limit": 32, "max_bytes": 48000}}})
        page = response.get("page", {})
        if (response.get("status") != "catalog_page" or not isinstance(page.get("items"), list)
                or len(page["items"]) > 32 or type(page.get("revision")) is not int
                or page["revision"] < 1
                or not re.fullmatch(r"[0-9a-f]{32}", page.get("namespace", ""))
                or revision is not None and (page["revision"] != revision or page["namespace"] != namespace)):
            raise RuntimeError("Clean-break catalog page is incomplete or crossed")
        pages.append(response)
        rows.extend(page["items"])
        cursor = page.get("continuation")
        if cursor is None:
            return rows, pages
        if (not isinstance(cursor, dict) or cursor.get("kind") != kind or cursor.get("index_id") != index_id
                or cursor.get("project_id") is not None
                or cursor.get("revision") != page["revision"] or not isinstance(cursor.get("after"), str)
                or not 1 <= len(cursor["after"]) <= 512
                or after is not None and cursor["after"] <= after["after"]):
            raise RuntimeError("Clean-break catalog continuation changed its authority")
        after, revision, namespace = cursor, page["revision"], page["namespace"]
    raise RuntimeError("Clean-break catalog page budget exceeded; no partial roster accepted")


def fresh_clean_break_selection(rpc, replacement):
    """Explicitly review a NEW grant; predecessor grants are never retargeted."""
    items, index_pages = catalog_page_items(rpc, "indexes")
    matches = [row["value"] for row in items if row.get("kind") == "index"
               and row.get("value", {}).get("backing") == replacement and row["value"].get("tombstone") is False]
    if len(matches) != 1:
        raise RuntimeError("Fresh connection has no exact singular replacement Index")
    index = matches[0]["id"]
    sources, source_pages = catalog_page_items(rpc, "sources", index)
    enabled = [row["value"] for row in sources if row.get("kind") == "source" and row.get("value", {}).get("enabled") is True]
    if (len(enabled) != 1 or enabled[0].get("owner") is not True
            or enabled[0].get("canonical_path") != replacement["root"]):
        raise RuntimeError("Fresh connection source selection differs from the reviewed fixture")
    def key():
        epoch = rpc("request_epoch")
        return {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}
    context = uuid.uuid4().hex + uuid.uuid4().hex
    descriptor = rpc({"catalog_host_context": {"key": context, "expected_catalog_revision": None}})["descriptor"]
    revision, project = descriptor["catalog_revision"], "prj_"+uuid.uuid4().hex
    def apply(operation):
        nonlocal revision
        request = {"caller": uuid.uuid4().hex, "key": key(), "expected_catalog_revision": revision, "operation": operation}
        preview = rpc({"catalog_preview": {"request": request}})
        applied = rpc({"catalog_apply": {"request": request, "preview_digest": preview["preview_digest"]}})
        if applied.get("status") != "catalog_applied":
            raise RuntimeError("Fresh connection review was not applied")
        revision = applied["receipt"]["catalog_revision"]
    apply({"operation": "create_project", "id": project, "name": "Fresh clean-break Project"})
    apply({"operation": "set_membership", "membership": {"project_id": project, "index_id": index, "revision": 1,
        "scope": [{"source_root_id": enabled[0]["id"], "kind": "subtree", "path": ""}]},
        "expected_scope_revision": 1, "expected_membership_revision": None})
    rpc({"catalog_host_context": {"key": context, "expected_catalog_revision": revision}})
    grant = rpc({"catalog_grant_draft": {"context_key": context, "selection": {"key": key(),
        "expected_catalog_revision": revision, "project_ids": [project], "index_ids": [index]}}})["draft"]
    granted = rpc({"catalog_apply": {"request": grant["request"], "preview_digest": grant["preview_digest"]}})
    if granted.get("status") != "catalog_applied":
        raise RuntimeError("Fresh connection grant was not applied")
    selection = {"schema": 1, "context_key": context, "namespace": descriptor["namespace"],
        "expected_catalog_revision": granted["receipt"]["catalog_revision"], "grant_id": grant["request"]["operation"]["grant"]["id"],
        "expected_grant_revision": 1, "index_id": index, "project_ids": [project]}
    return selection, {"index_pages": index_pages, "source_pages": source_pages}


def runtime_package_tools():
    specification = importlib.util.spec_from_file_location("upgrade_package_native", Path(__file__).with_name("package-native.py"))
    package = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(package)
    return package


def legacy_windows_path_budget(fixture_root, inventories, platform_name=sys.platform):
    """Predict legacy MAX_PATH consumers before starting any native process."""
    if platform_name != "win32":
        return {"checked": False, "reason": "Windows legacy path limits do not apply"}
    owner = str(fixture_root).replace("/", "\\")
    if owner.startswith("\\\\?\\"):
        owner = owner[4:]
    install = ntpath.join(owner, "install")
    stage = ntpath.join(install, "runtimes", "stage-" + "0"*32)
    selected = ntpath.join(install, "runtimes", "0"*64)
    files = [ntpath.join(stage, ".airk-stage-parts", "0"*64),
             ntpath.join(install, "stage-"+"0"*64+".json"),
             ntpath.join(selected, "approval.json")]
    for inventory in inventories:
        source = str(inventory["root"]).replace("/", "\\")
        if source.startswith("\\\\?\\"):
            source = source[4:]
        for name in [*inventory["files"], "airk-install.json"]:
            if not isinstance(name, str) or ntpath.isabs(name) or any(part in ("", ".", "..") for part in name.replace("\\", "/").split("/")):
                raise RuntimeError("Invalid sealed runtime path in legacy path preflight")
            relative = name.replace("/", "\\")
            files.extend(ntpath.join(base, relative) for base in (source, stage, selected))
    units = lambda path: len(path.encode("utf-16-le"))//2
    directories = set()
    for path in files:
        parent = ntpath.dirname(path)
        while parent and parent != ntpath.dirname(parent):
            directories.add(parent)
            parent = ntpath.dirname(parent)
    worst_file = max(files, key=units)
    worst_directory = max(directories, key=units)
    if units(worst_file) > 259 or units(worst_directory) > 247:
        raise RuntimeError("legacy_windows_fixture_path_budget: predicted file/directory exceeds MAX_PATH before staging; "
                           f"file={units(worst_file)}/259 directory={units(worst_directory)}/247; "
                           "select a shorter existing owner-controlled --fixture-base; preserve earlier fixtures")
    return {"checked": True, "max_file_utf16_units": units(worst_file), "file_limit": 259,
            "max_directory_utf16_units": units(worst_directory), "directory_limit": 247,
            "predicted_files": len(files), "fixture_root": str(fixture_root)}


def fixture_parent(explicit=None):
    selected = explicit or os.environ.get("AIRK_PRIVATE_CENSUS_BASE")
    if selected:
        parent = Path(selected).resolve(strict=True)
    else:
        parent = Path(__file__).resolve().parents[1] / "target"
        parent.mkdir(exist_ok=True)
    if not parent.is_dir() or parent.is_symlink() or getattr(parent.lstat(), "st_file_attributes", 0) & 0x400:
        raise RuntimeError("Upgrade fixture base must be an existing regular owned directory")
    return parent


def check_fixture_path_budget(root, runtime, previous_runtime, interrupted_runtime=None):
    package = runtime_package_tools()
    inventories = []
    for supplied in (runtime, previous_runtime, interrupted_runtime):
        if supplied is not None:
            canonical = Path(supplied).resolve(strict=True)
            inventories.append({"root": canonical, "files": package.bounded_json(canonical/"airk-install.json")["files"]})
    return legacy_windows_path_budget(root, inventories)


def check_upgrade_inputs(runtime, previous_runtime, interrupted_runtime=None, clean_break=False, fixture_root=None):
    """Read-only fixture preflight using the native packager's bounded census."""
    package = runtime_package_tools()
    observations = {}
    for role, supplied in (("candidate", runtime), ("predecessor", previous_runtime), ("interrupted", interrupted_runtime)):
        if supplied is None:
            continue
        supplied = Path(supplied)
        metadata = supplied.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or supplied.is_symlink() or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise RuntimeError("Upgrade input must be an accessible regular runtime directory: " + str(supplied))
        canonical = supplied.resolve(strict=True)
        files = package.inventory(canonical)
        install = package.bounded_json(canonical / "airk-install.json")
        native = package.bounded_json(canonical / "airk-runtime.json", 4096)
        version = install.get("version")
        host_os = "windows" if sys.platform == "win32" else "macos" if sys.platform == "darwin" else "linux"
        machine = platform.machine().lower()
        host_arch = "x86_64" if machine in ("amd64", "x86_64") else "aarch64" if machine in ("arm64", "aarch64") else None
        if (install.get("schema") != 1 or not isinstance(version, str)
                or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version)
                or install.get("files") != {name: value for name, value in files.items() if name != "airk-install.json"}
                or native.get("schema") not in (1, 2)
                or host_arch is None or install.get("os") != host_os or install.get("arch") != host_arch
                or any(native.get(key) != install.get(key) for key in ("version", "os", "arch"))):
            raise RuntimeError("Upgrade input has inconsistent version or sealed file inventory: " + str(canonical))
        main = runtime_binary(canonical, sys.platform)
        if files.get(main.relative_to(canonical).as_posix()) != native.get("airk_sha256"):
            raise RuntimeError("Upgrade input main differs from runtime metadata: " + str(canonical))
        if native["schema"] == 1:
            helper = main.with_name("airk-embed.exe" if sys.platform == "win32" else "airk-embed").relative_to(canonical).as_posix()
            if files.get(helper) != native.get("embed_sha256") or not isinstance(native.get("embed_sha256"), str):
                raise RuntimeError("Upgrade input predecessor helper differs from runtime metadata: " + str(canonical))
        observations[role] = {"root": str(canonical), "version": version, "runtime_schema": native["schema"],
            "install_manifest_sha256": files["airk-install.json"], "sealed_files": len(install["files"]),
            "main_sha256": files[main.relative_to(canonical).as_posix()]}
    if clean_break and (not clean_break_required(observations["predecessor"]["version"], observations["candidate"]["version"])
            or observations["candidate"]["runtime_schema"] != 2):
        raise RuntimeError("Clean-break input preflight requires an actual pre-1.8.16 predecessor and supported single runtime")
    result = {"passed": True, "check_inputs_only": True, "runtime_executed": False, "inputs": observations,
            "scope": "Accessible canonical fixture roots and bounded complete sealed-file/version census; native activation verification remains separate."}
    if fixture_root is not None:
        result["legacy_path_budget"] = check_fixture_path_budget(fixture_root, runtime, previous_runtime, interrupted_runtime)
    return result


def verify_registry_version(version, previous, runtime_schema):
    # Schema-2 runtimes retain contract 12 until the first durable in-process
    # inference/provider record raises it to 13. Startup can perform that write
    # before this observation; neither state is a failed registry migration.
    # Older pair runtimes do not understand 13. Unknown versions are not accepted.
    allowed = (12, 13) if runtime_schema == 2 else (12,) if runtime_schema == 1 else ()
    if version not in allowed or version < previous:
        raise RuntimeError(
            f"Upgraded registry schema is {version}; expected {allowed} without "
            f"downgrade from {previous} for runtime schema {runtime_schema}")


def retire_obsolete_runtime(invoke, binary, install, expected, previous):
    """Native cleanup acceptance; editor tests separately prove config rewriting."""
    def processes(value, scope):
        return (isinstance(value, dict) and value.get("scope") == scope
                and type(value.get("remaining")) is int and value["remaining"] == 0
                and all(isinstance(value.get(field), list) and len(value[field]) <= 20
                        for field in ("observations", "image_roster", "image_exclusions")))

    launchers = invoke(binary, "runtime", "retire-legacy-launchers", "--install-dir", install,
                       "--expected-active", expected)
    if (type(launchers.get("schema")) is not int or launchers["schema"] != 1
            or launchers.get("active") != expected
            or launchers.get("legacy_launchers_retired") is not True
            or "launcher" not in launchers or launchers["launcher"] is not None
            or "universal_launcher" not in launchers or launchers["universal_launcher"] is not None
            or not processes(launchers.get("processes"), "verified_launcher_images")):
        raise RuntimeError("Old launcher retirement is incomplete")
    retired = invoke(binary, "runtime", "retire-previous", "--install-dir", install,
                     "--expected-active", expected)
    roster = retired.get("retired_runtimes")
    if (retired.get("active") != expected or retired.get("complete") is not True
            or retired.get("selected_service_restarted") is not False
            or not isinstance(roster, list) or not 1 <= len(roster) <= 4
            or any(not isinstance(item, str) or not re.fullmatch(r"[0-9a-f]{64}", item) for item in roster)
            or len(set(roster)) != len(roster) or expected in roster or previous not in roster
            or not processes(retired.get("processes"), "verified_superseded_runtime_images")):
        raise RuntimeError("Superseded runtime retirement is incomplete")
    if sorted(path.name for path in (install / "runtimes").iterdir()) != [expected]:
        raise RuntimeError("Obsolete runtime payloads remain after completed retirement")
    for name in ("airk-launcher.exe", "airk-launcher", "launcher.json"):
        if os.path.lexists(install / name):
            raise RuntimeError("Old launcher remains after completed retirement")
    return {"launchers": launchers, "runtimes": retired,
            "scope": "native retirement after real scoped HTTPS reconnect; editor configuration migration qualified separately"}


def capture_client_image(path):
    metadata = path.lstat()
    canonical = path.resolve(strict=True)
    if (not stat.S_ISREG(metadata.st_mode) or path.is_symlink() or metadata.st_nlink != 1
            or location_key(canonical) != location_key(path.absolute()) or metadata.st_size > 2 * 1024 * 1024 * 1024
            or getattr(metadata, "st_file_attributes", 0) & 0x400
            or hasattr(os, "getuid") and metadata.st_uid != os.getuid()):
        raise RuntimeError("Owned predecessor client image is not exact native regular bytes")
    return {"path": str(canonical), "sha256": digest(path), "bytes": metadata.st_size,
            "device": metadata.st_dev, "file_id": metadata.st_ino}


def darwin_client_exit(pid, exit_code, image, cleanup=None):
    """OS exit is not causal proof of a forced signal or a global census."""
    if (type(pid) is not int or pid <= 0 or type(exit_code) is not int
            or exit_code not in (0, -DARWIN_SIGKILL)
            or not isinstance(image, dict) or not isinstance(image.get("path"), str)
            or not Path(image["path"]).is_absolute()
            or not isinstance(image.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", image["sha256"])):
        raise RuntimeError("Darwin predecessor client did not report an accepted exact owned exit")
    result = {"schema": 1, "owned_client_pid": pid, "owned_client_exit_code": exit_code,
              "image_before_activation": image, "held_kernel_census": False,
              "signal_causality_proven": False, "qualified": False,
              "qualification_scope": "Exact owned Popen OS exit only; pending canonical retirement of its exact image. No global process census."}
    if cleanup is None:
        return result
    launchers = cleanup.get("launchers", {})
    processes = launchers.get("processes", {})
    if (type(launchers.get("schema")) is not int or launchers["schema"] != 1
            or launchers.get("legacy_launchers_retired") is not True
            or "launcher" not in launchers or launchers["launcher"] is not None
            or "universal_launcher" not in launchers or launchers["universal_launcher"] is not None
            or processes.get("scope") != "verified_launcher_images"
            or type(processes.get("remaining")) is not int or processes["remaining"] != 0):
        raise RuntimeError("Darwin predecessor exit lacks canonical completed launcher retirement")
    def exact(rows):
        if not isinstance(rows, list) or not 1 <= len(rows) <= 20:
            raise RuntimeError("Darwin predecessor image proof is absent or exceeds its bound")
        matches = [row for row in rows if isinstance(row, dict) and isinstance(row.get("path"), str)
                   and os.path.normpath(row["path"]) == image["path"]]
        if len(matches) != 1 or matches[0].get("sha256") != image["sha256"]:
            raise RuntimeError("Darwin predecessor exit crosses its original launcher path/hash")
        return matches[0]
    exact(processes.get("image_roster"))
    observed = exact(processes.get("observations")).get("observation", {})
    if (observed.get("complete") is not True or observed.get("failure") is not None
            or observed.get("matching_executable_pids") != []
            or type(observed.get("unobserved")) is not int or observed["unobserved"] != 0):
        raise RuntimeError("Darwin predecessor exit lacks complete exact-image process absence")
    result.update(qualified=True, native_launcher_retirement=launchers,
                  qualification_scope="Exact owned Popen exit and canonical completed retirement/path/hash process absence for that launcher image; no global census or forced-signal causality inferred.")
    return result


def isolated_git_environment(owner):
    """Explicit empty global rules for this fictional corpus; local rules remain.

    Only native child environments receive these paths. No user configuration
    or process-wide environment is modified.
    """
    root = Path(owner) / "fixture-git-config"
    root.mkdir(mode=0o700, exist_ok=True)
    if root.is_symlink() or getattr(root.lstat(),"st_file_attributes",0) & 0x400:
        raise RuntimeError("Redirected fictional Git configuration root")
    excludes, config = root / "global-excludes", root / "gitconfig"
    text = "[core]\n\texcludesFile = " + json.dumps(excludes.absolute().as_posix(),ensure_ascii=False) + "\n"
    for path, content in ((excludes,b""),(config,text.encode("utf-8"))):
        if path.exists() or path.is_symlink():
            metadata = path.lstat()
            if (not path.is_file() or path.is_symlink() or getattr(metadata,"st_file_attributes",0) & 0x400
                    or metadata.st_nlink != 1 or metadata.st_size != len(content) or path.read_bytes() != content):
                raise RuntimeError("Fictional Git configuration changed; preserve existing bytes")
        else:
            with path.open("xb") as output:
                output.write(content)
            path.chmod(0o600)
    home = root / "xdg"
    home.mkdir(mode=0o700, exist_ok=True)
    if home.is_symlink() or getattr(home.lstat(),"st_file_attributes",0) & 0x400:
        raise RuntimeError("Redirected fictional Git configuration home")
    return {"GIT_CONFIG_GLOBAL":str(config.resolve()),"GIT_CONFIG_NOSYSTEM":"1","XDG_CONFIG_HOME":str(home.resolve())}


def typed_https_read(read, retain, deadline):
    specification = importlib.util.spec_from_file_location("upgrade_typed_read", Path(__file__).with_name("check-service-clients.py"))
    reads = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(reads)
    def observe(until):
        value = read(until)
        retain(value)  # Preserve every actual outcome before retry classification.
        return value
    return reads.read_envelope(observe, deadline)[0]


def semantic_body(envelope):
    if envelope.get("isError") is True or not isinstance(envelope.get("content"),list) or not envelope["content"]:
        raise RuntimeError("Semantic fixture native read failed")
    value = json.loads(envelope["content"][0]["text"])
    if not isinstance(value,dict):
        raise RuntimeError("Semantic fixture native body is not an object")
    return value


def wait_semantic_job(rpc, accepted, project, kind):
    job = accepted.get("job",{})
    if (accepted.get("status") != "job" or not re.fullmatch(r"[0-9a-f]{64}",job.get("id",""))
            or job.get("project") != project or job.get("kind") != kind):
        raise RuntimeError("Old semantic submission did not return its exact job")
    deadline = time.monotonic()+180
    for _ in range(512):
        observed = rpc({"job":{"id":job["id"]}})["job"]
        if any(observed.get(key) != job[key] for key in ("id","project","kind")):
            raise RuntimeError("Old semantic job observation changed identity")
        if observed.get("state") in ("succeeded","failed","canceled"):
            if observed["state"] != "succeeded":
                raise RuntimeError("Old semantic job failed; no resubmission")
            return {"accepted":accepted,"completed":observed}
        if time.monotonic() >= deadline:
            break
        time.sleep(.1)
    raise RuntimeError("Old semantic job outcome unknown; no resubmission")


def prepare_discarded_job(rpc, project):
    """Create accepted predecessor authority only through its native owner."""
    binding = project["binding"]
    paused = rpc({"pause": {"project": binding, "paused": True}})
    if (paused.get("status") != "registered" or paused.get("project", {}).get("paused") is not True
            or paused["project"].get("registration") != project):
        raise RuntimeError("Pending reconciliation fixture did not pause its exact source binding")
    epoch = rpc("request_epoch")
    if (epoch.get("status") != "request_epoch" or not re.fullmatch(r"[0-9a-f]{32}", epoch.get("namespace", ""))
            or type(epoch.get("epoch")) is not int or epoch["epoch"] < 0):
        raise RuntimeError("Pending reconciliation fixture lacks a native request epoch")
    request = {"project": binding, "kind": "reconcile", "caller": uuid.uuid4().hex,
               "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}}
    accepted = rpc({"enqueue": request})
    job = accepted.get("job", {})
    if (accepted.get("status") != "job" or not re.fullmatch(r"[0-9a-f]{64}", job.get("id", ""))
            or job.get("project") != binding or job.get("kind") != "reconcile"
            or job.get("state") != "pending" or job.get("attempts") != 0 or job.get("outcome") is not None):
        raise RuntimeError("Pending reconciliation fixture did not retain its exact unstarted accepted job")
    query = {"request_receipt": {"project": binding, "caller": request["caller"], "key": request["key"]}}
    retained = rpc(query)
    if retained.get("status") != "request_receipt" or retained.get("expired") is not False or retained.get("job") != job:
        raise RuntimeError("Pending reconciliation native request receipt changed accepted authority")
    return {"registration": project, "pause": paused, "request": request, "accepted": accepted,
            "request_receipt_before": retained}


def semantic_jobs(database, project):
    with closing(sqlite3.connect(database.as_uri()+"?mode=ro",uri=True)) as sql:
        rows = sql.execute("SELECT id,kind,state,epoch,attempts,outcome FROM service_jobs WHERE project=? AND kind IN ('index','vectors','model_setup','rebuild','reconcile','recover','maintain') ORDER BY seq LIMIT 4097",(project,)).fetchall()
    if len(rows) > 4096:
        raise RuntimeError("Semantic maintenance job census exceeds bound")
    result = []
    for identifier,kind,state,epoch,attempts,outcome in rows:
        if outcome is not None and len(outcome) > 2048:
            raise RuntimeError("Semantic job outcome exceeds native bound")
        result.append({"id":identifier,"kind":kind,"state":state,"epoch":epoch,"attempts":attempts,
                       "outcome":json.loads(outcome) if outcome is not None else None})
    return result


def retained_model(control):
    """Verify the OLD owned extracted set; metadata alone is not model proof."""
    selected_root = control / "model-files-1.4"
    if selected_root.is_symlink() or getattr(selected_root.lstat(),"st_file_attributes",0)&0x400:
        raise RuntimeError("Old model store is redirected")
    root = selected_root.resolve(strict=True)
    references = list(root.glob("default-*.json"))
    if len(references) != 1:
        raise RuntimeError("Expected one actual old shared default model")
    def regular(path,limit):
        metadata = path.lstat()
        if (path.is_symlink() or not path.is_file() or metadata.st_nlink != 1
                or getattr(metadata,"st_file_attributes",0)&0x400 or metadata.st_size > limit
                or not path.resolve(strict=True).is_relative_to(root)):
            raise RuntimeError("Old model reference is redirected or exceeds bound")
    regular(references[0],16384)
    raw = references[0].read_bytes()
    selected = json.loads(raw)
    identity = selected.get("shared",{}).get("set","")
    if (not re.fullmatch(r"[0-9a-f]{64}",identity)
            or not re.fullmatch(r"[0-9a-f]{64}",selected.get("legacy_identity",""))
            or not re.fullmatch(r"[0-9a-f]{64}",selected.get("pipeline_digest",""))
            or set(selected.get("assets",{})) != {"model.json","model.onnx","tokenizer.json","LICENSE"}):
        raise RuntimeError("Old semantic model is not a complete shared ONNX set")
    directory = root / "sets" / identity
    for entry in (root,directory.parent,directory):
        if entry.is_symlink() or getattr(entry.lstat(),"st_file_attributes",0)&0x400:
            raise RuntimeError("Old model store is redirected")
    for name,asset in selected["assets"].items():
        path = directory / name
        regular(path,16384 if name == "model.json" else 64*1024*1024)
        if (type(asset.get("bytes")) is not int or not 0 < asset["bytes"] <= 64*1024*1024
                or not re.fullmatch(r"[0-9a-f]{64}",asset.get("sha256",""))
                or path.stat().st_size != asset["bytes"] or digest(path) != asset["sha256"]):
            raise RuntimeError("Old actual model asset differs from its set")
    manifest = json.loads((directory / "model.json").read_bytes())
    catalog = json.loads((Path(__file__).resolve().parents[1] / "crates/airk-retrieval/src/model_catalog.json").read_bytes())
    matches = [profile for profile in catalog["profiles"] if profile["manifest"] == manifest]
    if len(matches) != 1 or manifest.get("dimensions") != 384:
        raise RuntimeError("Old actual model is not an approved pinned MiniLM export")
    for asset in matches[0]["assets"]:
        observed = selected["assets"].get(asset["name"],{})
        if observed.get("sha256") != asset["sha256"] or observed.get("bytes") != asset["bytes"]:
            raise RuntimeError("Old model set assets do not match the approved MiniLM manifest")
    return {"reference_sha256":hashlib.sha256(raw).hexdigest(),"reference":selected,"manifest":manifest,"profile":matches[0]["id"]}


def publication_identity(value, generation=None, collection=None):
    """Validate token and every expanded identity field without rewriting replies."""
    if not isinstance(value, dict):
        raise RuntimeError("Native publication is not an identity object")
    match = re.fullmatch(r"airk-r1:([0-9a-f]{32}):([1-9][0-9]*):([0-9a-f]{64})", value.get("token", ""))
    if match is None:
        raise RuntimeError("Native publication token is malformed")
    incarnation, number, revision = match.groups()
    number = int(number)
    if number > 2**64-1 or (generation is not None and (type(generation) is not int or generation != number)):
        raise RuntimeError("Native publication generation changed")
    for field, expected in (("incarnation",incarnation),("generation",number),("revision",revision)):
        if field in value and (value[field] != expected or (field == "generation" and type(value[field]) is not int)):
            raise RuntimeError("Expanded native publication differs from its token")
    if "collection" in value and (not isinstance(value["collection"],str) or not value["collection"]
            or (collection is not None and value["collection"] != collection)):
        raise RuntimeError("Native publication collection changed")
    return {"token":value["token"],"incarnation":incarnation,"generation":number,"revision":revision}


def semantic_identity(status, model, queries):
    ready = status.get("semantic_readiness",{})
    if (ready.get("state") != "ready" or ready.get("completed_artifacts") != 1
            or ready.get("selected_artifacts") != 1 or status.get("indexed_vectors",0) < 1
            or not isinstance(status.get("publication"),dict)):
        raise RuntimeError("Actual retained semantic publication is incomplete or changed pipeline")
    # Scoped status intentionally summarizes readiness without exposing the
    # collection pipeline. Qualify that identity through both actual full-source
    # semantic reads, not a field invented in the scoped status response.
    if set(queries) != {"vector", "hybrid"}:
        raise RuntimeError("Retained semantic identity requires both actual query proofs")
    pipeline = model["reference"]["pipeline_digest"]
    published = publication_identity(status["publication"],status["generation"],status.get("collection"))
    selections = []
    for mode, query in queries.items():
        semantic = query.get("semantic_readiness", {})
        execution = query.get("execution", {})
        if (publication_identity(query.get("publication"),query.get("generation"),status.get("collection")) != published or query.get("generation") != status["generation"]
                or semantic.get("state") != "ready" or semantic.get("pipeline_digest") != pipeline
                or semantic.get("selected_artifacts") != 1 or semantic.get("completed_artifacts") != 1
                or execution.get("requested_mode") != mode or execution.get("served_mode") != mode
                or execution.get("incomplete") is not False
                or not re.fullmatch(r"[0-9a-f]{64}", semantic.get("selection_digest", ""))):
            raise RuntimeError("Actual semantic query proof differs from the retained publication or pipeline")
        selections.append(semantic["selection_digest"])
    if selections[0] != selections[1] or ("pipeline_digest" in ready and ready["pipeline_digest"] != pipeline) or ("selection_digest" in ready and ready["selection_digest"] != selections[0]):
        raise RuntimeError("Retained semantic status/query identity differs")
    return {"publication":published,"generation":status["generation"],"indexed_vectors":status["indexed_vectors"],
            "pipeline_digest":pipeline,"selection_digest":selections[0],"identity_provenance":"actual_vector_and_hybrid_publication_reads",
            "legacy_model_identity":model["reference"]["legacy_identity"],"model_profile":model["profile"]}


def assert_semantic_query(body, mode, publication, pipeline, original):
    execution = body.get("execution",{})
    if (execution.get("requested_mode") != mode or execution.get("served_mode") != mode
            or execution.get("incomplete") is not False
            or publication_identity(body.get("publication"),body.get("generation"),body.get("collection")) != publication_identity(publication)
            or body.get("semantic_readiness",{}).get("pipeline_digest") != pipeline):
        raise RuntimeError("Semantic query omitted the requested vector work or retained identity")
    if not any(hit.get("citation",{}).get("path") == "README.md"
               and hit["citation"].get("content_hash") == hashlib.sha256(original).hexdigest()
               and original.decode().strip() in hit.get("text","") for hit in body.get("hits",[])):
        raise RuntimeError("Semantic query lost exact retained source coordinates/hash/text")


def verify_https_reconnect(invoke, binary, install, control, root, project, selected, configuration_receipt, original, semantic_before=None, command=None):
    """Reconnect through preserved native authority, not the terminated launcher."""
    spec = importlib.util.spec_from_file_location("upgrade_http_fixture", Path(__file__).with_name("http_fixture.py"))
    transport = importlib.util.module_from_spec(spec); spec.loader.exec_module(transport)
    identifier = uuid.uuid4().hex
    if selected is not None:
        connection = transport.enroll_native(
            lambda *arguments, timeout=None: invoke(binary, *arguments, timeout=timeout),
            install, control, {"kind": "scoped", "selection": selected}, identifier)
        expected, observed = selected.copy(), connection["selection"]["selection"].copy()
        expected.pop("expected_catalog_revision"); observed.pop("expected_catalog_revision")
        if connection["selection"]["kind"] != "scoped" or observed != expected:
            raise RuntimeError("HTTPS reconnect changed the exact reviewed authority")
    else:
        directory = control / "http-fixture-inputs"
        directory.mkdir(mode=0o700, exist_ok=False)
        registration = directory / "registration.json"
        with os.fdopen(os.open(registration, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w", encoding="utf-8") as output:
            json.dump(project, output)
        connection = invoke(binary, "runtime", "http-legacy-connection", "--install-dir", install,
            "--registration-file", registration, "--configuration-receipt", configuration_receipt,
            "--connection-id", identifier, "--client", "unknown")
        if connection["selection"]["kind"] != "imported_legacy" or connection["selection"]["selection"]["registration"] != project:
            raise RuntimeError("HTTPS reconnect changed predecessor registration or claimed a new grant")
    health = invoke(binary, "service", "rpc", "--runtime-dir", control, "--stdin", input=json.dumps("health"))["observation"]
    if connection["connection_id"] != identifier:
        raise RuntimeError("Upgraded HTTPS connection identity changed")
    transport.validate_native_identity(connection, health)
    evidence_file = root / "https-reconnect-evidence.json"
    evidence = {"connection": connection, "native_health": health}
    with evidence_file.open("x", encoding="utf-8") as output:
        json.dump(evidence, output, indent=2)
    def retain(name, value):
        evidence[name] = value
        evidence_file.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    network = transport.McpClient(connection, control)
    try:
        network.send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "upgraded-http-fixture", "version": "1"}}})
        initialized = network.receive(1)
        retain("initialize", initialized)
        if initialized["result"]["serverInfo"]["name"] != "airk":
            raise RuntimeError("Upgraded HTTPS server identity differs")
        network.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        arguments = {"path": "README.md", "start_line": 1, "end_line": 1, "response_mode": "full"}
        evidence["read_attempts"] = []
        def read(route, until):
            remaining = until-time.monotonic()
            if remaining <= 0:
                raise TimeoutError("Upgraded HTTPS source-read deadline exhausted")
            network.connection.timeout = remaining
            if network.connection.sock is not None:
                network.connection.sock.settimeout(remaining)
            if route == "mcp":
                network.send({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "read_file", "arguments": arguments}})
                return network.receive(2)["result"]
            return network.api_read("read_file", arguments)
        def attempt(route, value):
            evidence["read_attempts"].append({"transport":route,"result":value})
            evidence_file.write_text(json.dumps(evidence,indent=2)+"\n",encoding="utf-8")
        deadline = time.monotonic()+60
        mcp = typed_https_read(lambda until:read("mcp",until),lambda value:attempt("mcp",value),deadline)
        retain("mcp", mcp)
        api = typed_https_read(lambda until:read("json_api",until),lambda value:attempt("json_api",value),deadline)
        retain("json_api", api)
        def body(envelope):
            if envelope.get("isError") is True or len(envelope.get("content", [])) != 1:
                raise RuntimeError("Upgraded HTTPS read failed or omitted its complete native body")
            return json.loads(envelope["content"][0]["text"])
        mcp_body, api_body = body(mcp), body(api)
        if (transport.parity_envelope(mcp) != transport.parity_envelope(api)
                or transport.parity_value(mcp_body) != transport.parity_value(api_body)):
            raise RuntimeError("Upgraded MCP/JSON API complete read results differ")
        hits = mcp_body.get("hits", [])
        if not any(hit.get("citation", {}).get("path") == "README.md"
                   and hit["citation"].get("content_hash") == hashlib.sha256(original).hexdigest()
                   and original.decode().strip() in hit.get("text", "") for hit in hits):
            raise RuntimeError("Upgraded HTTPS connection did not read the exact retained source revision")
        semantic_after = None
        if semantic_before is not None:
            if command is None:
                raise RuntimeError("Semantic upgrade requires the actual CLI transport")
            cli_scope = ["--service-runtime-dir",str(control)]
            if selected is not None:
                cli_scope += ["--catalog-session",json.dumps(connection["selection"]["selection"])]
            else:
                cli_scope += ["--service-project",project["binding"],"--root",project["root"],"--data-dir",project["data_root"],"--collection",project["collection"]]
            semantic_after = {"queries":{},"attempts":[]}
            def semantic_read(route, tool, arguments, until):
                remaining = until-time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("Retained semantic read deadline exhausted")
                network.connection.timeout = remaining
                if network.connection.sock is not None:
                    network.connection.sock.settimeout(remaining)
                if route == "cli":
                    reply = command(binary,*cli_scope,"tools","call","airk_"+tool,"--args",json.dumps(arguments),timeout=remaining)
                    if reply.returncode not in (0,1):
                        raise RuntimeError("Current semantic CLI transport failed")
                    return json.loads(reply.stdout)
                if route == "json_api":
                    return network.api_read(tool,arguments)
                network.send({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":tool,"arguments":arguments}})
                return network.receive(10)["result"]
            def actual_read(route,tool,arguments):
                def retain_attempt(value):
                    semantic_after["attempts"].append({"transport":route,"tool":tool,"arguments":arguments,"result":value})
                    retain("semantic",semantic_after)
                return typed_https_read(lambda until:semantic_read(route,tool,arguments,until),retain_attempt,time.monotonic()+90)
            status = actual_read("mcp","status",{"response_mode":"full"})
            semantic_after["status"] = status
            semantic_after["model"] = retained_model(control)
            for mode, before in semantic_before["queries"].items():
                results = {route:actual_read(route,"search",before["arguments"]) for route in ("mcp","json_api","cli")}
                if any(transport.parity_envelope(value) != transport.parity_envelope(results["mcp"]) for value in results.values()):
                    raise RuntimeError("Current retained semantic CLI/HTTPS full-envelope parity differs")
                observed = semantic_body(results["mcp"])
                assert_semantic_query(observed,mode,semantic_before["identity"]["publication"],semantic_before["identity"]["pipeline_digest"],original)
                if publication_identity(observed.get("publication"),observed.get("generation"),observed.get("collection")) != publication_identity(before["body"].get("publication"),before["body"].get("generation"),before["body"].get("collection")):
                    raise RuntimeError("Retained semantic query changed publication")
                def citations(value):
                    return [{"citation":hit["citation"],"text":hit["text"]} for hit in value.get("hits",[])]
                if not observed.get("hits") or citations(observed) != citations(before["body"]):
                    raise RuntimeError("Retained semantic source/text/coordinate evidence changed")
                semantic_after["queries"][mode] = results
                retain("semantic",semantic_after)
            semantic_after["identity"] = semantic_identity(semantic_body(status),semantic_after["model"],
                {mode:semantic_body(results["mcp"]) for mode,results in semantic_after["queries"].items()})
            if semantic_after["model"] != semantic_before["model"] or semantic_after["identity"] != semantic_before["identity"]:
                raise RuntimeError("Upgrade changed retained model/pipeline/vector publication identity")
            provider = invoke(binary,"service","rpc","--runtime-dir",control,"--stdin",input=json.dumps("embedding_status"))
            observation = provider.get("provider",{})
            if (provider.get("status") != "embedding_status" or provider.get("available") is not True
                    or provider.get("process_id") is not None or observation.get("kind") != "in_process"
                    or type(provider.get("loads")) is not int or provider["loads"] < 1
                    or type(observation.get("admitted_high_water")) is not int or observation["admitted_high_water"] < 1
                    or observation.get("provider",{}).get("service_instance") != health["instance"]
                    or not re.fullmatch(r"[0-9a-f]{64}",observation.get("provider",{}).get("approved_execution_sha256",""))):
                raise RuntimeError("Retained semantic queries did not use the actual native in-process provider")
            semantic_after["provider"] = provider
            semantic_after["jobs"] = semantic_jobs(control / "service-1.4.sqlite3",project["binding"])
            if semantic_after["jobs"] != semantic_before["jobs"]:
                raise RuntimeError("Upgrade or retained semantic reads changed/reissued index/vector/model jobs")
            semantic_after["score_comparison"] = "Full current CLI/HTTPS results compared; old/new floating-point score equality is not assumed."
            retain("semantic",semantic_after)
        return {"transport": "https", "connection_id": identifier, "selection": connection["selection"],
                "endpoint": connection["endpoint"], "mcp": mcp, "json_api": api,
                "same_original_source": True, "complete_mcp_api_parity": True,"retained_semantic":semantic_after}
    finally:
        network.close()


def reviewed_fixture(rpc, source, data, collection="upgrade"):
    """Explicitly set up one fictional Index and a one-Project grant natively."""
    def key():
        epoch = rpc("request_epoch")
        return {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}
    context_key = uuid.uuid4().hex + uuid.uuid4().hex
    descriptor = rpc({"catalog_host_context": {"key": context_key, "expected_catalog_revision": None}})["descriptor"]
    selection = {"key": key(), "expected_catalog_revision": descriptor["catalog_revision"],
                 "name": "Previous-version upgrade fixture", "primary_root": str(source),
                 "data_root": str(data), "collection": collection, "additional_roots": [], "setup_watch": False}
    review = rpc({"catalog_index_setup_draft": {"selection": selection}})
    setup = rpc({"catalog_index_setup_apply": {"draft": review["draft"]}})
    if setup.get("status") != "catalog_index_setup_applied" or setup["receipt"]["stage"] != "ready":
        raise RuntimeError("Reviewed predecessor fixture setup did not reach ready: " + json.dumps(setup))
    receipt = setup["receipt"]
    revision, index, project = receipt["catalog_revision"], receipt["index_id"], "prj_" + uuid.uuid4().hex
    def apply(operation):
        nonlocal revision
        request = {"caller": uuid.uuid4().hex, "key": key(), "expected_catalog_revision": revision, "operation": operation}
        preview = rpc({"catalog_preview": {"request": request}})
        response = rpc({"catalog_apply": {"request": request, "preview_digest": preview["preview_digest"]}})
        if response.get("status") != "catalog_applied":
            raise RuntimeError("Reviewed predecessor metadata was not applied: " + json.dumps(response))
        revision = response["receipt"]["catalog_revision"]
    apply({"operation": "create_project", "id": project, "name": "Upgrade fixture Project"})
    apply({"operation": "set_membership", "membership": {"project_id": project, "index_id": index, "revision": 1,
           "scope": [{"source_root_id": receipt["source_ids"][0], "kind": "subtree", "path": ""}]},
           "expected_scope_revision": 1, "expected_membership_revision": None})
    descriptor = rpc({"catalog_host_context": {"key": context_key, "expected_catalog_revision": revision}})["descriptor"]
    grant = rpc({"catalog_grant_draft": {"context_key": context_key, "selection": {"key": key(),
        "expected_catalog_revision": revision, "project_ids": [project], "index_ids": [index]}}})["draft"]
    granted = rpc({"catalog_apply": {"request": grant["request"], "preview_digest": grant["preview_digest"]}})
    if granted.get("status") != "catalog_applied":
        raise RuntimeError("Reviewed predecessor grant was not applied: " + json.dumps(granted))
    selected = {"schema": 1, "context_key": context_key, "namespace": descriptor["namespace"],
        "expected_catalog_revision": granted["receipt"]["catalog_revision"], "grant_id": grant["request"]["operation"]["grant"]["id"],
        "expected_grant_revision": 1, "index_id": index, "project_ids": [project]}
    detail = rpc({"catalog_detail": {"entity": {"kind": "index", "id": index}, "expected_catalog_revision": selected["expected_catalog_revision"]}})["detail"]
    registration = detail["item"]["value"]["backing"]
    if (not Path(registration["root"]).samefile(source) or not Path(registration["data_root"]).samefile(data)
            or registration["collection"] != collection):
        raise RuntimeError("Reviewed predecessor setup returned a different fictional registration")
    return registration, selected


def reviewed_snapshot(rpc, selected):
    """Preserve complete exact catalog values, excluding only response capabilities."""
    response = rpc({"catalog_host_context": {"key": selected["context_key"], "expected_catalog_revision": None}})
    descriptor = response["descriptor"]
    if descriptor["namespace"] != selected["namespace"]:
        raise RuntimeError("Reviewed fixture namespace changed")
    revision = descriptor["catalog_revision"]
    result = {"namespace": descriptor["namespace"], "revision": revision}
    for kind, identifier in [("index", selected["index_id"]), ("project", selected["project_ids"][0]), ("grant", selected["grant_id"])]:
        detail = rpc({"catalog_detail": {"entity": {"kind": kind, "id": identifier}, "expected_catalog_revision": revision}})["detail"]
        if detail["namespace"] != descriptor["namespace"] or detail["revision"] != revision or detail["item"]["kind"] != kind:
            raise RuntimeError("Crossed reviewed fixture catalog snapshot")
        result[kind] = detail["item"]["value"]
    memberships = rpc({"catalog_page": {"request": {"kind": "memberships", "project_id": selected["project_ids"][0],
        "index_id": selected["index_id"], "after": None, "expected_catalog_revision": revision, "limit": 16, "max_bytes": 48000}}})["page"]
    if (memberships["namespace"] != descriptor["namespace"] or memberships["revision"] != revision
            or memberships["continuation"] is not None or len(memberships["items"]) != 1):
        raise RuntimeError("Reviewed fixture membership snapshot is incomplete or not singular")
    result["memberships"] = memberships["items"]
    return result, response.get("capabilities")


def record_native_command(root, records, executable, arguments, *, input=None, timeout=180, flags=0, environment=None):
    ordinal = len(records) + 1
    row = {"ordinal": ordinal, "command": [str(executable), *map(str, arguments)], "exit_code": None}
    def persist(stdout, stderr):
        for suffix, content in [("stdout", stdout), ("stderr", stderr)]:
            raw = content if isinstance(content, bytes) else (content or "").encode("utf-8")
            (root / f"command-{ordinal:03d}.{suffix}").write_bytes(raw)
        (root / "commands.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    try:
        result = subprocess.run(row["command"], input=input, capture_output=True,
            text=True, encoding="utf-8", timeout=timeout, creationflags=flags, env=environment)
    except subprocess.TimeoutExpired as error:
        row.update(failure_type="TimeoutExpired", timeout_seconds=timeout)
        records.append(row)
        try:
            persist(error.stdout, error.stderr)
        except Exception as logging_error:
            row["diagnostic_error"] = repr(logging_error)
        raise  # Preserve the exact original timeout and its partial streams.
    row["exit_code"] = result.returncode
    records.append(row)
    try:
        persist(result.stdout, result.stderr)
    except Exception as logging_error:
        row["diagnostic_error"] = repr(logging_error)
        if not result.returncode:
            raise  # A successful operation without evidence does not qualify.
    return result


def owned_temporary(parent):
    temporary = tempfile.TemporaryDirectory(prefix="au-", dir=parent)
    # Retention is established BEFORE native activation or any fallible cleanup.
    # Success explicitly removes this owned fixture after acknowledged uninstall.
    temporary._finalizer.detach()
    return temporary


def stop_owned_runtime(command, binary, install, control, runtime_digest, execution_digest,
                       *, observed_health=None, owner="owned_fixture"):
    """Use the instance-fenced lifecycle CLI and retain its exact request and full receipt."""
    if (install / "activation.json").exists():
        raise RuntimeError("Pending activation forbids fixture lifecycle cleanup")
    if digest(binary) != execution_digest:
        raise RuntimeError("Fixture cleanup executable changed")
    report = {"owner": owner, "process_exit_confirmed": False, "lifecycle_complete": False, "native_stop": None}
    if observed_health is None:
        result = command(binary, "service", "rpc", "--runtime-dir", control, "--stdin", input=json.dumps("health"), timeout=15)
        report["health_exit_code"] = result.returncode
        if result.returncode != 0:
            return report
        health = json.loads(result.stdout)
    else:
        health = observed_health
        report["health_exit_code"] = 0
    report["health"] = health
    observed = health.get("observation",{})
    if (health.get("status") != "health" or observed.get("runtime_sha256") != runtime_digest
            or observed.get("executable_sha256") != execution_digest
            or not re.fullmatch(r"[0-9a-f]{32}",observed.get("instance",""))
            or type(observed.get("process_id")) is not int or observed["process_id"] <= 0):
        raise RuntimeError("Fixture cleanup owner identity unavailable or crossed")
    epoch_result = command(binary, "service", "rpc", "--runtime-dir", control, "--stdin", input=json.dumps("request_epoch"), timeout=15)
    if epoch_result.returncode != 0:
        raise RuntimeError("Fixture lifecycle epoch unavailable; no Stop request submitted")
    epoch = json.loads(epoch_result.stdout)
    report["request_epoch"] = epoch
    if (epoch.get("status") != "request_epoch" or not re.fullmatch(r"[0-9a-f]{32}", epoch.get("namespace", ""))
            or type(epoch.get("epoch")) is not int or not 0 <= epoch["epoch"] <= (1 << 64)-1):
        raise RuntimeError("Fixture lifecycle epoch is incomplete or crossed; no Stop request submitted")
    request = {"caller": uuid.uuid4().hex,
        "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
        "action": "stop", "expected_instance": observed["instance"], "runtime": runtime_digest, "force": False}
    if (install / "activation.json").exists() or digest(binary) != execution_digest:
        raise RuntimeError("Fixture cleanup authority changed before Stop submission")
    request_file = install.parent / "fixture-stop-request.json"
    observation_file = install.parent / "fixture-stop-observation.json"
    if os.path.lexists(observation_file):
        raise FileExistsError(observation_file)
    with request_file.open("x", encoding="utf-8") as output:
        output.write(json.dumps(request, indent=2)+"\n")
        output.flush(); os.fsync(output.fileno())
    report.update(request=request, request_file=str(request_file), observation_file=str(observation_file))
    def retain(active_error=None):
        try:
            with observation_file.open("x", encoding="utf-8") as output:
                output.write(json.dumps(report, indent=2)+"\n")
                output.flush(); os.fsync(output.fileno())
        except Exception as error:
            if active_error is None:
                raise
            print(f"Secondary fixture Stop receipt error: {error!r}; original request retained at {request_file}", file=sys.stderr)
    try:
        stopped = command(binary, "runtime", "lifecycle", "--install-dir", install, input=json.dumps(request), timeout=180)
    except BaseException as error:
        report["outcome_unknown"] = {"type": type(error).__name__, "message": str(error)}
        retain(active_error=error)
        raise
    report["stop_exit_code"] = stopped.returncode
    try:
        response = json.loads(stopped.stdout)
    except (TypeError, ValueError):
        response = {"stdout": stopped.stdout, "stderr": stopped.stderr}
    report["native_stop"] = response
    if stopped.returncode != 0:
        report["stderr"] = stopped.stderr
    record = response.get("record", {}) if isinstance(response, dict) else {}
    expected = {key: value for key, value in request.items() if key != "force"}
    expected["manager"] = False  # This fixture never registers OS-managed startup.
    report["lifecycle_complete"] = (stopped.returncode == 0 and isinstance(response, dict)
        and response.get("status") == "lifecycle" and isinstance(record, dict) and record.get("phase") == "complete"
        and isinstance(record.get("id"), str) and re.fullmatch(r"[0-9a-f]{64}", record["id"]) is not None
        and record.get("request") == expected)
    retain()
    if not report["lifecycle_complete"]:
        raise RuntimeError(f"Fixture lifecycle Stop outcome is unconfirmed or crossed; retain {request_file} and {observation_file}")
    # A native lifecycle acknowledgment is retained verbatim; no PID absence or
    # successful teardown is inferred from the receipt alone.
    return report


def stop_predecessor_before_staging(command, old_binary, install, control, runtime_digest, execution_digest):
    """Only the isolated original owner; callers must fence all candidate effects."""
    return stop_owned_runtime(command, old_binary, install, control, runtime_digest, execution_digest,
        owner="pre_candidate_predecessor")


def finish_fixture(temporary, root, completed, uninstall, client=None, client_log=None, observe_stop=None):
    errors = []
    def attempt(name, operation):
        try:
            return operation()
        except Exception as error:
            errors.append({"operation": name, "error": repr(error)})
            return None
    if client is not None:
        if client.poll() is None:
            attempt("kill_owned_client", client.kill)
        attempt("wait_owned_client", lambda: client.wait(timeout=15))
        attempt("close_owned_stdin", client.stdin.close)
        attempt("close_owned_stdout", client.stdout.close)
    if client_log is not None:
        attempt("close_client_log", client_log.close)
    pending = (root / "install/activation.json").exists()
    stopped = None
    # A committed pending catalog upgrade requires explicit owner recovery.
    # Never auto-abort it, restart an old metadata reader, or mask it by uninstall.
    acknowledged = False
    if completed and not pending:
        if not (root / "install").is_dir():
            errors.append({"operation": "uninstall", "error": "Owned installation missing; no native uninstall acknowledgment"})
        else:
            value = attempt("uninstall", uninstall)
            acknowledged = isinstance(value, dict) and value.get("uninstalled") is True
            if not acknowledged and not errors:
                errors.append({"operation": "uninstall", "error": "Native uninstall deferred or unacknowledged"})
    elif observe_stop is not None:
        stopped = attempt("observe_owned_candidate_stop", observe_stop)
    removed = completed and not pending and acknowledged and not errors
    if removed:
        attempt("remove_completed_fixture", temporary.cleanup)
        removed = not errors
    if not removed:
        print(f"Preserving owned fictional upgrade fixture: {root}", flush=True)
    return {"errors": errors, "activation_pending": pending, "uninstall_acknowledged": acknowledged, "fixture_retained": not removed,
            "candidate_stop_observation": stopped, "teardown_complete": removed}


def run_clean_break(args, new, old):
    """Qualify the actual boundary with two independent saved indexes."""
    if args.output.exists():
        raise FileExistsError(args.output)
    binary, old_binary = runtime_binary(new), runtime_binary(old)
    temporary = owned_temporary(fixture_parent(args.fixture_base))
    root = Path(temporary.name)
    install, old_state = root / "install", root / "state"
    old_control = old_state / "control"
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1")
    environment.update(isolated_git_environment(root))
    records, originals, source_inputs = [], [], []
    client = client_log = census = None
    completed, candidate_staged, primary_error = False, False, None
    control = old_control
    old_identity, new_identity = digest(old / "airk-install.json"), digest(new / "airk-install.json")
    receipt = {"passed": False, "qualification_scope": "pre-1.8.16 forced clean break with all saved fictional indexes",
        "fixture_root": str(root), "candidate": new_identity, "previous": old_identity,
        "previous_version": json.loads((old / "airk-install.json").read_text(encoding="utf-8"))["version"],
        "version": json.loads((new / "airk-install.json").read_text(encoding="utf-8"))["version"],
        "indexes_before": originals, "source_inputs": source_inputs}
    def command(executable, *arguments, input=None, timeout=180):
        return record_native_command(root, records, executable, arguments, input=input,
            timeout=timeout, flags=flags, environment=environment)
    def invoke(executable, *arguments, input=None, timeout=180):
        result = command(executable, *arguments, input=input, timeout=min(timeout, 180))
        if result.returncode:
            raise RuntimeError(f"Native clean-break fixture failed: {result.stderr[-4096:]}")
        return json.loads(result.stdout)
    def rpc(executable, runtime_control, method, allow_failure=False):
        response = invoke(executable, "service", "rpc", "--runtime-dir", runtime_control, "--stdin", input=json.dumps(method))
        if response.get("status") == "failure" and not allow_failure:
            raise RuntimeError("Native clean-break control refused: " + json.dumps(response))
        return response
    old_rpc = lambda method: rpc(old_binary, old_control, method)
    try:
        receipt["legacy_path_budget"] = check_fixture_path_budget(root, new, old)
        invoke(old_binary, "runtime", "stage", "--source", old, "--install-dir", install, "--state-dir", old_state,
            "--approved-sha256", old_identity, "--verification-basis", "organization-digest")
        invoke(old_binary, "runtime", "activate", "--install-dir", install, "--runtime-sha256", old_identity)
        invoke(old_binary, "runtime", "ensure", "--install-dir", install)
        old_namespace = old_rpc("request_epoch")["namespace"]
        receipt["old_namespace"] = old_namespace
        for position in range(2):
            source, data = root / f"source{position}", root / f"data{position}"
            source.mkdir(); data.mkdir()
            original = f"Fictional clean-break source number {position}; preserve these original bytes.\n".encode()
            (source / "README.md").write_bytes(original)
            source_inputs.append({"source_file": str(source / "README.md"), "source_sha256": hashlib.sha256(original).hexdigest()})
            collection = f"clean{position}"
            if args.fixture_setup == "reviewed":
                registration, selection = reviewed_fixture(old_rpc, source, data, collection)
            else:
                registration = invoke(old_binary, "--root", source, "--data-dir", data, "--collection", collection,
                    "service", "prepare-registration")
                saved = old_rpc({"register": {"project": registration}})
                if saved.get("status") != "registered" or saved.get("project", {}).get("registration") != registration:
                    raise RuntimeError("Predecessor did not register the exact fictional source")
                selection = None
            scope = ["--service-runtime-dir", str(old_control), "--service-project", registration["binding"],
                "--root", registration["root"], "--data-dir", registration["data_root"], "--collection", collection]
            accepted = invoke(old_binary, *scope, "configure", "--stdin", input=json.dumps({"operation": "vectors", "enabled": False}))
            configured = wait_semantic_job(old_rpc, accepted, registration["binding"], "configure")
            accepted = invoke(old_binary, *scope, "index")
            indexed = wait_semantic_job(old_rpc, accepted, registration["binding"], "index")
            watched = old_rpc({"configure_watch": {"project": registration["binding"], "enabled": position == 1}})
            watch = old_rpc({"watch": {"project": registration["binding"]}})
            if watch.get("status") != "watch" or watch.get("observation", {}).get("enabled") is not (position == 1):
                raise RuntimeError("Predecessor did not retain the explicit watch preference")
            item = {"registration": registration, "selection": selection, "sources": {}, "paused": position == 1,
                "watch": position == 1, "vectors": False, "source_file": str(source / "README.md"),
                "source_sha256": hashlib.sha256(original).hexdigest(), "source_text": original.decode(),
                "configured": configured, "indexed_job": indexed["completed"], "watch_observation": watch,
                "configure_watch": watched}
            originals.append(item)
        receipt["discarded_pending_work"] = prepare_discarded_job(old_rpc, originals[1]["registration"])
        launcher = install / ("airk-launcher.exe" if os.name == "nt" else "airk-launcher")
        receipt["predecessor_client_image"] = capture_client_image(launcher)
        first = originals[0]
        if first["selection"] is not None:
            context = old_rpc({"catalog_host_context": {"key": first["selection"]["context_key"], "expected_catalog_revision": None}})["descriptor"]
            if context["namespace"] != old_namespace:
                raise RuntimeError("Predecessor client scope crossed its original namespace")
            first["selection"]["expected_catalog_revision"] = context["catalog_revision"]
        client_scope = ["--catalog-session", json.dumps(first["selection"])] if first["selection"] else [
            "--service-project", first["registration"]["binding"], "--root", first["registration"]["root"],
            "--data-dir", first["registration"]["data_root"], "--collection", first["registration"]["collection"]]
        old_command = [str(launcher), "runtime", "launch", "--install-dir", str(install), "--", *client_scope, "mcp"]
        old_configuration = root / "old-client-configuration.json"
        with old_configuration.open("x", encoding="utf-8") as output:
            json.dump({"command": old_command[0], "args": old_command[1:]}, output)
        client_log = (root / "client.stderr").open("wb")
        client = subprocess.Popen(old_command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=client_log,
            text=True, encoding="utf-8", creationflags=flags, env=environment)
        replies = queue.Queue()
        def receive():
            try:
                for line in client.stdout:
                    replies.put(line)
            finally:
                replies.put(None)
        threading.Thread(target=receive, daemon=True).start()
        client.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "clean-break-old-fixture", "version": "1"}}})+"\n")
        client.stdin.flush()
        line = replies.get(timeout=60)
        initialized = json.loads(line) if line is not None else {}
        if initialized.get("id") != 1 or "error" in initialized:
            raise RuntimeError("Predecessor client did not initialize before reset")
        client.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"})+"\n"); client.stdin.flush()
        old_health = old_rpc("health")["observation"]
        if (old_health.get("runtime_sha256") != old_identity or type(old_health.get("process_id")) is not int
                or old_health["process_id"] <= 0):
            raise RuntimeError("Predecessor health crossed its approved runtime")
        if sys.platform != "darwin":
            specification = importlib.util.spec_from_file_location("clean_break_owned_processes", Path(__file__).with_name("owned-runtime-processes.py"))
            processes = importlib.util.module_from_spec(specification); specification.loader.exec_module(processes)
            census = processes.capture(install.resolve(strict=True), old_identity, {client.pid, old_health["process_id"]})
            receipt["predecessor_processes_before"] = census.before
        else:
            receipt["predecessor_processes_before"] = {"held_kernel_census": False, "native_health": old_health, "owned_client_pid": client.pid}
        candidate_staged = True
        invoke(binary, "runtime", "stage", "--source", new, "--install-dir", install, "--state-dir", old_state,
            "--approved-sha256", new_identity, "--verification-basis", "organization-digest")
        started = time.monotonic()
        selected = invoke(binary, "runtime", "activate", "--install-dir", install, "--runtime-sha256", new_identity,
            "--expected-active", old_identity)
        receipt["activation_elapsed_seconds"] = time.monotonic()-started
        if selected.get("active") != new_identity:
            raise RuntimeError("Clean break did not select the reviewed candidate")
        client.wait(timeout=10)
        receipt["connected_client_terminated"] = True
        if census is None:
            receipt["predecessor_processes_after"] = darwin_client_exit(client.pid, client.returncode, receipt["predecessor_client_image"])
        health = invoke(binary, "runtime", "ensure", "--install-dir", install)
        if (health.get("runtime_sha256") != new_identity or health.get("version") != receipt["version"]
                or health.get("instance") == old_health["instance"]):
            raise RuntimeError("Clean-break successor health differs from the reviewed new runtime")
        receipt["health"] = health
        old_jobs = ({item["indexed_job"]["id"] for item in originals}
            | {item["configured"]["completed"]["id"] for item in originals}
            | {receipt["discarded_pending_work"]["accepted"]["job"]["id"]})
        deadline = time.monotonic()+180
        receipt["rebuild_observations"] = []
        for _ in range(512):
            diagnosis = invoke(binary, "runtime", "diagnose", "--install-dir", install)
            receipt["rebuild_observations"].append(diagnosis)
            rebuild = validate_clean_break_rebuild(diagnosis, originals, old_namespace, old_jobs, new_identity, old_identity)
            if rebuild is not None and rebuild["stage"] == "ready" and rebuild["cleanup"]["complete"]:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError("Clean-break rebuild or owned cleanup did not complete; no request replay")
            time.sleep(.1)
        else:
            raise RuntimeError("Clean-break observation count exceeded its finite budget")
        control = Path(rebuild["successor_control"])
        # The original native canonical root has already been retired. Compare
        # its captured identity without resolving a now-nonexistent directory.
        if location_key(control) == location_key(old_control) or rebuild["successor_namespace"] == old_namespace:
            raise RuntimeError("Clean break reused the original control root or namespace")
        marker = json.loads((control / "airk-state-contract.json").read_text(encoding="utf-8"))
        validate_clean_break_baseline(marker, rebuild, control, install)
        receipt["state_contract"] = marker
        receipt["obsolete_cleanup"] = validate_clean_break_cleanup(rebuild, originals, old_control, new_identity, old_identity)
        if census is not None:
            receipt["predecessor_processes_after"] = census.verify_retired_gone(receipt["obsolete_cleanup"], timeout=10)
        if os.path.lexists(old_state) or os.path.lexists(old_control) or any(os.path.lexists(item["registration"]["data_root"]) for item in originals):
            raise RuntimeError("Clean-break completion left prior owned state or index artifacts")
        if sorted(path.name for path in (install / "runtimes").iterdir()) != [new_identity]:
            raise RuntimeError("Clean-break completion left a predecessor runtime payload")
        if any(os.path.lexists(install / name) for name in ("airk-launcher.exe", "airk-launcher", "launcher.json")):
            raise RuntimeError("Clean-break completion left a legacy launcher artifact")
        new_rpc = lambda method: rpc(binary, control, method)
        receipt["old_authority_reads"] = []
        for identifier in sorted(old_jobs):
            absent = rpc(binary, control, {"job": {"id": identifier}}, allow_failure=True)
            if absent.get("status") != "failure":
                raise RuntimeError("Successor still exposes a predecessor indexing job")
            receipt["old_authority_reads"].append(absent)
        request = receipt["discarded_pending_work"]["request"]
        old_request = rpc(binary, control, {"request_receipt": {
            "project": request["project"], "caller": request["caller"], "key": request["key"]}}, allow_failure=True)
        if (old_request.get("status") != "failure"
                and not (old_request.get("status") == "request_receipt"
                    and old_request.get("expired") is True and old_request.get("job") is None)):
            raise RuntimeError("Clean-break successor retained accepted predecessor request authority")
        receipt["old_authority_reads"].append(old_request)
        new_items, pages = catalog_page_items(new_rpc, "indexes")
        active = [row["value"] for row in new_items if row.get("kind") == "index" and row["value"].get("tombstone") is False]
        if {row["backing"]["binding"] for row in active} != {row["replacement"]["binding"] for row in rebuild["indexes"]} or len(active) != 2:
            raise RuntimeError("Fresh catalog does not contain exactly all replacement indexes")
        old_index_ids = {item["selection"]["index_id"] for item in originals if item["selection"] is not None}
        if any(item["id"] in old_index_ids for item in active):
            raise RuntimeError("Clean-break fresh catalog reused an old Index identity")
        grants, grant_pages = catalog_page_items(new_rpc, "grants")
        if grants:
            raise RuntimeError("Clean break unexpectedly carried old grants or enrolled new clients automatically")
        receipt["fresh_catalog_before_enrollment"] = {"index_pages": pages, "grant_pages": grant_pages}
        receipt["fresh_connections"] = []
        for row in rebuild["indexes"]:
            fixture = next(item for item in originals if item["registration"] == row["original"])
            watching = new_rpc({"watch": {"project": row["replacement"]["binding"]}})
            if watching.get("observation", {}).get("enabled") is not fixture["watch"]:
                raise RuntimeError("Clean break changed a saved watch preference")
            inspection = new_rpc({"inspection": {"project": row["replacement"]["binding"]}})
            snapshot = (inspection.get("observation") or {}).get("snapshot", {})
            if (inspection.get("status") != "inspection" or inspection.get("project") != row["replacement"]["binding"]
                    or snapshot.get("project") != row["replacement"]["binding"]
                    or snapshot.get("vectors_enabled") is not fixture["vectors"]):
                raise RuntimeError("Clean-break replacement did not preserve the supported vector opt-out")
            selection, reviewed = fresh_clean_break_selection(new_rpc, row["replacement"])
            connection_root = root / ("connection-"+row["replacement"]["binding"][:12])
            connection_root.mkdir()
            read = verify_https_reconnect(invoke, binary, install, control, connection_root, row["replacement"], selection,
                digest(old_configuration), fixture["source_text"].encode(), command=command)
            receipt["fresh_connections"].append({"original": row["original"], "replacement": row["replacement"],
                "watch": watching, "inspection": inspection, "selection": selection, "review": reviewed, "https": read})
        if census is None:
            receipt["predecessor_processes_after"] = darwin_client_exit(client.pid, client.returncode,
                receipt["predecessor_client_image"], receipt["obsolete_cleanup"])
        after_cleanup = new_rpc("health")
        if (after_cleanup.get("status") != "health" or after_cleanup.get("observation", {}).get("instance") != health["instance"]
                or after_cleanup["observation"].get("runtime_sha256") != new_identity):
            raise RuntimeError("Clean-break cleanup restarted or replaced the selected service")
        receipt["health_after_cleanup"] = after_cleanup
        completed = True
    except BaseException as error:
        primary_error = error
        receipt["primary_error"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        if census is not None:
            census.close()
        source_observation = observe_source_inputs(source_inputs)
        receipt["source_observation_before_teardown"] = source_observation
        preserved = source_observation["preserved"]
        receipt["source_preserved_before_teardown"] = preserved
        completed = completed and preserved is True and len(originals) == 2 and source_observation["count"] == 2
        def observe_owned_stop():
            if not candidate_staged and (install / "selected.json").is_file():
                return stop_predecessor_before_staging(command, old_binary, install, old_control, old_identity, digest(old_binary))
            inspected = invoke(binary, "runtime", "inspect", "--install-dir", install)
            if inspected.get("selection", {}).get("active") != new_identity:
                return {"candidate_stop_acknowledged": False, "reason": "reviewed candidate is not selected"}
            active_control = inspected.get("control_dir")
            observed = invoke(binary, "service", "rpc", "--runtime-dir", active_control, "--stdin", input=json.dumps("health"))
            if observed.get("status") != "health" or observed.get("observation", {}).get("runtime_sha256") != new_identity:
                return {"candidate_stop_acknowledged": False, "reason": "exact candidate owner unavailable"}
            return stop_owned_runtime(command, binary, install, Path(active_control), new_identity, digest(binary),
                observed_health=observed, owner="reviewed_candidate")
        try:
            teardown = finish_fixture(temporary, root, completed, lambda: invoke(binary, "runtime", "uninstall", "--install-dir", install),
                client, client_log, observe_owned_stop if not completed else None)
            receipt.update(teardown=teardown, passed=completed and teardown["teardown_complete"], source_preserved=preserved,
                source_scope_note="Only fictional fixture sources are removed after successful acknowledged teardown; failed fixtures stay preserved.")
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("x", encoding="utf-8") as output:
                output.write(json.dumps(receipt, indent=2)+"\n")
            print(json.dumps(receipt), flush=True)
            if primary_error is None and not receipt["passed"]:
                raise RuntimeError("Clean-break assertions did not produce complete owned teardown; fixture retained")
        except BaseException as error:
            if primary_error is None:
                raise
            print(f"Secondary clean-break diagnostic failure: {error!r}; retained {root}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--previous-runtime", type=Path, required=True)
    parser.add_argument("--interrupted-runtime", type=Path,
                        help="Stage a superseded candidate and seed its exact pre-probe intent in this fictional fixture")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check-inputs", action="store_true",
                        help="Verify accessible fixture runtime roots and complete bounded seals, write a receipt, and execute no runtime")
    parser.add_argument("--fixture-base", type=Path,
                        help="Existing owner-controlled fixture parent; overrides AIRK_PRIVATE_CENSUS_BASE without changing its ACLs")
    parser.add_argument("--source-state", choices=("indexed", "registered-unindexed"), default="indexed",
                        help="Indexed is a positive explicit old-fixture Index; registered-unindexed retains the original fail-closed recovery case")
    parser.add_argument("--fixture-setup", choices=("legacy", "reviewed"), default="legacy",
                        help="Legacy retains historical registration coverage; reviewed explicitly creates a current catalog Index, membership and grant")
    parser.add_argument("--semantic-old",action="store_true",help="Explicitly set up the actual old pinned MiniLM model/vectors, then prove retained vector/hybrid reads after upgrade")
    parser.add_argument("--clean-break", action="store_true",
                        help="Qualify a forced pre-1.8.16 reset with multiple rebuilt indexes and complete owned cleanup")
    args = parser.parse_args()
    if args.fixture_setup == "reviewed" and args.source_state != "indexed":
        parser.error("Reviewed setup is only supported for the positive indexed upgrade fixture")
    if args.semantic_old and args.source_state != "indexed":
        parser.error("Semantic predecessor qualification requires an explicitly indexed old fixture")
    if args.clean_break and (args.source_state != "indexed" or args.semantic_old or args.interrupted_runtime):
        parser.error("Clean-break qualification requires the indexed fixture without retained-semantic or interrupted-migration campaigns")
    if sys.platform not in ("win32","linux","darwin"):
        raise RuntimeError("Upgrade fixture supports Windows/Linux retirement or native Darwin graceful upgrade")
    if args.check_inputs:
        try:
            checked = check_upgrade_inputs(args.runtime, args.previous_runtime, args.interrupted_runtime, args.clean_break,
                                           fixture_parent(args.fixture_base) / "au-xxxxxxxx")
        except Exception as error:
            checked = {"passed": False, "check_inputs_only": True, "runtime_executed": False,
                       "failure": {"type": type(error).__name__, "message": str(error)}}
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("x", encoding="utf-8") as output:
                output.write(json.dumps(checked, indent=2)+"\n")
            raise
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with args.output.open("x", encoding="utf-8") as output:
            output.write(json.dumps(checked, indent=2)+"\n")
        print(json.dumps(checked), flush=True)
        return
    new = args.runtime.resolve(strict=True)
    old = args.previous_runtime.resolve(strict=True)
    binary, old_binary = runtime_binary(new),runtime_binary(old)
    boundary = clean_break_required(json.loads((old / "airk-install.json").read_text(encoding="utf-8"))["version"],
                                    json.loads((new / "airk-install.json").read_text(encoding="utf-8"))["version"])
    if args.clean_break and not boundary:
        parser.error("Clean-break fixture requires a verified pre-1.8.16 predecessor")
    if boundary:
        if args.source_state != "indexed" or args.semantic_old or args.interrupted_runtime:
            parser.error("Pre-1.8.16 resets require the clean-break fixture; retained-state assertions no longer apply")
        return run_clean_break(args, new, old)
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1")

    records = []
    root = None
    def command(executable, *arguments, input=None, timeout=180):
        return record_native_command(root, records, executable, arguments, input=input,
                                     timeout=timeout, flags=flags, environment=environment)
    def invoke(executable, *arguments, input=None, timeout=180):
        result = command(executable, *arguments, input=input, timeout=min(180, timeout))
        if result.returncode:
            raise RuntimeError(f"Native upgrade fixture failed: {result.stderr[-4096:]}")
        return json.loads(result.stdout)
    parent = fixture_parent(args.fixture_base)
    temporary = owned_temporary(parent)
    root = Path(temporary.name)
    environment.update(isolated_git_environment(root))
    install, state = root / "install", root / "state"
    source, data = root / "source", root / "data"
    source.mkdir()
    # Register validates existing roots; prepare-registration alone only plans
    # storage. Both directories belong exclusively to this synthetic fixture.
    data.mkdir()
    original = b"Original fictional source preserved through upgrade.\n"
    (source / "README.md").write_bytes(original)
    client = None
    client_log = None
    census = None
    completed = False
    candidate_stage_attempted = False
    old_execution_digest = digest(old_binary)
    primary_error = None
    receipt = {"passed": False, "source_state": args.source_state, "fixture_setup": args.fixture_setup, "fixture_root": str(root),
               "semantic_old_requested":args.semantic_old,
               "retirement_mode":"native_darwin_exact_image" if sys.platform == "darwin" else "native_retirement_with_held_process_observations",
               "qualification_scope": "explicit old-index positive upgrade" if args.source_state == "indexed"
                   else "registered never-indexed recovery observation; not positive upgrade qualification"}
    try:
        receipt["legacy_path_budget"] = check_fixture_path_budget(root, new, old, args.interrupted_runtime)
        old_identity, new_identity = digest(old / "airk-install.json"), digest(new / "airk-install.json")
        invoke(old_binary, "runtime", "stage", "--source", old, "--install-dir", install,
               "--state-dir", state, "--approved-sha256", old_identity,
               "--verification-basis", "organization-digest")
        invoke(old_binary, "runtime", "activate", "--install-dir", install, "--runtime-sha256", old_identity)
        control = state / "control"
        def old_rpc(method):
            response = invoke(old_binary, "service", "rpc", "--runtime-dir", control, "--stdin", input=json.dumps(method))
            if response.get("status") == "failure":
                raise RuntimeError("Predecessor reviewed fixture refused: " + json.dumps(response))
            return response
        selected_scope = None
        if args.fixture_setup == "reviewed":
            invoke(old_binary, "runtime", "ensure", "--install-dir", install)
            project, selected_scope = reviewed_fixture(old_rpc, source, data)
        else:
            project = invoke(old_binary, "--root", source, "--data-dir", data, "--collection", "upgrade",
                             "service", "prepare-registration")
            # Registration contains only the fictional paths created above.
            result = command(old_binary, "service", "rpc", "--runtime-dir", control, "--stdin",
                             input=json.dumps({"register": {"project": project}}))
            if result.returncode or json.loads(result.stdout).get("status") != "registered":
                raise RuntimeError(f"Old fixture registration failed: {result.stdout[-4096:]} {result.stderr[-4096:]}")
        semantic_before = None
        owner_scope = ["--service-runtime-dir",str(control),"--service-project",project["binding"],"--root",project["root"],"--data-dir",project["data_root"],"--collection",project["collection"]]
        if args.semantic_old:
            semantic_before = {"explicit_jobs":{},"queries":{},"attempts":[]}
            receipt["semantic_before"] = semantic_before
            accepted = invoke(old_binary,*owner_scope,"setup-model")
            semantic_before["explicit_jobs"]["model_setup"] = wait_semantic_job(old_rpc,accepted,project["binding"],"model_setup")
            accepted = invoke(old_binary,*owner_scope,"configure","--stdin",input=json.dumps({"operation":"vectors","enabled":True}))
            semantic_before["explicit_jobs"]["configure_vectors"] = wait_semantic_job(old_rpc,accepted,project["binding"],"configure")
        if args.source_state == "indexed":
            # This is an explicit action on the OLD fictional fixture, never a
            # migration assumption or inferred grant/configuration for missing state.
            accepted = invoke(old_binary, "--service-runtime-dir", control,
                              "--service-project", project["binding"], "--root", source,
                              "--data-dir", data, "--collection", "upgrade", "index")
            saved_job = accepted["job"]["id"]
            deadline = time.monotonic() + 180
            while True:
                observed = invoke(old_binary, "service", "rpc", "--runtime-dir", control,
                                  "--stdin", input=json.dumps({"job": {"id": saved_job}}))["job"]
                if observed["id"] != saved_job:
                    raise RuntimeError("Old Index receipt changed identity")
                if observed["state"] in ("succeeded", "failed", "canceled"):
                    if observed["state"] != "succeeded":
                        raise RuntimeError("Explicit old fixture Index failed: " + json.dumps(observed))
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError("Explicit old fixture Index deadline exhausted; no resubmission")
                time.sleep(0.05)
            receipt["explicit_old_index_job"] = observed
        if args.semantic_old:
            semantic_before["explicit_jobs"]["index"] = {"accepted":accepted,"completed":observed}
            epoch = old_rpc("request_epoch")
            accepted = old_rpc({"enqueue":{"project":project["binding"],"kind":"vectors","caller":uuid.uuid4().hex,
                "key":{"namespace":epoch["namespace"],"epoch":epoch["epoch"],"nonce":uuid.uuid4().hex}}})
            semantic_before["explicit_jobs"]["vectors"] = wait_semantic_job(old_rpc,accepted,project["binding"],"vectors")
            def save_semantic_before():
                (root / "semantic-before.json").write_text(json.dumps(semantic_before,indent=2)+"\n",encoding="utf-8")
            def old_semantic_read(tool,arguments):
                scope = owner_scope
                if selected_scope is not None:
                    selected = selected_scope.copy()
                    selected["expected_catalog_revision"] = old_rpc({"catalog_host_context":{"key":selected["context_key"],"expected_catalog_revision":None}})["descriptor"]["catalog_revision"]
                    scope = ["--service-runtime-dir",str(control),"--catalog-session",json.dumps(selected)]
                def read(until):
                    result = command(old_binary,*scope,"tools","call","airk_"+tool,"--args",json.dumps(arguments),timeout=max(.001,until-time.monotonic()))
                    if result.returncode not in (0,1):
                        raise RuntimeError("Old semantic CLI transport failed")
                    return json.loads(result.stdout)
                def retain_attempt(value):
                    semantic_before["attempts"].append({"tool":tool,"arguments":arguments,"result":value}); save_semantic_before()
                return typed_https_read(read,retain_attempt,time.monotonic()+90)
            deadline = time.monotonic()+180
            while True:
                status_reply = old_semantic_read("status",{"response_mode":"full"})
                status = semantic_body(status_reply)
                if status.get("semantic_readiness",{}).get("state") == "ready":
                    break
                if status.get("semantic_readiness",{}).get("state") in ("disabled","failed") or time.monotonic() >= deadline:
                    raise RuntimeError("Old actual semantic pipeline did not reach ready")
                time.sleep(.1)
            semantic_before["status"] = status_reply
            semantic_before["model"] = retained_model(control)
            semantic_before["old_provider"] = old_rpc("embedding_status")
            if semantic_before["old_provider"].get("available") is not True or type(semantic_before["old_provider"].get("process_id")) is not int:
                raise RuntimeError("Old real semantic helper was not observed")
            for mode in ("vector","hybrid"):
                arguments = {"query":"Retain original source material through a software update","mode":mode,"limit":3,"max_bytes":8192,"response_mode":"full","verification":"off"}
                envelope = old_semantic_read("search",arguments)
                body = semantic_body(envelope)
                assert_semantic_query(body,mode,status["publication"],semantic_before["model"]["reference"]["pipeline_digest"],original)
                semantic_before["queries"][mode] = {"arguments":arguments,"envelope":envelope,"body":body}
                save_semantic_before()
            semantic_before["identity"] = semantic_identity(status,semantic_before["model"],
                {mode:query["body"] for mode,query in semantic_before["queries"].items()})
            semantic_before["jobs"] = semantic_jobs(control / "service-1.4.sqlite3",project["binding"])
            save_semantic_before()
        if selected_scope is not None:
            original_catalog, _ = reviewed_snapshot(old_rpc, selected_scope)
            receipt["reviewed_scope_before"] = original_catalog
        database = control / "service-1.4.sqlite3"
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
            old_format = connection.execute("PRAGMA user_version").fetchone()[0]
            if selected_scope is not None:
                receipt["saved_contract_before"] = connection.execute(
                    "SELECT authority_contract,policy_contract FROM index_catalog_meta WHERE id=1").fetchone()
        launcher = install / ("airk-launcher.exe" if os.name == "nt" else "airk-launcher")
        receipt["predecessor_client_image"] = capture_client_image(launcher)
        client_log_path = root / "client.stderr"
        client_log = client_log_path.open("wb")
        client_scope = ["--catalog-session", json.dumps(selected_scope)] if selected_scope is not None else [
            "--service-project", project["binding"], "--root", project["root"], "--data-dir", project["data_root"], "--collection", project["collection"]]
        old_client_command = [str(launcher), "runtime", "launch", "--install-dir", str(install), "--", *client_scope, "mcp"]
        # This is the exact fictional predecessor command actually launched below.
        # Its bytes identify configuration provenance; native saved authority still controls enrollment.
        old_configuration = root / "old-client-configuration.json"
        with old_configuration.open("x", encoding="utf-8") as output:
            json.dump({"command": old_client_command[0], "args": old_client_command[1:]}, output)
        client = subprocess.Popen(old_client_command,
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=client_log,
                                  text=True, encoding="utf-8", creationflags=flags, env=environment)
        replies = queue.Queue()
        def receive_lines():
            try:
                for line in client.stdout:
                    replies.put(line)
            finally:
                replies.put(None)
        threading.Thread(target=receive_lines, daemon=True).start()
        client.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "old-upgrade-fixture", "version": "1"}}}) + "\n")
        client.stdin.flush()
        try:
            line = replies.get(timeout=60)
        except queue.Empty as error:
            raise RuntimeError(f"Old MCP initialize timed out (exit={client.poll()}): "
                               f"{client_log_path.read_bytes()[-4096:].decode('utf-8', errors='replace')}") from error
        if line is None:
            raise RuntimeError(f"Old MCP client closed before initialize: "
                               f"{client_log_path.read_bytes()[-4096:].decode('utf-8', errors='replace')}")
        initialized = json.loads(line)
        if initialized.get("id") != 1 or "error" in initialized:
            raise RuntimeError("Old connected MCP client did not initialize")
        client.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        client.stdin.flush()
        specification = importlib.util.spec_from_file_location("upgrade_owned_processes", Path(__file__).with_name("owned-runtime-processes.py"))
        processes = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(processes)
        old_health = old_rpc("health")["observation"]
        if (type(old_health.get("process_id")) is not int or old_health["process_id"] <= 0
                or old_health.get("runtime_sha256") != old_identity):
            raise RuntimeError("Predecessor process observation is not bound to the owned approval")
        required_pids = {client.pid,old_health["process_id"]}
        if semantic_before is not None:
            required_pids.add(semantic_before["old_provider"]["process_id"])
        if sys.platform == "darwin":
            receipt["predecessor_processes_before"] = {"schema":1,"owned_client_pid":client.pid,"native_health":old_health,
                "old_provider":semantic_before.get("old_provider") if semantic_before else None,
                "held_kernel_census":False,"qualification_scope":"Native Darwin activation/graceful client completion; no Windows/Linux retirement census or all-process claim."}
        else:
            census = processes.capture(install.resolve(strict=True),old_identity,required_pids)
            receipt["predecessor_processes_before"] = census.before
        (root / "predecessor-processes-before.json").write_text(json.dumps(receipt["predecessor_processes_before"],indent=2)+"\n",encoding="utf-8")
        candidate_stage_attempted = True  # Before any new-version installation effects.
        invoke(binary, "runtime", "stage", "--source", new, "--install-dir", install,
               "--state-dir", state, "--approved-sha256", new_identity,
               "--verification-basis", "organization-digest")
        started = time.monotonic()
        if args.interrupted_runtime:
            interrupted = args.interrupted_runtime.resolve(strict=True)
            interrupted_identity = digest(interrupted / "airk-install.json")
            invoke(binary, "runtime", "stage", "--source", interrupted, "--install-dir", install,
                   "--state-dir", state, "--approved-sha256", interrupted_identity,
                   "--verification-basis", "organization-digest")
            diagnosis = invoke(binary, "runtime", "diagnose", "--install-dir", install)
            # Seed only synthetic fixture metadata, never copied production state.
            intent = {"schema": 1, "install": diagnosis["state_layout"]["layout"]["install"],
                      "control": diagnosis["state_layout"]["control"], "candidate": interrupted_identity,
                      "prior": {"schema": 1, "active": old_identity, "previous": None},
                      "manager": False, "phase": "drain"}
            (install / "activation.json").write_text(json.dumps(intent), encoding="utf-8")
            invoke(binary, "runtime", "abort-activation", "--install-dir", install,
                   "--expected-candidate", interrupted_identity, "--expected-active", old_identity)
            if (install / "activation.json").exists():
                raise RuntimeError("Superseded pre-probe intent survived acknowledged abort")
        selected = invoke(binary, "runtime", "activate", "--install-dir", install,
                          "--runtime-sha256", new_identity, "--expected-active", old_identity)
        elapsed = time.monotonic() - started
        if args.source_state == "registered-unindexed":
            raise RuntimeError("Registered-unindexed activation unexpectedly succeeded; inspect the changed recovery contract")
        if selected.get("active") != new_identity:
            raise RuntimeError("Upgrade selected the wrong payload")
        client.wait(timeout=10)
        if census is not None:
            receipt["predecessor_processes_after"] = census.verify_gone(timeout=10)
        else:
            receipt["predecessor_processes_after"] = darwin_client_exit(
                client.pid, client.returncode, receipt["predecessor_client_image"])
        (root / "predecessor-processes-after.json").write_text(json.dumps(receipt["predecessor_processes_after"],indent=2)+"\n",encoding="utf-8")
        health = invoke(binary, "runtime", "ensure", "--install-dir", install)
        expected_version = json.loads((new / "airk-install.json").read_text(encoding="utf-8"))["version"]
        if (health.get("version") != expected_version or health.get("runtime_sha256") != new_identity
                or health.get("instance") == old_health["instance"]):
            raise RuntimeError("New service identity mismatch")
        if selected_scope is not None:
            def new_rpc(method):
                response = invoke(binary, "service", "rpc", "--runtime-dir", control, "--stdin", input=json.dumps(method))
                if response.get("status") == "failure":
                    raise RuntimeError("Upgraded reviewed fixture refused: " + json.dumps(response))
                return response
            current_catalog, capabilities = reviewed_snapshot(new_rpc, selected_scope)
            if current_catalog != original_catalog:
                raise RuntimeError("Upgrade changed the complete reviewed Index, Project, membership or grant snapshot")
            if not isinstance(capabilities, dict) or capabilities.get("policy_contract", 0) < 2:
                raise RuntimeError("Upgraded native host context did not report split capability")
            receipt.update(reviewed_scope_preserved=True, native_host_capabilities=capabilities,
                           reviewed_scope_after=current_catalog)
            split = new_rpc({"catalog_split_source_state": {"index_id": selected_scope["index_id"],
                "expected_catalog_revision": current_catalog["revision"]}})
            if (split.get("status") != "catalog_split_source_state"
                    or split["state"]["index"]["id"] != selected_scope["index_id"]
                    or split["capabilities"].get("policy_contract", 0) < 2):
                raise RuntimeError("Upgraded existing Index did not support the canonical split baseline read")
            receipt["existing_index_split_baseline"] = split
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
            version = connection.execute("PRAGMA user_version").fetchone()[0]
            saved = connection.execute("SELECT payload,registered FROM projects WHERE binding=?", (project["binding"],)).fetchone()
            if saved is None or len(saved[0]) > 65536 or saved[1] != 1 or json.loads(saved[0]) != project:
                raise RuntimeError("Upgrade did not preserve the complete original project registration")
            if selected_scope is not None:
                receipt["saved_contract_after"] = connection.execute(
                    "SELECT authority_contract,policy_contract FROM index_catalog_meta WHERE id=1").fetchone()
                if receipt["saved_contract_after"] != receipt["saved_contract_before"]:
                    raise RuntimeError("Capability discovery unexpectedly changed the existing source-policy contract")
        runtime_schema = json.loads((new / "airk-runtime.json").read_text(encoding="utf-8"))["schema"]
        verify_registry_version(version, old_format, runtime_schema)
        if (source / "README.md").read_bytes() != original:
            raise RuntimeError("Upgrade changed the fictional source bytes")
        if runtime_schema == 2:
            receipt["https_reconnect"] = verify_https_reconnect(invoke, binary, install, control, root,
                project, selected_scope, digest(old_configuration), original,semantic_before,command)
            receipt["obsolete_cleanup"] = retire_obsolete_runtime(
                invoke, binary, install, new_identity, old_identity)
            if census is None:
                receipt["predecessor_processes_after"] = darwin_client_exit(
                    client.pid, client.returncode, receipt["predecessor_client_image"], receipt["obsolete_cleanup"])
                (root / "predecessor-processes-after.json").write_text(json.dumps(receipt["predecessor_processes_after"], indent=2) + "\n", encoding="utf-8")
            after_cleanup = invoke(binary, "service", "rpc", "--runtime-dir", control,
                                   "--stdin", input=json.dumps("health"))
            if (after_cleanup.get("status") != "health"
                    or after_cleanup.get("observation", {}).get("instance") != health["instance"]
                    or after_cleanup["observation"].get("runtime_sha256") != new_identity):
                raise RuntimeError("Obsolete cleanup restarted or replaced the selected service")
            receipt["health_after_cleanup"] = after_cleanup
        if census is None and receipt["predecessor_processes_after"].get("qualified") is not True:
            raise RuntimeError("Darwin predecessor exit remains pending exact native retirement proof")
        receipt.update({"passed": True, "previous_version": json.loads((old / "airk-install.json").read_text(encoding="utf-8"))["version"],
                   "version": expected_version, "previous_format": old_format, "format": version,
                   "activation_elapsed_seconds": elapsed, "connected_client_terminated": True,
                   "registration_preserved": True, "source_preserved": True,
                   "superseded_preprobe_intent": bool(args.interrupted_runtime),
                   "candidate": new_identity, "previous": old_identity})
        completed = True
    except BaseException as error:
        primary_error = error
        receipt["primary_error"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        if census is not None:
            census.close()  # Release read-only handles only; never signal here.
        def observe_owned_candidate():
            if not candidate_stage_attempted and not (install / "activation.json").exists() and locals_old_identity is not None:
                return stop_predecessor_before_staging(command,old_binary,install,state / "control",
                    locals_old_identity,old_execution_digest)
            # Observe health, then explicitly mutate ONLY this owned candidate to stop;
            # neither successful health nor an absent listener creates authority.
            result = command(binary, "service", "rpc", "--runtime-dir", state / "control",
                             "--stdin", input=json.dumps("health"), timeout=15)
            observation = {"health_exit_code": result.returncode, "stopped": None,
                           "scope": "unknown unless matching candidate acknowledged shutdown; no PID inference"}
            if result.returncode == 0:
                health = json.loads(result.stdout)
                observation["health"] = health
                if isinstance(locals_new_identity, str) and len(locals_new_identity) == 64 and health.get("status") == "health" and health.get("observation", {}).get("runtime_sha256") == locals_new_identity:
                    stopped = command(binary, "service", "rpc", "--runtime-dir", state / "control",
                                      "--stdin", input=json.dumps("shutdown"), timeout=15)
                    observation["shutdown_exit_code"] = stopped.returncode
                    if stopped.returncode == 0:
                        observation["shutdown_reply"] = json.loads(stopped.stdout)
                        observation["shutdown_acknowledged"] = observation["shutdown_reply"].get("status") == "stopping"
                        observation["process_exit_confirmed"] = False  # Ack is not an exit proof.
            return observation
        locals_new_identity = locals().get("new_identity")
        locals_old_identity = locals().get("old_identity")
        try:
            receipt["source_preserved_before_teardown"] = (source / "README.md").read_bytes() == original
            receipt["pending_manual_recovery"] = (install / "activation.json").exists()
            if "database" in locals() and "project" in locals():
                try:
                    with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as sql:
                        row = sql.execute("SELECT payload,registered FROM projects WHERE binding=?", (project["binding"],)).fetchone()
                        if row is None or len(row[0]) > 65536:
                            raise RuntimeError("Saved registration absent or exceeds 64 KiB")
                        receipt["registration_unchanged"] = json.loads(row[0]) == project and row[1] == 1
                        receipt["observed_registry_schema"] = sql.execute("PRAGMA user_version").fetchone()[0]
                        if args.source_state == "registered-unindexed":
                            receipt["observed_imported_grants"] = sql.execute("SELECT count(*) FROM index_access_grants WHERE import_legacy_binding=?", (project["binding"],)).fetchone()[0]
                except Exception as error:
                    receipt["registration_or_grant_observation_unavailable"] = repr(error)
        except Exception as error:
            receipt["source_observation_unavailable"] = repr(error)
        # Final exact observations, not mere row count or missing-as-success,
        # gate the positive receipt and whether teardown may remove evidence.
        if completed and (receipt.get("registration_unchanged") is not True
                          or receipt.get("source_preserved_before_teardown") is not True):
            receipt["preservation_error"] = "Exact final registration/source preservation unavailable or changed"
            completed = False
        try:
            teardown = finish_fixture(temporary, root, completed,
                lambda: invoke(binary, "runtime", "uninstall", "--install-dir", install),
                client, client_log, observe_owned_candidate if not completed else None)
            receipt["teardown"] = teardown
            receipt["passed"] = completed and teardown["teardown_complete"]
            receipt["source_preserved"] = receipt.get("source_preserved_before_teardown")
            receipt["source_scope_note"] = "Successful teardown removed only the verified fictional fixture; failures preserve source and native pending state."
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("x", encoding="utf-8") as output:
                output.write(json.dumps(receipt, indent=2) + "\n")
            print(json.dumps(receipt), flush=True)
            if primary_error is None and not receipt["passed"]:
                raise RuntimeError("Upgrade assertions completed but owned teardown remains incomplete; retained fixture")
        except BaseException as diagnostic_error:
            if primary_error is None:
                raise
            # The detached fixture remains even if output persistence is broken.
            # Never replace the native refusal with a cleanup/logging exception.
            try:
                print(f"Secondary upgrade fixture diagnostic/teardown error: {diagnostic_error!r}; retained {root}", file=sys.stderr)
            except Exception:
                pass




if __name__ == "__main__":
    main()
