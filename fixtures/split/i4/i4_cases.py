# SPDX-License-Identifier: Apache-2.0
"""Generate the I4 install pointer, record and setup-operation cases (split C6).

Expectations are reviewed statements of the I4 contract (docs/split/contracts.md
§5), written independently of the Rust readers: this file states the canonical
byte format itself (one compact JSON line, fields in declaration order, then a
newline), computes record digests with hashlib and names the expected outcome.

    python fixtures/split/i4/i4_cases.py          # rewrite i4_cases.json
    python fixtures/split/i4/i4_cases.py --check  # exit 1 on drift
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("i4_cases.json")
SCHEMA = "airk.split-fixture.i4-cases/1"
INSTALL_ID = "0d" * 16
OPERATION_ID = "0e" * 16
PROGRAM = "C:\\Users\\fixture\\AppData\\Local\\Programs\\AIRK"
ROOT = "C:\\Users\\fixture\\AppData\\Local\\AIRK"
NOW = 1_800_000_000_000


def canonical(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False) + "\n"


def digest(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def locations(program=PROGRAM):
    return {"program": program, "data": ROOT + "\\data\\indexes", "state": ROOT + "\\state\\service",
            "cache": ROOT + "\\cache\\service", "logs": ROOT + "\\logs\\service"}


def record():
    return {
        "schema": "airk-install-record/1", "install_id": INSTALL_ID, "version": "1.9.0", "channel": "stable",
        "target": "x86_64-pc-windows-msvc",
        "contracts": {"i1_protocol": 1, "i2_contract": 1, "i3_admin": 1, "i4_record": 1, "state": "airk-state/1"},
        "executables": {
            "airkctl": {"path": "bin/airkctl.exe", "sha256": "1" * 64, "bytes": 4_000_000},
            "airk": {"path": "service/airk.exe", "sha256": "2" * 64, "bytes": 300_000_000},
        },
        "admin_assets": {"inventory_sha256": "3" * 64, "bundle_sha256": "4" * 64},
        "locations": locations(),
        "signing": {"platform": "windows", "publisher": "CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US",
                    "identity_eku": "1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669"},
        "installed_by": {"operation_id": OPERATION_ID, "installer": "inno", "installed_unix_ms": NOW},
    }


def pointer(record_text):
    return {"schema": "airk-install-pointer/1", "install_id": INSTALL_ID, "program": PROGRAM,
            "record": "install-record.json", "record_sha256": digest(record_text), "version": "1.9.0",
            "written_unix_ms": NOW + 1}


def operation():
    return {
        "schema": "airk-setup-operation/1", "operation_id": OPERATION_ID, "kind": "install",
        "created_unix_ms": NOW, "expires_unix_ms": NOW + 600_000, "requested_by": "inno",
        "acknowledgements": {"predecessor_removal": True, "index_reset": True},
        "locations": locations(), "payload_inventory_sha256": "5" * 64,
        "options": {"login_startup": True, "trust": "ask", "start": True, "delete_indexes": "keep"},
    }


def changed(base, path, value):
    result = copy.deepcopy(base)
    node = result
    for key in path[:-1]:
        node = node[key]
    if value is DELETE:
        del node[path[-1]]
    else:
        node[path[-1]] = value
    return result


DELETE = object()


def cases():
    good_record = canonical(record())
    good_pointer = canonical(pointer(good_record))
    pointers = [
        {"name": "valid pointer", "text": good_pointer, "expect": "ok"},
        {"name": "pretty-printed pointer is not canonical", "text": json.dumps(pointer(good_record), indent=1) + "\n",
         "expect": "install_file_non_canonical"},
        {"name": "missing trailing newline", "text": good_pointer.rstrip("\n"), "expect": "install_file_non_canonical"},
        {"name": "unknown field", "text": canonical({**pointer(good_record), "extra": 1}), "expect": "install_file_malformed"},
        {"name": "other schema", "text": canonical(changed(pointer(good_record), ["schema"], "airk-install-pointer/2")),
         "expect": "install_file_unknown_schema"},
        {"name": "uppercase install id", "text": canonical(changed(pointer(good_record), ["install_id"], "0D" * 16)),
         "expect": "install_file_malformed"},
        {"name": "relative program folder", "text": canonical(changed(pointer(good_record), ["program"], "Programs\\AIRK")),
         "expect": "install_location_relative"},
        {"name": "other record file name", "text": canonical(changed(pointer(good_record), ["record"], "..\\record.json")),
         "expect": "install_file_malformed"},
        {"name": "oversize", "text": canonical(changed(pointer(good_record), ["program"], PROGRAM + "\\" + "x" * 5000)),
         "expect": "install_file_oversize"},
    ]
    records = [
        {"name": "valid record", "text": good_record, "pointer": good_pointer, "expect": "ok"},
        {"name": "record differs from the pointer digest", "text": canonical(changed(record(), ["channel"], "preview")),
         "pointer": good_pointer, "expect": "install_record_digest_mismatch"},
    ]

    def record_case(name, value, expect):
        text = canonical(value)
        return {"name": name, "text": text, "pointer": canonical(pointer(text)), "expect": expect}

    records += [
        record_case("other install id", changed(record(), ["install_id"], "0f" * 16), "install_id_mismatch"),
        record_case("other version", changed(record(), ["version"], "1.9.1"), "install_version_mismatch"),
        record_case("executable escapes the program folder",
                    changed(record(), ["executables", "airk", "path"], "../airk.exe"), "install_executable_outside_program"),
        record_case("absolute executable path",
                    changed(record(), ["executables", "airkctl", "path"], "C:\\airkctl.exe"), "install_executable_outside_program"),
        record_case("backslash executable path",
                    changed(record(), ["executables", "airkctl", "path"], "bin\\airkctl.exe"), "install_executable_outside_program"),
        record_case("relative data location", changed(record(), ["locations", "data"], "data"), "install_location_relative"),
        record_case("program location differs from the pointer",
                    changed(record(), ["locations", "program"], "D:\\AIRK"), "install_location_relative"),
        record_case("missing admin assets", changed(record(), ["admin_assets"], DELETE), "install_file_malformed"),
        record_case("unknown signing platform",
                    changed(record(), ["signing"], {"platform": "plan9", "signed": False}), "install_file_malformed"),
        record_case("linux integrity signing", changed(record(), ["signing"],
                    {"platform": "linux", "signed": False, "integrity": "record-sha256"}), "ok_with_other_pointer"),
    ]
    for case in records:
        if case["expect"] == "ok_with_other_pointer":
            case["expect"] = "ok"
    operations = [
        {"name": "valid operation", "text": canonical(operation()), "now_unix_ms": NOW + 1, "expect": "ok"},
        {"name": "expired", "text": canonical(operation()), "now_unix_ms": NOW + 600_000, "expect": "setup_operation_expired"},
        {"name": "clock before creation", "text": canonical(operation()), "now_unix_ms": NOW - 1, "expect": "setup_operation_expired"},
        {"name": "lifetime above ten minutes", "text": canonical(changed(operation(), ["expires_unix_ms"], NOW + 600_001)),
         "now_unix_ms": NOW + 1, "expect": "setup_operation_lifetime_too_long"},
        {"name": "unknown option", "text": canonical(changed(operation(), ["options", "silent"], True)),
         "now_unix_ms": NOW + 1, "expect": "install_file_malformed"},
        {"name": "unknown index deletion choice", "text": canonical(changed(operation(), ["options", "delete_indexes"], "maybe")),
         "now_unix_ms": NOW + 1, "expect": "install_file_malformed"},
        {"name": "relative cache location", "text": canonical(changed(operation(), ["locations", "cache"], "cache")),
         "now_unix_ms": NOW + 1, "expect": "install_location_relative"},
    ]
    nonce = "1a" * 16

    def start(**changes):
        value = {"schema": "airk-start-request/1", "nonce": nonce, "mode": "explicit",
                 "created_unix_ms": NOW, "expires_unix_ms": NOW + 30_000}
        value.update(changes)
        return canonical(value)

    starts = [
        {"name": "valid explicit start", "text": start(), "nonce": nonce, "now_unix_ms": NOW + 1, "expect": "ok"},
        {"name": "valid automatic start", "text": start(mode="automatic"), "nonce": nonce, "now_unix_ms": NOW,
         "expect": "ok"},
        {"name": "request for another nonce", "text": start(), "nonce": "1b" * 16, "now_unix_ms": NOW + 1,
         "expect": "start_request_nonce_mismatch"},
        {"name": "expired at its deadline", "text": start(), "nonce": nonce, "now_unix_ms": NOW + 30_000,
         "expect": "start_request_expired"},
        {"name": "clock before creation", "text": start(), "nonce": nonce, "now_unix_ms": NOW - 1,
         "expect": "start_request_expired"},
        {"name": "lifetime above thirty seconds", "text": start(expires_unix_ms=NOW + 30_001), "nonce": nonce,
         "now_unix_ms": NOW + 1, "expect": "start_request_lifetime_too_long"},
        {"name": "unknown mode", "text": start(mode="forced"), "nonce": nonce, "now_unix_ms": NOW + 1,
         "expect": "install_file_malformed"},
        {"name": "unknown field", "text": canonical({"schema": "airk-start-request/1", "nonce": nonce,
         "mode": "explicit", "created_unix_ms": NOW, "expires_unix_ms": NOW + 30_000, "runtime_dir": "C:\\x"}),
         "nonce": nonce, "now_unix_ms": NOW + 1, "expect": "install_file_malformed"},
        {"name": "pretty-printed request is not canonical", "text": json.dumps(json.loads(start()), indent=1) + "\n",
         "nonce": nonce, "now_unix_ms": NOW + 1, "expect": "install_file_non_canonical"},
    ]
    phases = ["locked", "predecessors_retired", "service_drained", "placed", "record_written", "pointer_written",
              "login_registered", "login_failed", "trust_applied", "trust_declined", "trust_unsupported",
              "started", "healthy", "complete"]
    return {"schema": SCHEMA, "contract": "crates/airk-core/src/install_record/mod.rs",
            "limits": {"pointer": 4096, "record": 65536, "setup_operation": 16384, "operation_lifetime_ms": 600000,
                       "start_request": 1024, "start_request_lifetime_ms": 30000},
            "pointers": pointers, "records": records, "operations": operations, "start_requests": starts,
            "phases": phases}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    rendered = json.dumps(cases(), indent=1, ensure_ascii=False) + "\n"
    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != rendered:
            print(f"{OUTPUT.name} differs; run without --check after review", file=sys.stderr)
            return 1
        return 0
    OUTPUT.write_text(rendered, encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
