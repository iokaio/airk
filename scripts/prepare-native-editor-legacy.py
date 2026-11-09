# SPDX-License-Identifier: Apache-2.0
"""Prepare a stopped, authentic legacy-to-schema-2 installation for editor tests.

This is a T2 native journey, not a unit test. Preserve every failed fixture.
The owner must be a fresh path beneath artifacts in the contributor's protected
fixture base. No old executable is retained in the upgraded installation.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import stat
from pathlib import Path
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(owner, name, value):
    path = owner / name
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2)
        output.write("\n")
    return {"path": str(path), "sha256": sha(path)}


def verify_payload(root, expected):
    def plain(path):
        for component in reversed((path.absolute(), *path.absolute().parents)):
            metadata = component.lstat()
            assert not stat.S_ISLNK(metadata.st_mode) and not getattr(metadata, "st_file_attributes", 0) & 0x400, "Redirected payload path"
            assert stat.S_ISDIR(metadata.st_mode) or stat.S_ISREG(metadata.st_mode), "Unsupported payload object"
    plain(root)
    root = root.resolve(strict=True)
    assert sha(root / "airk-install.json") == expected, "Reviewed runtime identity changed"
    manifest = json.loads((root / "airk-install.json").read_text())
    assert 0 < len(manifest["files"]) <= 4096
    observed = set()
    for path in root.rglob("*"):
        plain(path)
        if path.is_file():
            observed.add(path.relative_to(root).as_posix())
    assert observed == set(manifest["files"]) | {"airk-install.json"}, "Unsealed payload files"
    for name, digest in manifest["files"].items():
        path = root / name
        assert path.resolve(strict=True).is_relative_to(root) and not path.is_symlink()
        assert sha(path) == digest, name
    return root, manifest


def verify_stop_response(request, response):
    expected = {key: value for key, value in request.items() if key != "force"}
    expected["manager"] = False
    assert response["status"] == "lifecycle" and response["record"]["phase"] == "complete"
    assert response["record"]["request"] == expected, "Crossed or stale lifecycle receipt"


def require_imported_predecessor(compatibility):
    """This journey requires raw registration to become sealed imported authority."""
    assert compatibility["topology"] == "paired" and compatibility["authority"] == "legacy", \
        "Imported legacy journey requires a verified managed raw-registration predecessor; reviewed catalog migration is a separate case"
    assert compatibility["original_scope"] == "service_project"
    assert compatibility.get("migration_path") == "audited_imported_legacy", \
        "Managed raw predecessor has no verified compatible catalog migration path"


def original_connection(fixture, name, install):
    require_imported_predecessor(fixture.compatibility)
    registration = fixture.projects[name].copy()
    # Preserve the installed extension's pinned scope and the original launcher's
    # forwarding protocol (model.ts launchArgs), independently of CLI admin args.
    arguments = ["runtime", "launch", "--install-dir", str(install), "--",
                 "--root", registration["root"], "--data-dir", registration["data_root"],
                 "--collection", registration["collection"], "--service-project", registration["binding"], "mcp"]
    return {"args": arguments, "scope": registration,
            "migration_provenance": "imported_legacy_snapshot", "retrieval_authority": "sealed_imported_snapshot"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--previous-runtime", type=Path, required=True)
    parser.add_argument("--expected-runtime-sha256", required=True)
    parser.add_argument("--expected-previous-sha256", required=True)
    parser.add_argument("--owner", type=Path, required=True)
    args = parser.parse_args()
    assert sys.platform == "win32", "This retained editor fixture recipe is Windows-qualified only"
    new, _ = verify_payload(args.runtime, args.expected_runtime_sha256)
    old, _ = verify_payload(args.previous_runtime, args.expected_previous_sha256)
    new_metadata = json.loads((new / "airk-runtime.json").read_text())
    old_metadata = json.loads((old / "airk-runtime.json").read_bytes().decode("utf-8"))
    assert new_metadata["schema"] == 2 and old_metadata["schema"] == 1
    clients = load("editor_legacy_clients", "check-service-clients.py")
    # Fail before creating the fixture or starting either installed service.
    require_imported_predecessor(clients.runtime_compatibility(old))
    owner = args.owner.absolute()
    assert owner.parent.resolve(strict=True).is_relative_to((ROOT / "artifacts").resolve(strict=True))
    assert not owner.exists(), "Fresh fixture required; preserve existing attempts"
    owner.mkdir()
    owner = owner.resolve(strict=True)
    upgrade = load("editor_legacy_upgrade", "check-runtime-upgrade.py")
    replay = load("editor_legacy_replay", "benchmark-mcp-investigation.py")
    install, state, source, data = (owner / name for name in ("install", "state", "source", "data"))
    source.mkdir(); data.mkdir()
    (source / "fixture.ts").write_text("export function LegacyOriginalEvidence() { return 42; }\n", encoding="utf-8")
    original = {"fixture.ts": sha(source / "fixture.ts")}
    fixture = clients.Fixture(old, owner)
    fixture.control = state / "control"
    records = []
    candidate_attempted = False
    completed = False
    primary = None
    report = None

    def invoke(runtime, *arguments, input=None, timeout=180):
        result = upgrade.record_native_command(owner, records, runtime / "airk.exe", arguments,
            input=input, timeout=timeout, flags=getattr(__import__("subprocess"), "CREATE_NO_WINDOW", 0), environment=fixture.environment)
        assert result.returncode == 0, result.stderr[-4096:]
        return json.loads(result.stdout)

    def stop_selected():
        inspection = invoke(new if candidate_attempted else old, "runtime", "inspect", "--install-dir", install)
        identity = inspection["selection"]["active"]
        assert identity in (args.expected_previous_sha256, args.expected_runtime_sha256)
        runtime = new if identity == args.expected_runtime_sha256 else old
        fixture.binary = runtime / "airk.exe"
        health = fixture.rpc("health")["observation"]
        assert health["runtime_sha256"] == identity
        assert health["executable_sha256"] == sha(runtime / "airk.exe")
        epoch = fixture.rpc("request_epoch")
        request = {"caller": uuid.uuid4().hex, "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                   "action": "stop", "expected_instance": health["instance"], "runtime": identity, "force": False}
        value = invoke(runtime, "runtime", "lifecycle", "--install-dir", install, input=json.dumps(request))
        write(owner, "producer-native-stop.json", {"before_health": health, "request": request, "response": value})
        verify_stop_response(request, value)
        after = upgrade.record_native_command(owner, records, runtime / "airk.exe",
            ("service", "health", "--runtime-dir", fixture.control), timeout=15,
            flags=getattr(__import__("subprocess"), "CREATE_NO_WINDOW", 0), environment=fixture.environment)
        write(owner, "producer-post-stop-health.json", {"exit_code": after.returncode, "stdout": after.stdout, "stderr": after.stderr})
        assert after.returncode != 0, "Owned service remains healthy after Stop"

    try:
        invoke(old, "runtime", "stage", "--source", old, "--install-dir", install, "--state-dir", state,
               "--approved-sha256", args.expected_previous_sha256, "--verification-basis", "organization-digest")
        invoke(old, "runtime", "activate", "--install-dir", install, "--runtime-sha256", args.expected_previous_sha256)
        fixture.register("legacy", source, data)
        indexed = fixture.job("legacy", "index")
        write(owner, "explicit-predecessor-index.json", indexed)
        fixture.rpc({"pause": {"project": fixture.projects["legacy"]["binding"], "paused": True}})
        inspection = invoke(old, "runtime", "inspect", "--install-dir", install)
        command = Path(inspection["launcher"])
        launcher_receipt = install / "launcher.json"
        snapshot = {"schema": 1, "manifest_text": (old / "airk-install.json").read_bytes().decode("utf-8"),
                    "runtime_text": (old / "airk-runtime.json").read_bytes().decode("utf-8"), "native_inspect": inspection,
                    "command_sha256": sha(command), "launcher_receipt_text": launcher_receipt.read_bytes().decode("utf-8")}
        assert json.loads(snapshot["launcher_receipt_text"])["sha256"] == snapshot["command_sha256"]
        predecessor_snapshot = write(owner, "predecessor-provenance.json", snapshot)
        connection = original_connection(fixture, "legacy", install)
        arguments = connection["args"]
        entry = {"command": str(command), "args": arguments, "env": {"AIRK_CLIENT": "vscode"}}
        configuration = write(owner, "original-mcp.json", {"servers": {"legacy-original": entry}})
        with replay.Session({**entry, "cwd": str(source), "expected_version": old_metadata["version"]}, 90) as session:
            envelope, metadata = session.tool(("symbols", {"name": "LegacyOriginalEvidence", "compact": False, "response_mode": "full"}))
            body = clients.tool_value(envelope)
            write(owner, "original-manual-before-upgrade.json", {"envelope": envelope, "body": body, "metadata": metadata})
            assert not envelope.get("isError")
            assert len(body["items"]) == 1 and body["items"][0]["citation"]["content_hash"] == original["fixture.ts"]
        candidate_attempted = True
        invoke(new, "runtime", "stage", "--source", new, "--install-dir", install, "--state-dir", state,
               "--approved-sha256", args.expected_runtime_sha256, "--verification-basis", "organization-digest")
        invoke(new, "runtime", "activate", "--install-dir", install, "--runtime-sha256", args.expected_runtime_sha256,
               "--expected-active", args.expected_previous_sha256)
        fixture.binary = new / "airk.exe"
        health = fixture.rpc("health")
        inspection = invoke(new, "runtime", "inspect", "--install-dir", install)
        activation = json.loads((install / "activation-result.json").read_text())
        upgrade_receipt = json.loads((fixture.control / "index-catalog-upgrade.json").read_text())
        retirement = upgrade.retire_obsolete_runtime(invoke, new, install,
            args.expected_runtime_sha256, args.expected_previous_sha256)
        evidence = [write(owner, name + ".json", value) for name, value in
                    [("activation", activation), ("inspect", inspection), ("post-health", health), ("upgrade-receipt", upgrade_receipt), ("obsolete-cleanup", retirement)]]
        assert sha(Path(configuration["path"])) == configuration["sha256"]
        assert original == {path.relative_to(source).as_posix(): sha(path) for path in source.rglob("*") if path.is_file()}
        report = {"schema": 1, "fixture_kind": "managed_native_catalog_upgrade", "fixture_root": str(owner),
            "install_dir": str(install), "control_dir": str(fixture.control), "source_root": str(source), "data_root": str(data),
            "candidate": {"install_manifest_sha256": args.expected_runtime_sha256, "runtime_dir": str(install / "runtimes" / args.expected_runtime_sha256), "engine_sha256": new_metadata["airk_sha256"]},
            "predecessor": {"install_manifest_sha256": args.expected_previous_sha256, "snapshot": predecessor_snapshot},
            "original_connection": connection,
            "legacy_config": {"kind": "vscode_servers", "binding_kind": "retired_installation_launcher", "path": configuration["path"], "sha256": configuration["sha256"],
                "entry_key": "legacy-original", **entry, "cwd": str(source)},
            "source_hashes_before": original, "source_hashes_after": original,
            "probe": {"symbol": "LegacyOriginalEvidence", "path": "fixture.ts", "content_sha256": original["fixture.ts"]},
            "native": {"upgrade_receipt": upgrade_receipt, "activation_receipt": activation, "authority_v1_readiness": upgrade_receipt["authority_v1"],
                "post_health": health, "inspect": inspection, "obsolete_cleanup": retirement, "evidence_files": evidence}}
        completed = True
    except BaseException as error:
        primary = error
        write(owner, "producer-failure.json", {"type": type(error).__name__, "error": str(error)[:4096]})
        raise
    finally:
        try:
            if install.exists():
                stop_selected()
        except BaseException as cleanup:
            write(owner, "producer-cleanup-failure.json", {"type": type(cleanup).__name__, "error": str(cleanup)[:4096]})
            if primary is None:
                raise
            primary.add_note("Exact owned Stop incomplete; preserve fixture: " + str(owner))
        finally:
            fixture.log.close()
    assert completed
    output = write(owner, "fixture.json", report)
    print(json.dumps({"fixture": output, "stopped": True, "retained_for_editor": True}))


if __name__ == "__main__":
    main()
