# SPDX-License-Identifier: Apache-2.0
"""Read complete first-level AWS guide prefixes through genuine, separately authenticated native service lifetimes."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
probe = runpy.run_path(str(HERE / "probe_aws_canceled_worker.py"))
gates = runpy.run_path(str(HERE / "observe_aws_public_gates.py"))
groups = runpy.run_path(str(HERE / "aws_guide_partitions.py"))
normal, n, E, require = (probe[k] for k in ("normal", "n", "E", "require"))
FINAL = E / "aws-candidate-retained-r5"
PAIR_SHA = "1bcd2c1a637a1cef0ef9c91484b93dead47400817a888fcecd8f28403fac621e"


class Service:
    def __init__(self, w, owner, out, control, deadline, member):
        self.w, self.member = w, member
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        for routine, result, params in (
                ("OpenProcess", wintypes.HANDLE, [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]),
                ("WaitForSingleObject", wintypes.DWORD, [wintypes.HANDLE, wintypes.DWORD]),
                ("GetExitCodeProcess", wintypes.BOOL, [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]),
                ("CloseHandle", wintypes.BOOL, [wintypes.HANDLE])):
            fn = getattr(self.api, routine)
            fn.restype, fn.argtypes = result, params
        self.process = None
        self.commands = out / "commands"
        self.commands.mkdir()
        self.control, self.deadline = control, deadline
        self.images, self.pins = probe["pair"](owner, E / "aws-worker-checks-r2/pair-proof.private.json", PAIR_SHA)
        self.env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "PYTHON", "RUST_LOG"))}
        self.env.update(AIRK_TEST_FIXTURE_CONTROL=str(control), AIRK_TEST_FIXTURE_MAIN_SHA256=self.pins["airk"]["sha256"],
                        AIRK_TEST_FIXTURE_CLI_SHA256=self.pins["airkctl"]["sha256"])

    def command(self, name, args, until):
        return self.w["run_logged"]([self.images["airkctl"], "--output", "json", "--service-runtime-dir", self.control, *args],
                                    self.commands, name, self.env, until)[0]

    def cli(self, name, *args):
        return self.w["read_json"](self.command(name, args, self.deadline - 30))

    def start(self, ordinal):
        label = f"bundle-{ordinal:02}-start"
        out, _ = self.w["run_logged"]([self.images["start_retained_fixture"], "--ignored", "--exact",
                "start_retained_fixture_pair", "--nocapture", "--test-threads=1"], self.commands, label,
                self.env, self.deadline - 30)
        logs = [line.partition("AIRK_FIXTURE_HEALTH ")[2] for line in out.read_text().splitlines()
                if "AIRK_FIXTURE_HEALTH " in line]
        require(len(logs) == 1, "missing candidate start health")
        health = json.loads(logs[0])["observation"]
        require(health["user"] == probe["SID"] and health["executable_sha256"] == self.pins["airk"]["sha256"],
                "original started image or user differs")
        self.member(health["process_id"])
        self.process = self.api.OpenProcess(0x101000, False, health["process_id"])
        if not self.process:
            self.command(label + "-stop-unheld", ["service", "stop", "--runtime-dir", self.control], self.deadline)
            raise ValueError("cannot hold original owned process")
        public = self.cli(label + "-health", "service", "health", "--runtime-dir", self.control)["observation"]
        require(health["user"] == public["user"] == probe["SID"]
                and health["instance"] == public["instance"] and health["process_id"] == public["process_id"]
                and public["executable_sha256"] == health["executable_sha256"] == self.pins["airk"]["sha256"],
                "native service health or owner differs")
        self.member(public["process_id"])
        return public

    def stop(self, ordinal):
        try:
            result = self.w["read_json"](self.command(f"bundle-{ordinal:02}-stop", ["service", "stop", "--runtime-dir", self.control],
                                                      self.deadline))
            require(result["status"] == "lifecycle_accepted", "ordinary service stop refused")
            timeout = max(0, min(30_000, int((self.deadline - time.monotonic()) * 1000)))
            require(self.api.WaitForSingleObject(self.process, timeout) == 0, "original service failed to settle")
            code = wintypes.DWORD()
            require(self.api.GetExitCodeProcess(self.process, ctypes.byref(code)) and code.value == 0,
                    "original owned service did not exit normally")
            self.w["write_json"](self.commands / f"bundle-{ordinal:02}-process-settled.json",
                                 {"normal_exit": True, "exit_code": code.value})
        finally:
            if self.process:
                self.api.CloseHandle(self.process)
                self.process = None

    def read(self, label, index, payload, scope):
        until = min(time.monotonic() + 120, self.deadline - 30)
        args = ("tools", "call", "airk_list_files", "--index", index, "--args", json.dumps(payload))
        def invoke(name, arguments, original):
            return self.w["read_json"](self.command(name, arguments, original))
        return self.w["tool_body"](probe["public_read"](invoke, self.w["read_json"], self.commands, label,
                                                       args, index, until, scope))


def pilot_source(w, out, tree, members, rootname):
    selected = {}
    for prefix, paths in sorted(groups["partition"](members).items()):
        natural = sorted(path for path in paths if path.endswith(".md"))[:2]
        if len(natural) != 2:
            continue
        for name in natural:
            raw = (E / rootname / name).read_bytes()
            require(hashlib.sha256(raw).hexdigest() == members[name]["sha256"], "pilot original content differs")
            target = tree / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
            selected[name] = members[name]
        if len(selected) == 6:
            break
    require(len(selected) == 6 and len(groups["partition"](selected)) == 3, "three-guide pilot lacks originals")
    w["write_json"](out / "pilot-originals.private.json", {k: v["sha256"] for k, v in selected.items()})
    return selected


def build_pilot(service, tree, store, out):
    log = service.command("register", ["--root", tree, "--data-dir", store, "library", "setup", "--name",
                          "aws-original-guide-pilot", "--watch", "off", "--apply"], service.deadline - 30)
    _, applied = (json.loads(line) for line in log.read_bytes().splitlines())
    receipt = applied["receipt"]
    binding = receipt["request"]["registration"]["binding"]
    policy = out / "source-only.json"
    service.w["write_json"](policy, {"operation": "vectors", "enabled": False})
    for label, args in (("policy", ["configure", "--file", policy]), ("index", ["index"])):
        job = service.cli(label, "--service-project", binding, *args)["job"]
        sample = 0
        while True:
            status = service.cli(f"{label}-job-{sample}", "jobs", "get", job["id"])["job"]
            if status["state"] == "succeeded":
                break
            require(status["state"] in ("pending", "running") and time.monotonic() + 31 < service.deadline,
                    "pilot job failed or reached original deadline")
            time.sleep(.5)
            sample += 1
    return receipt["index_id"]


def authenticate_kernel(w, owner, path, seal):
    require(path == E / "aws-kernel-census-r2/owned.json", "wrong completed kernel source")
    n["checked"](owner.plain(path, regular=True), seal)
    normal["stage"]["require_settlement"](w, w["read_json"](path))
    audit = w["read_json"](path.parent / "owner-audit.json")
    require(audit["exit_code"] == 0 and audit["owner"] == probe["SID"], "native kernel ownership incomplete")
    result = w["read_json"](path.parent / "kernel-census.private.json")
    raw = owner.plain(path.parent / "native-list.stdout", regular=True)
    n["checked"](raw, result["native_listing_sha256"])
    rows = set()
    count = 0
    for line in raw.read_bytes().splitlines():
        page = json.loads(line)
        require(page["publication"]["token"] == probe["old"]["REVISION"] and len(page["paths"]) == len(set(page["paths"]))
                and not rows.intersection(page["paths"]),
                "kernel source changed revision or repeated a file")
        rows.update(page["paths"])
        count += 1
    require(count == 354 and len(rows) == 35400 and not page["truncated"], "kernel reference is incomplete")
    require(result["before"] == result["after"] == w["read_json"](FINAL / "after.json")
            and w["read_json"](path.parent / "comparison.json")["different_rows"] == 0, "read-only kernel custody differs")
    return rows


def read_guide(service, guide, ordinal, index, allowed, expected, limit, revision, prior_scope):
    def call(name, tool, args):
        require(tool == "airk_list_files", "unexpected native operation")
        return service.read(f"guide-{ordinal:03}-{name}", index, args, prior_scope)
    first = call("first", "airk_list_files", {"prefix": guide, "limit": limit, "max_bytes": 65536,
                                             "response_mode": "full", "include_status": False})
    token = first["publication"]["token"]
    require(revision is None or token == revision, "original publication changed")
    rows = []
    try:
        for page in gates["pages"](first, call, allowed, limit, guide):
            rows.extend(page)
    except (AssertionError, ValueError, TimeoutError, KeyError, TypeError) as error:
        return {"prefix": guide, "revision": token, "scope": first["scope_observation"], "paths": rows,
                "terminal": False, "error": str(error)}
    return {"prefix": guide, "revision": token, "scope": first["scope_observation"], "paths": rows,
            "terminal": True, "missing": sorted(expected - set(rows)), "extra": sorted(set(rows) - expected)}


def execute(w, owner, args, out, deadline, member):
    pilot = args.mode == "pilot"
    menu = probe["menu"]
    inventory = E / menu["INVENTORY"]
    n["checked"](inventory, menu["INVENTORY_SHA"])
    rootname, _, members = menu["inputs"](w["read_json"](inventory))
    control = out / "control" if pilot else probe["old"]["ORIGIN"] / "control"
    store = out / "index" if pilot else probe["old"]["ORIGIN"] / "index"
    revision = scope = index = None
    if pilot:
        creator = n["ROOT"] / "scripts/arbiter/evaluation/research_history/composed_orchestration/stage_assets--5a7c8d6f3f93.py"
        n["checked"](creator, "5a7c8d6f3f93431eb6c5674cc53eeeda10db53a1c6b62a5970b8b849b7408028")
        runpy.run_path(str(creator))["protected_child"](control)
        tree = out / "source"
        tree.mkdir()
        store.mkdir()
        members = pilot_source(w, out, tree, members, rootname)
        visible = set(members)
        limit, page_budget = 1, 4
    else:
        n["checked"](FINAL / "owned.json", "24b7af6875592d9072551ecab5743eb305a6fe30fcc1c18ecf3b2547db781da9")
        n["checked"](FINAL / "activation.private.json", args.activation_sha)
        n["checked"](owner.plain(args.pilot_proof, regular=True), args.pilot_sha)
        require(args.pilot_proof == E / "aws-guide-prefix-pilot-r1/owned.json", "wrong prefix pilot")
        normal["stage"]["require_settlement"](w, w["read_json"](args.pilot_proof))
        control_run = w["read_json"](args.pilot_proof.parent / "guide-union.private.json")
        require(control_run["complete"] and control_run["lifetimes"] == 2 and control_run["unique_files"] == 6,
                "two native original-guide pilot lifetimes required")
        visible = authenticate_kernel(w, owner, args.kernel_proof, args.kernel_sha)
        require(visible <= members.keys(), "kernel reference contains an unexpected source")
        activation = w["read_json"](FINAL / "activation.private.json")
        revision, scope, index = activation["first_publication"], activation["first_scope"], activation["index"]
        require(revision == probe["old"]["REVISION"], "wrong original R5 publication")
        n["checked"](control / "service-1.4.sqlite3", "e868fc8e1b65881bdc42141bf586ae704f0dc80b2a9e2592fdd8469e0eef3fab")
        baseline = probe["witness"]["capture"](store, out / "before.rows", deadline)
        require(baseline == w["read_json"](FINAL / "after.json"), "persisted store changed after R5/kernel")
        w["write_json"](out / "before.json", baseline)
        limit, page_budget = 100, 160
    inputs = groups["partition"](members)
    expected = groups["partition"](visible)
    require(inputs.keys() == expected.keys() and (pilot or len(inputs) == 226), "guide coverage differs")
    plan = groups["bundles"](expected, limit, page_budget)
    service = Service(w, owner, out, control, deadline, member)
    reports, instances, all_paths = [], [], set()
    number = 0
    try:
        for sequence, bundle in enumerate(plan):
            try:
                health = service.start(sequence)
                require(health["instance"] not in instances, "native service instance was reused")
                instances.append(health["instance"])
                if pilot and sequence == 0:
                    index = build_pilot(service, tree, store, out)
                for guide in bundle:
                    try:
                        row = read_guide(service, guide, number, index, inputs[guide], expected[guide], limit, revision, scope)
                    except (AssertionError, ValueError, TimeoutError, KeyError, TypeError) as error:
                        row = {"prefix": guide, "terminal": False, "paths": [], "error": str(error)}
                    if "scope" in row:
                        scope = scope or row["scope"]
                        revision = revision or row["revision"]
                    require(not all_paths.intersection(row["paths"]), "native guide results overlap")
                    all_paths.update(row["paths"])
                    row["service_instance"] = health["instance"]
                    w["write_json"](out / f"guide-{number:03}.private.json", row)
                    reports.append(row)
                    number += 1
            finally:
                if service.process:
                    service.stop(sequence)
    finally:
        if not pilot:
            after = probe["witness"]["capture"](store, out / "after.rows", deadline)
            w["write_json"](out / "after.json", after)
            left = probe["published_identity"](out / "before.rows")
            right = probe["published_identity"](out / "after.rows")
            w["write_json"](out / "index-changes.private.json", {**probe["witness"]["compare"](out / "before.rows", out / "after.rows"),
                            "before_publication_files": left, "after_publication_files": right})
            require(left == right, "original publication material changed")
    exact = {r["prefix"] for r in reports} == set(expected) and all_paths == visible
    complete = exact and all(r["terminal"] and not r.get("missing") and not r.get("extra") for r in reports)
    w["write_json"](out / "guide-union.private.json", {"complete": complete, "native_revision": revision,
                    "lifetimes": len(instances), "instances": instances, "guides": len(reports), "unique_files": len(all_paths),
                    "visible_reference_files": len(visible), "source_files": len(members), "original_menu_single_cursor_complete": False,
                    "source_absent": sorted(members.keys() - visible), "unobserved": sorted(visible - all_paths),
                    "unexpected": sorted(all_paths - visible), "model_scored": False})


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--mode", choices=("pilot", "full"), required=True)
    p.add_argument("--output", type=Path, required=True)
    for name in ("kernel", "pilot"):
        p.add_argument(f"--{name}-proof", type=Path)
        p.add_argument(f"--{name}-sha")
    p.add_argument("--activation-sha")
    p.add_argument("--worker", action="store_true")
    args = p.parse_args()
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    out = owner.plain(args.output)
    require(out.parent == E and out.name.startswith("aws-guide-prefix-" + args.mode + "-"), "wrong private output")
    if args.mode == "full":
        require(all((args.pilot_proof, args.pilot_sha, args.kernel_proof, args.kernel_sha, args.activation_sha)),
                "sealed native control and kernel evidence required")
    if args.worker:
        require(os.environ["AIRK_AWS_GUIDE_ROOT"] == str(out), "original owned worker required")
        with normal["capped"](w, out, 2 << 30) as (member, _):
            execute(w, owner, args, out, float(os.environ["AIRK_AWS_GUIDE_DEADLINE"]), member)
        return
    require(not out.exists(), "fresh evidence required")
    wall = min(600 if args.mode == "pilot" else 7200, int(1795194600 - time.time()))
    require(wall > 120, "insufficient remaining campaign time")
    out.mkdir()
    tmp = out / "tmp"
    tmp.mkdir()
    argsv = [sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"]
    result = owner.run_evaluation_fixture(argsv, n["ROOT"], out / "outer.log", wall, os.environ | {
                "TMP": str(tmp), "TEMP": str(tmp), "AIRK_AWS_GUIDE_ROOT": str(out),
                "AIRK_AWS_GUIDE_DEADLINE": str(time.monotonic() + wall)})
    w["write_json"](out / "owned.json", result)
    normal["stage"]["require_settlement"](w, result)


if __name__ == "__main__":
    main()
