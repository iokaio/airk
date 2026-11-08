# SPDX-License-Identifier: Apache-2.0
"""Expect a retained-object integrity failure after native adoption preparation.

Uses authenticated historical fixture copies, never executes their helpers, and
keeps the deliberately damaged copy/pending native journal for diagnosis.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sqlite3
import stat
import uuid


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


saved = module("negative_adoption_saved", "check-standalone-adoption.py")
clients = module("negative_adoption_clients", "check-service-clients.py")


def corrupt_copied_object(data, layout, collection):
    if layout != "collections":
        raise ValueError("Negative campaign requires the historical generation format")
    data = Path(data).resolve(strict=True)
    before = saved.inventory(data)
    prefix = f"{layout}/{collection}/objects/artifacts/"
    candidates = [name for name in sorted(before) if name.startswith(prefix)
                  and len(name[len(prefix):]) == 64
                  and all(char in "0123456789abcdef" for char in name[len(prefix):])]
    assert candidates, "No authenticated retained object in copied fixture"
    name = candidates[0]
    path = data / name
    info = path.lstat()
    assert stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and not path.is_symlink()
    assert 0 < info.st_size <= 16 * 1024 * 1024
    assert before[name]["sha256"] == path.name, "Copied object is already damaged"
    with path.open("r+b") as stream:
        original = stream.read(1)
        stream.seek(0)
        stream.write(bytes([original[0] ^ 1]))
        stream.flush()
        os.fsync(stream.fileno())
    after = saved.inventory(data)
    expected = dict(before)
    expected[name] = after[name]
    assert after == expected and after[name]["sha256"] != before[name]["sha256"]
    assert after[name]["bytes"] == before[name]["bytes"]
    return {"path": name, "before": before[name], "after": after[name],
            "copied_inventory_before": before, "copied_inventory_after": after}


def validate_restoration(operation, original, current, journal, receipt):
    assert journal["phase"] == "restored" and journal["operation"] == operation
    assert journal["original_running"] is True and journal["original_explicit_user_stop"] is False
    assert journal["original_health"]["instance"] == original["instance"]
    restoration = journal["restoration"]
    assert restoration["operation"] == operation and restoration["restarted"] is True
    assert restoration["originally_explicitly_stopped"] is False
    assert current["instance"] == restoration["service_instance"] != original["instance"]
    assert current["runtime_sha256"] == original["runtime_sha256"]
    assert current["executable_sha256"] == original["executable_sha256"]
    assert current["policy"]["accepting"] is True
    assert journal["restoration_state_sha256"] is not None
    assert len(journal["adoption_plans"]) == 1
    assert receipt["stage"] == "prepared" and receipt["completion"] is None
    assert receipt["request"] == journal["adoption_plans"][0]["setup"]
    assert receipt["legacy_adoption"] == receipt["preparation"]["legacy_adoption"] is not None


def framed_digest(domain, value):
    digest = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in (domain, value):
        digest.update(len(field).to_bytes(8, "big"))
        digest.update(field)
    return digest.hexdigest()


def read_verified_snapshot(path, expected):
    path = Path(path)
    for component in (path, *path.parents):
        info = component.lstat()
        assert not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400
    info = path.stat()
    assert stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= 512 * 1024
    with path.open("rb") as stream:
        raw = stream.read(512 * 1024 + 1)
    assert len(raw) <= 512 * 1024
    assert framed_digest(b"airk/checkpoint-native-state/1", raw) == expected
    snapshot = json.loads(raw)
    assert snapshot["schema"] == 1 and snapshot["complete"] is True
    return snapshot


def validate_snapshot(before, after, receipt, raw_receipt):
    assert json.loads(raw_receipt) == receipt
    assert before["namespace"] == after["namespace"] == receipt["request"]["key"]["namespace"]
    for key in ("jobs", "requests", "retained_intents", "retained_intent_contract", "retained_intent_disposition"):
        assert before[key] == after[key], key
    assert before["unfinished"] == [], "Fresh negative campaign unexpectedly contained prior work"
    expected = {"owner": "index_operation_receipts", "id": receipt["operation_id"], "kind": "catalog_index_setup",
                "state": "prepared", "original_engine": None,
                "payload_digest": framed_digest(b"airk/checkpoint-operation/1", raw_receipt)}
    assert after["unfinished"] == [expected]
    binding = receipt["request"]["registration"]["binding"]
    publication = [value for value in after["publications"] if value["binding"] == binding]
    assert len(publication) == 1 and publication[0]["publication"] == {"state": "not_initialized"}
    remaining = [value for value in after["publications"] if value["binding"] != binding]
    assert remaining == before["publications"], "Another publication changed"


def qualify(runtime, historical, owner, report):
    original_data = Path(historical["data_root"])
    original_source = Path(historical["source"])
    data_before, source_before = saved.inventory(original_data), saved.inventory(original_source)
    assert data_before == saved.expected_inventory(historical.get("saved_data_after_content_reads", historical["saved_data_inventory"]))
    assert source_before == saved.expected_inventory(historical["source_after"])
    helper = Path(historical["helper"]["path"])
    archive = historical["saved_index_archive"]
    assert saved.sha(helper) == historical["helper"]["sha256"] and saved.sha(archive["path"]) == archive["sha256"]
    data = owner / "damaged-copy"
    shutil.copytree(original_data, data, copy_function=shutil.copy2)
    assert saved.inventory(data) == data_before
    corruption = corrupt_copied_object(data, historical["layout"], historical["collection"])
    storage = owner / "User" / "globalStorage"
    copied_helper = storage / "iokaio.airk" / "helpers" / helper.parent.name / helper.name
    copied_helper.parent.mkdir(parents=True)
    shutil.copy2(helper, copied_helper)
    assert saved.sha(copied_helper) == historical["helper"]["sha256"]
    config = owner / "User" / "config.toml"
    server = "airk-negative-integrity"
    argv = ["--root", str(original_source), "--data-dir", str(data), "--collection", historical["collection"], "mcp"]
    config.write_text(f"[mcp_servers.{server}]\ncommand = {json.dumps(str(copied_helper))}\nargs = {json.dumps(argv)}\n", encoding="utf-8")
    config_sha = saved.sha(config)
    report.update(fixture=str(owner), corruption=corruption, original_config_sha256=config_sha)
    with clients.Fixture(runtime, owner, command_timeout=240) as fixture:
        original = fixture.managed_health
        active = original["runtime_sha256"]
        locators = saved.write_native_input(fixture.control, "locators", [{"path": str(config), "format": "codex_toml", "server": server}])
        prepared = json.loads(fixture.run("runtime", "prepare-legacy-caches", "--install-dir", fixture.install,
            "--expected-active", active, "--global-storage", storage, "--origin-locators-file", locators).stdout)
        operation = uuid.uuid4().hex
        failed = fixture.run("runtime", "adopt-legacy-indexes", "--install-dir", fixture.install,
            "--expected-active", active, "--operation-id", operation, "--locators-file", locators,
            "--expected-cache-inspection-sha256", prepared["inspection_sha256"], check=False)
        report["native_failure"] = {"exit_code": failed.returncode, "stdout": failed.stdout, "stderr": failed.stderr}
        assert len(failed.stdout.encode()) + len(failed.stderr.encode()) <= 2 * 1024 * 1024
        # Refresh the fixture's expected owner before its canonical final Stop.
        current = fixture.rpc("health")["observation"]
        fixture.managed_health = current
        journal = json.loads((fixture.install / "legacy-maintenance.json").read_text(encoding="utf-8"))
        report.update(maintenance=journal, original_health=original, restored_health=current)
        assert failed.returncode != 0 and "historical payload digest or bound mismatch" in failed.stderr
        plan = journal["adoption_plans"][0]
        receipt = fixture.rpc({"catalog_index_setup_receipt": {"caller": plan["setup"]["caller"], "key": plan["setup"]["key"]}})["receipt"]
        validate_restoration(operation, original, current, journal, receipt)
        report["prepared_native_receipt"] = receipt
        database = (fixture.control / "service-1.4.sqlite3").resolve(strict=True)
        with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as connection:
            connection.execute("PRAGMA query_only=ON")
            counts = {name: connection.execute(f"SELECT COUNT(*) FROM {name}").fetchone()[0]
                      for name in ("service_jobs", "service_background", "service_watch", "service_setup_pending", "query_projects", "index_access_grants")}
            raw_receipt = connection.execute("SELECT payload FROM index_operation_receipts WHERE id=? AND kind='catalog_index_setup' AND stage='prepared'",
                                             (receipt["operation_id"],)).fetchone()[0]
        assert len(raw_receipt) <= 16384
        before = read_verified_snapshot(fixture.install / f"legacy-maintenance-cycle-{operation}-{journal['cycle']}.json", journal["cycle_state_sha256"])
        after = read_verified_snapshot(fixture.install / f"legacy-maintenance-restoration-{operation}-{journal['cycle']}.json", journal["restoration_state_sha256"])
        assert Path(before["control"]).samefile(fixture.control) and Path(after["control"]).samefile(fixture.control)
        validate_snapshot(before, after, receipt, raw_receipt)
        report["native_restoration_snapshot"] = after
        report["native_cycle_baseline_snapshot"] = before
        assert all(count == 0 for count in counts.values()), counts
        report["native_metadata_counts"] = counts
        assert saved.sha(config) == config_sha and saved.inventory(data) == corruption["copied_inventory_after"]
        assert (fixture.install / "legacy-cache-retirement.json").is_file()
        assert not copied_helper.exists()
        assert saved.inventory(original_data) == data_before and saved.inventory(original_source) == source_before
        assert saved.sha(helper) == historical["helper"]["sha256"] and saved.sha(archive["path"]) == archive["sha256"]
    assert fixture.managed_stop is not None
    report.update(passed=True, adoption_succeeded=False, fixture_preserved=True,
                  original_fixture_unchanged=True, pending_cache_preserved=True,
                  canonical_final_stop=fixture.managed_stop)


class PreserveExpectedFailureFixture(Exception):
    pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--fixture-report", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), "Refusing evidence overwrite"
    historical = json.loads(args.fixture_report.read_text(encoding="utf-8"))
    case = next(c for c in historical["candidates"] if c["tag"] == args.tag and c["outcome"] == "created")
    report = {"schema": 1, "qualification": "expected damaged-copy failure and real native restoration; no agent UI",
              "harness_sha256": saved.sha(__file__), "positive_helper_sha256": saved.sha(saved.__file__),
              "fixture_report_sha256": saved.sha(args.fixture_report), "runtime_manifest_sha256": saved.sha(args.runtime / "airk-install.json")}
    try:
        with clients.owned_client_fixture() as temporary:
            owner = Path(temporary) / args.tag
            owner.mkdir(mode=0o700)
            try:
                qualify(args.runtime.resolve(strict=True), case, owner, report)
            except BaseException as error:
                report["error"] = {"type": type(error).__name__, "message": str(error)[:4096]}
                saved.write_private_json(args.output, report)
                raise
            saved.write_private_json(args.output, report)
            # Preserve the intentionally pending native/cache transaction even
            # on a successful negative campaign. Do not delete or edit journals.
            raise PreserveExpectedFailureFixture()
    except PreserveExpectedFailureFixture:
        print(json.dumps({"passed": True, "adoption_succeeded": False, "fixture_preserved": True, "output": str(args.output)}))


if __name__ == "__main__":
    main()
