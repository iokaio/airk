# SPDX-License-Identifier: Apache-2.0
"""Observe the checked candidate pair through a real admitted private service lifetime."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
old = runpy.run_path(str(HERE / "observe_aws_kernel_store.py"))
witness = runpy.run_path(str(HERE / "aws_kernel_entry_evidence.py"))
normal, n, E = old["normal"], old["n"], old["E"]
require, menu = n["require"], normal["menu"]
grants = runpy.run_path(str(HERE / "aws_grant_lifetime_control.py"))
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"


def pair(owner, path, sha):
    require(path.parent.parent == E and path.parent.name.startswith("aws-worker-checks-")
            and path.name == "pair-proof.private.json", "wrong original Cargo receipt")
    n["checked"](owner.plain(path, regular=True), sha)
    proof = json.loads(path.read_bytes())
    for stage, digest in proof["receipts"].items():
        origin = proof["receipt_origins"][stage]
        require(origin == path.parent.name or (stage in ("clippy", "nextest") and origin == "aws-worker-checks-r1"),
                "unexpected native receipt origin")
        n["checked"](owner.plain(E / origin / ("aws-worker-" + stage + "-r1.json"), regular=True), digest)
    require(set(proof["receipts"]) == {"clippy", "nextest", "pair", "starter"}, "missing native checks")
    images = {}
    for name in ("airk", "airkctl", "start_retained_fixture"):
        row = proof["executables"][name]
        p = owner.plain(path.parent / "pair" / (name + ".exe"), regular=True)
        require(row["path"] == p.name and p.stat().st_size == row["bytes"], "unexpected staged image")
        n["checked"](p, row["sha256"])
        images[name] = p
    return images, proof["executables"]


def published_identity(rows):
    """Protect immutable source/index material while recording the complete mutable runtime diff."""
    digest = hashlib.sha256()
    count = 0
    with rows.open("rb") as stream:
        for raw in stream:
            path, directory, *_ = json.loads(raw)
            if directory:
                continue
            parts = Path(path).parts
            if ("incarnations" in parts and (any(k in parts for k in ("objects", "current-metadata", "current-resources"))
                    or parts[-1] in ("current.sqlite3", "current-bootstrap.json", "current-owner.json"))
                    or parts[-1] == "active.json" or "current-grants" in parts):
                digest.update(raw)
                count += 1
    require(count > 100_000, "original publication material absent")
    return {"regular_files": count, "sha256": digest.hexdigest()}


def retained_inputs(w, owner, source_menu):
    tree, cases = normal["stage"]["source_closure"](w, owner, source_menu)
    case, = cases
    return tree, cases, case["members"]


def public_read(request, files, directory, label, args, index, until, expected_scope=None):
    client = n["ROOT"] / "scripts/check-service-clients.py"
    n["checked"](client, "9f7d8257eb38bf5fb1d26c8bb6297a9c925da026b59be65f5445639d431bcf22")
    retry = runpy.run_path(str(client))
    attempt = 0
    scope = None

    def read(deadline):
        nonlocal attempt, scope
        name = label if not attempt else f"{label}-retry-{attempt}"
        attempt += 1
        try:
            envelope = request(name, args, deadline)
        except ValueError:
            response = directory / (name + ".stdout")
            if not response.is_file() or response.stat().st_size > 65536:
                raise
            command = files(directory / (name + ".command.json"))
            if command["exit_code"] != 1:
                raise
            envelope = files(response)
            body = envelope.get("structuredContent")
            if not isinstance(body, dict) or body.get("code") not in ("selection_busy", "publication_busy"):
                raise
            if retry["read_retry_delay"](envelope) is None:
                raise
        body = envelope.get("structuredContent")
        require(isinstance(body, dict), "missing original native read object")
        observed = body.get("scope_observation")
        require(isinstance(observed, dict) and [m["index_id"] for m in observed["members"]] == [index]
                and (scope is None or scope == observed)
                and (expected_scope is None or grants["durable_scope"](observed) == grants["durable_scope"](expected_scope)),
                "native read changed selection or requested index")
        scope = observed
        return envelope

    result, _ = retry["read_envelope"](read, until)
    return result


def retained_baseline(w, owner, continuing):
    if not continuing:
        return ("04c24127af8c519606e39b30761b0271220038ad73993225919eb2d301495a89",
                "c9336c3b399726040f91cb84a9effd7ab8c90b5e221198ee9e2a746a45b1650c", None)
    previous = E / "aws-candidate-retained-r2"
    pins = {"owned.json": "71b83b494fa63141e3addc5213442a5efea4a228a737466ba15eff09ea66d906",
            "owner-audit.json": "fb6c8ad25968d1dfff23956d3eadc776c6c25f5f43f66e5613d23ac2b27c2f41",
            "index-changes.private.json": "bbbd0d197e7b64b029144a025c82e4cb67d7a7641c383284003ff93cd5d9e01b",
            "after.json": "218b644b40fcfdf9d5e7ecce005d65a18c2db9c9dc8e48a3e74c1ca138d9802f",
            "commands/stop.command.json": "59bca39c0eba6aa94b076349b6a3f96c1bd8a3b5fbf0f5f4f14fabd3da2a5ccd",
            "commands/activation-first-page.stdout": "8861de2c4d5c6ddbcd0f926e23f3b2db920b391b0a31ad37ab1c5a7aded8b621"}
    database = "1fdb004c1b195518041d23313e0f71612ee9a4500078ac57545ea27456299068"
    if continuing == "renewed":
        previous = E / "aws-candidate-retained-r3"
        database = "0971a9fe16ecb6a7efa5d3a7a17ca282a8811cb4dc3f953e4f81adc2280d1d97"
        pins = {"owned.json": "07122a8608cc877603701f4c2b95cf6c59c1da68133927f7a264861538248825",
                "owner-audit.json": "a5ec1ccecbe7d1cc6d09643979113a1a26117f827dc946e67d66c3217258dcbc",
                "index-changes.private.json": "afbafeb455207177f6409b2787f66cbb3843a438f864180b7706306a5f7a2017",
                "after.json": "a550371861aa99ef7f52d7619be895e236c52ae73c6a9268bcf0c8cdd65da38a",
                "commands/stop.command.json": "b3360b5d4bb8778691cd71402c083b8da6caca3791be756e6be9d661acde4c4f",
                "commands/activation-first-page.stdout": "ba3d20813973221060a83833b3d4e893cc2a2dea0acd5fa103458afbb29f12ac"}
    if continuing == "status":
        previous = E / "aws-candidate-retained-r4"
        database = "7c96627d4031ff8afa5bbff23ee37f3386c62d5f29c6828ef52fd4728c27ad8e"
        pins = {"owned.json": "2c08968008eb013e53595292da683ca869cc3955e48426bd956ee3be2ab00573",
                "owner-audit.json": "bbe22ba6372e04b2fdf65c195862b090b1925950381a944fd1a6241a8309900d",
                "index-changes.private.json": "eb56507e8cceb8e8002b3f4b3f344563229cbc67ad39cfc8838e9c470ac6d172",
                "after.json": "e338cd720003152feafd6a4749ab814e95a3c2dc0cc302f25680838447ed985e",
                "commands/stop.command.json": "efdcdddcc336d181e4d491655d17b72bb1b89feb8634382e424d2e774efdcb0a",
                "commands/activation-first-page-retry-3.stdout": "ae19857a9b508c742a3b793e4600cae96c81c6f02436440632884d0ec45eb10a",
                "commands/source-status-retry-6.stdout": "8aa96c79c11fcacd882c5715f46cf11c9f7c9498127c2711daaad8a5cbfb29c6"}
    for name, pin in pins.items():
        n["checked"](owner.plain(previous / name, regular=True), pin)
    changes = w["read_json"](previous / "index-changes.private.json")
    require(changes["before_publication_files"] == changes["after_publication_files"], "prior native material changed")
    first = "activation-first-page-retry-3" if continuing == "status" else "activation-first-page"
    hint = w["read_json"](previous / "commands" / (first + ".stdout"))["structuredContent"]
    return database, w["read_json"](previous / "after.json")["sha256"], hint["scope_observation"]


def run(w, owner, output, mode, proof, sha, deadline, member, continuing, two_lifetimes):
    images, pins = pair(owner, proof, sha)
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "PYTHON", "RUST_LOG"))}
    runs = output / "commands"
    runs.mkdir()
    pilot = mode == "pilot"
    store = output / "index" if pilot else old["ORIGIN"] / "index"
    control = output / "control" if pilot else old["ORIGIN"] / "control"
    expected_scope = None
    if pilot:
        store.mkdir()
        creator = n["ROOT"] / "scripts/arbiter/evaluation/research_history/composed_orchestration/stage_assets--5a7c8d6f3f93.py"
        n["checked"](creator, "5a7c8d6f3f93431eb6c5674cc53eeeda10db53a1c6b62a5970b8b849b7408028")
        protected = runpy.run_path(str(creator))
        require(protected["SID"] == SID and not control.exists(), "fresh same-user control required")
        w["write_json"](output / "control-creation.json", protected["protected_child"](control))
    else:
        database_sha, baseline_sha, expected_scope = retained_baseline(w, owner, continuing)
        n["checked"](control / "service-1.4.sqlite3", database_sha)
        n["checked"](old["ORIGIN"] / "commands/register.stdout", old["REGISTER"])
        n["checked"](E / "aws-durable-store-quota-r1.private.json",
                     "d47c3ace56d068a85609887de801d4ca0ab297c70806a9d17d23aeda82f1d423")
    env.update(AIRK_TEST_FIXTURE_CONTROL=str(control), AIRK_TEST_FIXTURE_MAIN_SHA256=pins["airk"]["sha256"],
               AIRK_TEST_FIXTURE_CLI_SHA256=pins["airkctl"]["sha256"])
    if pilot:
        n["checked"](E / menu["INVENTORY"], menu["INVENTORY_SHA"])
        rootname, _, members = menu["inputs"](w["read_json"](E / menu["INVENTORY"]))
        tree = output / "source"
        tree.mkdir()
        paths = sorted(k for k in members if k.endswith(".md"))
        pilot_sources = {}
        pilot_names = ("source.md", "second.md", "third.md") if two_lifetimes else ("source.md",)
        for source, target in zip(paths, pilot_names):
            raw = (E / rootname / source).read_bytes()
            digest = hashlib.sha256(raw).hexdigest()
            require(digest == members[source]["sha256"], "pilot source changed")
            (tree / target).write_bytes(raw)
            pilot_sources[target] = digest
    else:
        tree, cases, members = retained_inputs(w, owner, menu)
        require(len(members) == 35515 and len(cases[0]["cases"]) == 41, "original native menu changed")
        base = witness["capture"](store, output / "before.rows", deadline)
        w["write_json"](output / "before.json", base)
        require(base["sha256"] == baseline_sha,
                "retained entry baseline changed since original read diagnosis")

    def command(label, *args, closing=False, request_deadline=deadline):
        return w["run_logged"]([images["airkctl"], "--output", "json", "--service-runtime-dir", control, *args],
                               runs, label, env, min(request_deadline, deadline - (0 if closing else 30)))[0]

    def cli(label, *args):
        return w["read_json"](command(label, *args))

    def tool(label, name, payload):
        original = min(deadline - 30, time.monotonic() + 120)
        def read(attempt, args, until):
            return w["read_json"](command(attempt, *args, request_deadline=until))

        envelope = public_read(read, w["read_json"], runs, label,
                        ("tools", "call", name, "--index", index, "--args", json.dumps(payload)), index, original, expected_scope)
        return w["tool_body"](envelope)

    def start(label="start"):
        path, _ = w["run_logged"]([images["start_retained_fixture"], "--ignored", "--exact",
                    "start_retained_fixture_pair", "--nocapture", "--test-threads=1"], runs, label, env, deadline - 30)
        entries = [line.split("AIRK_FIXTURE_HEALTH ", 1)[1] for line in path.read_text().splitlines()
                   if "AIRK_FIXTURE_HEALTH " in line]
        require(len(entries) == 1, "missing native started health")
        health = json.loads(entries[0])["observation"]
        require(health["executable_sha256"] == pins["airk"]["sha256"] and health["user"] == SID,
                "wrong real service image or owner")
        member(health["process_id"])
        return health

    launched = False
    try:
        try:
            health = start()
            launched = True
        except Exception:
            try:
                probe = cli("failed-start-health", "service", "health", "--runtime-dir", control)["observation"]
                if probe["executable_sha256"] == pins["airk"]["sha256"] and probe["user"] == SID:
                    member(probe["process_id"])
                    launched = True
            except Exception:
                pass
            raise
        running = cli("health", "service", "health", "--runtime-dir", control)["observation"]
        require(health["process_id"] == running["process_id"] and running["executable_sha256"] == pins["airk"]["sha256"],
                "service identity changed at public client")
        if pilot:
            reg = command("register", "--root", tree, "--data-dir", store, "library", "setup", "--name",
                          "aws-canceled-worker-pilot", "--watch", "off", "--apply")
        else:
            reg = old["ORIGIN"] / "commands/register.stdout"
        _, applied = (json.loads(line) for line in reg.read_bytes().splitlines())
        receipt = applied["receipt"]
        index = receipt["index_id"]
        project = receipt["request"]["registration"]["binding"]
        if pilot:
            policy = output / "source-only.json"
            w["write_json"](policy, {"operation": "vectors", "enabled": False})
            for label, args in (("policy", ["configure", "--file", policy]), ("index", ["index"])):
                job = cli(label, "--service-project", project, *args)["job"]
                count = 0
                while True:
                    current = cli(f"{label}-job-{count}", "jobs", "get", job["id"])["job"]
                    if current["state"] == "succeeded":
                        break
                    require(current["state"] in ("pending", "running") and time.monotonic() + 31 < deadline,
                            "pilot job failed or exceeded setup clock")
                    time.sleep(0.5)
                    count += 1
        else:
            vjob = cli("retained-cancellation", "jobs", "get",
                    "08f313523140e4e3e3dd1d9a94c4e9ffdf73a3bc317381a16ade443cededc4e1")["job"]
            require(vjob["state"] == "canceled" and vjob["project"] == project and vjob["attempts"] == 0,
                    "original cancellation changed")
        first = tool("activation-first-page", "airk_list_files",
                     {"prefix": "", "limit": 100, "max_bytes": 65536, "response_mode": "full", "include_status": False})
        token = first["publication"]["token"]
        paths = first["paths"]
        require(paths and (set(paths) == pilot_sources.keys() if pilot else token == old["REVISION"] and set(paths) <= members.keys()),
                "native first page is foreign")
        w["write_json"](output / "activation.private.json", {"health": running, "index": index, "project": project,
                         "first_publication": token, "first_page_size": len(paths), "first_scope": first.get("scope_observation"),
                         "pair_sha256": sha, "scored_cases": 0})
        if pilot:
            result = tool("pilot-marker", "airk_file_info", {"path": "source.md", "response_mode": "full"})
            require(result["content_hash"] == pilot_sources["source.md"], "wrong pilot content")
            if two_lifetimes:
                one = grants["collect"](tool, "lifetime-first", pilot_sources, require)
                expected_scope = one["scope_observations"][0]
                command("first-stop", "service", "stop", "--runtime-dir", control)
                launched = False
                renewed = start("restart")
                launched = True
                two = grants["collect"](tool, "lifetime-second", pilot_sources, require)
                require(renewed["instance"] != running["instance"] and one["revision"] == two["revision"],
                        "native control lifetime or revision differs")
                w["write_json"](output / "grant-lifetimes.private.json",
                                {"first": one, "second": two, "first_health": running, "second_health": renewed})
        elif continuing == "status":
            gate_owner = runpy.run_path(str(HERE / "observe_aws_public_gates.py"))
            gate_owner["source_gates"](w, output, first, cases, tree, tool)
        else:
            observed = menu["observe"](menu["COHORT"], cases, tree, tool)
            require(len(observed[0]["visible_positive_file_hashes"]) == 27, "incomplete positive content visibility")
            span_file = E / "aws-unique-source-corpus-r1/manifest.private.json"
            n["checked"](span_file, "ef2844ba2406de3b57ac1f5a6b13e582320bacae27779b704d38d4786375e882")
            originals = json.loads(span_file.read_bytes())["cases"]
            stable = {row["question_sha256"]: row for row in cases[0]["cases"]}
            require(len(originals) == len(stable) == 41, "wrong exact positive cohort")
            spans = []
            for i, row in enumerate(originals):
                canon = stable[row["question_sha256"]]
                path = canon["source_path"].removeprefix("documents/")
                require(row["selector"] == canon["selector"] and members[path]["sha256"] == row["document"]["sha256"],
                        "source or selector changed")
                span = row["source_span"]
                response = tool(f"source-range-{i:02}", "airk_read_file", {"path": path, "start_line": span["line_start"],
                      "end_line": span["line_end_inclusive"], "max_bytes": 65536, "response_mode": "full", "include_status": True})
                require(response["scope_observation"] == observed[0]["native_scope"], "range changed scope")
                case = {**row, "document": {**row["document"], "path": path}}
                spans.append({"selector": row["selector"], **n["verified_lines"](case, (tree / path).read_bytes(), response)})
            w["write_json"](output / "public-native-census.private.json",
                            {"revision": token, "observations": observed, "source_range_reads": spans,
                             "excluded_ambiguous": 24, "scored_requests": 0, "source_only_config_applied": False,
                             "original_quota_refusal_preserved": True, "ranker_observed": False})
    finally:
        if launched:
            command("stop", "service", "stop", "--runtime-dir", control, closing=True)
        if not pilot:
            after = witness["capture"](store, output / "after.rows", deadline)
            w["write_json"](output / "after.json", after)
            changes = witness["compare"](output / "before.rows", output / "after.rows")
            before_material = published_identity(output / "before.rows")
            after_material = published_identity(output / "after.rows")
            w["write_json"](output / "index-changes.private.json", {**changes, "before_publication_files": before_material,
                             "after_publication_files": after_material})
            require(before_material == after_material, "original publication material changed")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--mode", choices=("pilot", "retained"), required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--pair-proof", type=Path, required=True)
    p.add_argument("--pair-sha", required=True)
    p.add_argument("--pilot-proof", type=Path)
    p.add_argument("--pilot-sha")
    p.add_argument("--worker", action="store_true")
    p.add_argument("--two-lifetimes", action="store_true", help="exercise three native pages across two pilot lifetimes")
    p.add_argument("--after-native-status", action="store_true", help="preserve R4 status and collect independent source gates")
    p.add_argument("--after-renewed-grant", action="store_true", help="use the R3 stop and two-lifetime native control")
    p.add_argument("--after-selection-busy", action="store_true", help="resume from the sealed retained R2 stop")
    args = p.parse_args()
    require(not args.two_lifetimes or args.mode == "pilot", "grant control requires pilot mode")
    require(not args.after_native_status or (args.mode == "retained" and not args.after_selection_busy
            and not args.after_renewed_grant), "public gate continuation requires retained R4")
    require(not args.after_renewed_grant or (args.mode == "retained" and not args.after_selection_busy),
            "renewed-grant continuation requires retained R3")
    require(not args.after_selection_busy or args.mode == "retained", "continuation requires retained mode")
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    out = owner.plain(args.output)
    require(out.parent == E and out.name.startswith("aws-candidate-" + args.mode + "-"), "wrong private output")
    if args.mode == "retained":
        require(args.pilot_proof and args.pilot_sha, "settled pilot proof required")
        pilot = owner.plain(args.pilot_proof, regular=True)
        require(pilot.name == "owned.json" and pilot.parent.parent == E
                and pilot.parent.name.startswith("aws-candidate-pilot-"), "wrong pilot origin")
        n["checked"](pilot, args.pilot_sha)
        if args.after_renewed_grant or args.after_native_status:
            n["checked"](pilot, "2030f1b677319c10e96a9928ec40a1274e1d957984b35219c2823ff98769df4b")
            n["checked"](pilot.parent / "grant-lifetimes.private.json", "ec2e09361a95b78e0b8c723a3f0dccfc947ad0fd2a847746c573c120e686ae08")
        normal["stage"]["require_settlement"](w, w["read_json"](pilot))
        require(w["read_json"](pilot.parent / "activation.private.json")["pair_sha256"] == args.pair_sha,
                "pilot used different native pair")
    if args.worker:
        require(os.environ["AIRK_AWS_SERVICE_OWNER"] == str(out), "owned service worker required")
        continuation = args.after_selection_busy
        if args.after_renewed_grant:
            continuation = "renewed"
        if args.after_native_status:
            continuation = "status"
        with normal["capped"](w, out, 2 << 30) as (member, _):
            run(w, owner, out, args.mode, args.pair_proof, args.pair_sha,
                float(os.environ["AIRK_AWS_SERVICE_DEADLINE"]), member, continuation, args.two_lifetimes)
    else:
        require(not out.exists(), "fresh service evidence required")
        wall = min(600 if args.mode == "pilot" else 7200, int(1795194600 - time.time()))
        require(wall > 150, "insufficient campaign time")
        out.mkdir()
        tmp = out / "tmp"
        tmp.mkdir()
        result = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"],
                 n["ROOT"], out / "outer.log", wall, os.environ | {"AIRK_AWS_SERVICE_OWNER": str(out),
                 "AIRK_AWS_SERVICE_DEADLINE": str(time.monotonic() + wall), "TMP": str(tmp), "TEMP": str(tmp)})
        w["write_json"](out / "owned.json", result)
        normal["stage"]["require_settlement"](w, result)


if __name__ == "__main__":
    main()
