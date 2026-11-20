# SPDX-License-Identifier: Apache-2.0
"""Measure one qualified installed Windows Answer arm against the sealed exposed timing roster."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys
import time


HERE = Path(__file__).resolve().parent
freeze = runpy.run_path(str(HERE / "freeze_answer_timing.py"))
obs = runpy.run_path(str(HERE / "installed_answer_observations.py"))
require = obs["require"]
E = freeze["E"]
ROSTER = E / "windows-exposed-warm-answer-workload-r1.private.json"
ROSTER_SHA = "326c544a0a89fe4218bb0b80d290b319f89eb0fc3cce0ee4f15e8d713f8f18cd"
INSTALL = Path("C:/Users/tdotn/AppData/Local/AIRK Service/program/install-record.json")
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"


def pin(path, digest):
    raw, _ = freeze["sealed"](path, digest)
    return json.loads(raw)


def write(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, allow_nan=False, sort_keys=True)


def classify_stdout(raw, scope, index, request, focus_profile, arm, elapsed_ns):
    try:
        native = obs["measure_answer"](body(json.loads(raw)),scope,index,request,focus_profile,arm)
        return {"state":obs["native_state"](native,elapsed_ns,request),"native":native}
    except (KeyError,ValueError,TypeError) as exc:
        return {"state":"invalid_native","error":str(exc)}


managed = runpy.run_path(str(HERE / "managed_windows_install.py"))


def install_record(arm):
    return managed["record_path"](arm,E,INSTALL)


def installed(arm, observation=None):
    original = install_record(arm)
    record = pin(original, arm["install_record_sha256"])
    images = {}
    for name in ("airk", "airkctl"):
        image = record["executables"][name]
        path = (original.parent / image["path"]).resolve(strict=True)
        require(path.is_relative_to(original.parent) and path.stat().st_size == image["bytes"], "foreign installed image")
        freeze["sealed"](path, image["sha256"])
        images[name] = path
    if "managed_install" in arm:
        proof = managed["verify"](arm,E,record,original.parent,images)
        if observation is not None:
            observation.update(proof)
    return images["airkctl"], record["executables"]["airk"]["sha256"]


def registration_receipt(raw):
    try:
        doc = json.loads(raw)
        registration = doc.get("receipt", doc)
    except json.JSONDecodeError:
        draft, applied = [json.loads(line) for line in raw.splitlines() if line.strip()]
        registration = applied["receipt"]
        require("draft" in draft and draft["request_digest"] == registration["request_digest"],
                "original registration draft and application differ")
    require(registration["schema"] == 1 and registration["stage"] == "ready", "registration did not reach native ready state")
    return registration


def bind_arm(arm, roster):
    require(arm["provider"] in ("cpu", "cuda") and arm["model"] in ("context_compact", "context_large"),
            "original Windows CPU or CUDA Context required")
    require(arm["runtime"] == "onnxruntime-" + arm["provider"] + "/1.30.0", "runtime does not match requested provider")
    desc = pin(Path(arm["profile_path"]), arm["profile_sha256"])
    cal = pin(Path(arm["calibration_path"]), arm["calibration_sha256"])
    calibration_asset, = [a for a in desc["assets"] if a["role"] == "calibration" and a["sha256"] == arm["calibration_sha256"]]
    require(calibration_asset["bytes"] == Path(arm["calibration_path"]).stat().st_size
            and cal["task_policy"] == desc["task_policies"]["airk.context_action.v1"]
            and cal["score_abi"] == desc["abi"] and cal["schema"] == "airk.arbiter.context-action.calibration/2",
            "joint calibration task, ABI or asset epoch differs")
    require(desc["manifest"] == arm["context_manifest"] and desc["runtime"] == arm["runtime"] and desc["qualification"]
            and desc["calibration"] == arm["calibration_sha256"] and "airk.context_action.v1" in desc["tasks"]
            and cal["compiler"] == desc["compiler"] and cal["model"] == desc["model"] and cal["rules"],
            "qualified descriptor and operative joint calibration required")
    control = Path(arm["control"])
    require(control.resolve(strict=True) == control and all(not p.is_junction() and not p.is_symlink()
            for p in (control, *control.parents)), "noncanonical product control")
    ids = arm["indexes"]
    require(set(ids) == {r["case"] for r in roster["cases"]} and len(set(ids.values())) == 13
            and all(re.fullmatch(r"idx_[0-9a-f]{32}", v) for v in ids.values()), "isolated 13-menu index binding required")
    for row in roster["cases"]:
        spec = arm["registrations"][row["case"]]
        raw, _ = freeze["sealed"](Path(spec["path"]), spec["sha256"])
        registration = registration_receipt(raw)
        require(registration["index_id"] == ids[row["case"]]
                and Path(registration["request"]["registration"]["root"]).samefile(Path(row["source_root"])),
                "registered index or original source root differs")
    return control


class CurrentCLI:
    def __init__(self, cli, control, output):
        self.prefix = [str(cli), "--output", "json", "--service-runtime-dir", str(control)]
        self.output = output
        self.env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "ORT_", "RUST_LOG", "CUDA_"))}

    def call(self, label, args, timeout=120):
        argv = self.prefix + [str(v) for v in args]
        log = self.output / (label + ".stdout")
        with log.open("xb") as stdout, (self.output / (label + ".stderr")).open("xb") as stderr:
            start = time.perf_counter_ns()
            with subprocess.Popen(argv, cwd=HERE, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                  env=self.env, creationflags=subprocess.CREATE_NO_WINDOW) as child:
                try:
                    code = child.wait(timeout=max(0, timeout - (time.perf_counter_ns() - start) / 1e9))
                    state = "observed" if code == 0 else "cli_error"
                except subprocess.TimeoutExpired:
                    child.kill()
                    code = child.wait()
                    state = "timeout"
                end = time.perf_counter_ns()
        meta = {"argv": argv, "start_ns": start, "end_ns": end, "elapsed_ns": end-start, "pid": child.pid,
                "exit_code": code, "state": state, "stdout": str(log), "stdout_sha256": hashlib.sha256(log.read_bytes()).hexdigest()}
        write(self.output / (label + ".command.json"), meta)
        return meta

    def read(self, label, args):
        meta = self.call(label, args)
        require(meta["state"] == "observed", label + " failed")
        return pin(Path(meta["stdout"]), meta["stdout_sha256"])

    def tool(self, label, name, index, payload):
        env = self.read(label, ["tools", "call", name, "--index", index, "--args",
                                json.dumps(payload | {"response_mode":"full", "include_status":True})])
        return body(env)


def body(envelope):
    require(envelope.get("isError") is not True, "native tool error")
    if "structuredContent" in envelope:
        return envelope["structuredContent"]
    text, = envelope["content"]
    require(text["type"] == "text", "foreign native transport")
    return json.loads(text["text"])


def source(cli, row, index):
    case = row["case"]
    status = cli.tool(case+"-status", "airk_status", index, {})
    require(status["files"] == 50 and status["discovery_readiness"]["state"] == status["semantic_readiness"]["state"] == "ready"
            and status["read_scope"] == {"source":"named","index_ids":[index]}, "unready original named publication")
    def same(label, name, args):
        result = cli.tool(case+"-"+label, name, index, args)
        require(result["publication"]["token"] == status["publication"]["token"]
                and result["scope_observation"] == status["scope_observation"], "native publication or grant changed")
        return result
    listing = same("list", "airk_list_files", {"prefix":"", "limit":100, "max_bytes":65536})
    docs = row["ordered_source_files"]
    require(not listing["truncated"] and not listing.get("cursor") and len(listing["paths"]) == 50
            and set(listing["paths"]) == {d["file"] for d in docs}, "partial or foreign native menu")
    for i, doc in enumerate(docs):
        info = same("info-"+str(i), "airk_file_info", {"path":doc["file"]})
        require(info["path"] == doc["file"] and info["content_hash"] == doc["sha256"] and info["source_bytes"] == doc["bytes"],
                "native menu member differs from original source")
    query = same("pipeline", "airk_search", {"query":"source","limit":1,"max_bytes":4096,"full_chunks":True})
    require(query["semantic_readiness"]["pipeline_digest"] == row["pipeline"] and not query["execution"]["incomplete"]
            and query["execution"]["served_mode"] in ("hybrid","vector"), "MiniLM pipeline or published search differs")
    return status


def run(arm, arm_sha, output):
    roster = pin(ROSTER, ROSTER_SHA)
    require(roster == freeze["freeze"](), "original question or source freeze changed")
    control = bind_arm(arm, roster)
    require("managed_install" in arm, "a Windows campaign run requires the approved D installation")
    installation = {}
    cli_image, service_sha = installed(arm,installation)
    write(output / "runtime-inspect.json", installation)
    install_bytes, _ = freeze["sealed"](install_record(arm), arm["install_record_sha256"])
    with (output / "install-record.json").open("xb") as retained:
        retained.write(install_bytes)
    cli = CurrentCLI(cli_image, control, output)
    cases = {row["case"]:row for row in roster["cases"]}
    order = roster["warm_sample_order"]
    rows = [{"ordinal": i, "case": case, "request_sha256": cases[case]["request_sha256"], "state":"deferred", "elapsed_ns":None}
            for i,case in enumerate(order)]
    report = {"roster_sha256":ROSTER_SHA,"arm_sha256":arm_sha,"model":arm["model"],"provider":arm["provider"],
              "install_record_sha256":arm["install_record_sha256"],"context_manifest":arm["context_manifest"],
              "calibration_sha256":arm["calibration_sha256"], "rows":rows, "cold_load":None, "warmup":None}
    def health(label):
        svc = cli.read(label, ["service","status","--runtime-dir",control])["service"]
        require(svc["state"] == "running" and not svc["explicitly_stopped"] and svc["pending_lifecycle"] is None
                and svc["health"]["executable_sha256"] == service_sha and svc["health"]["user"] == SID,
                "wrong running installed product image, lease or account")
        return svc["health"]["instance"]
    def stage(label):
        status = cli.read(label, ["arbiter","status"])
        return obs["stages"](status, arm["indexes"].values(), arm, cases[order[0]]["focus_profile"])
    try:
        instance = health("service-before")
        active = stage("stage-before")
        scopes = {case:source(cli, row, arm["indexes"][case]) for case,row in cases.items()}
        report.update(service_instance=instance, active=active, scopes=scopes)
        def answer(label, case):
            row = cases[case]
            request = pin(Path(row["request"]), row["request_sha256"])
            index = arm["indexes"][case]
            meta = cli.call(label, ["tools","call","airk_answer","--index",index,"--args-file",row["request"]],
                            timeout=request["budget"]["deadline_ms"]/1000)
            result = {"elapsed_ns":meta["elapsed_ns"],"state":meta["state"],"command":label+".command.json"}
            if meta["state"] == "observed":
                raw, _ = freeze["sealed"](Path(meta["stdout"]),meta["stdout_sha256"])
                result.update(classify_stdout(raw,scopes[case],index,request,row["focus_profile"],arm,meta["elapsed_ns"]))
            return result
        report["warmup"] = answer("untimed-warmup",order[0])
        require(report["warmup"]["state"] == "observed", "original uncounted warm-up failed")
        require(stage("after-warmup") == active, "warm-up changed native ready state")
        for item in rows:
            i, case = item["ordinal"], item["case"]
            item.update(answer(f"warm-{i:02}", case))
            write(output / f"observed-{i:02}.json", item)
            require(item["state"] in ("observed","timeout"), "native scope, response or invocation failed")
            require(stage(f"after-{i:02}") == active, "native availability changed during workload")
        require(health("service-after") == instance, "installed service instance changed")
        report["terminal"] = "settled"
    except (OSError, ValueError, KeyError, TypeError) as exc:
        report.update(terminal="refused", error=str(exc))
    report["summary"] = obs["summary"](rows, order)
    write(output / "arm.json", report)
    require(report["terminal"] == "settled", "arm refused, inspect original command receipts")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arm", type=Path, required=True)
    parser.add_argument("--arm-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "checked isolated Windows execution required")
    require(args.output.resolve(strict=True) == args.output and args.output.parent == E and not list(args.output.iterdir()),
            "fresh checked campaign output required")
    run(pin(args.arm,args.arm_sha256),args.arm_sha256,args.output)
