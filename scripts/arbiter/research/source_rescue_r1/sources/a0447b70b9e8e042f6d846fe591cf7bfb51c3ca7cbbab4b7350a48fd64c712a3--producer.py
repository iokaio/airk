# SPDX-License-Identifier: Apache-2.0
"""PRIVATE retained Step7 producer. prepare is read-only/native-zero; execute needs Root review.

Uses the immutable candidate's genuine representative_fixture and exact model bytes.
No script fabricates native Stop/ownership/completion, rewrites native manifests, or runs Cargo.
"""
import argparse
import copy
import csv
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import traceback

sys.dont_write_bytecode = True
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"
ACCOUNT = "tyai\\tdotn"
STATE = Path(r"C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\fresh-ann-query-r1")
ROOT = Path(r"D:\code\Github\airk")
CANDIDATE = ROOT / "artifacts/v190/final-plan/candidate-worktree-r1"
EVIDENCE = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"

def need(value, message):
    if not value:
        raise ValueError(message)

def identity():
    need(os.name == "nt", "This producer is prepared only for the verified Windows host")
    account, sid = next(csv.reader(subprocess.check_output(["whoami", "/user", "/fo", "csv", "/nh"], text=True, creationflags=subprocess.CREATE_NO_WINDOW).splitlines()))
    sessions = subprocess.check_output(["quser"], text=True, creationflags=subprocess.CREATE_NO_WINDOW)
    need(account.lower() == ACCOUNT and sid == SID, "Effective native account/SID differs")
    need(re.search(r"(?im)^\s*>?tdotn\s+.*\bActive\b", sessions), "The verified human has no observed active login; stop")
    return {"account": account, "sid": sid, "logged_in_human_observation": sessions}

def unaliased(path):
    path = Path(path).absolute()
    for item in (path, *path.parents):
        meta = item.lstat()
        need(not item.is_symlink() and not getattr(meta, "st_file_attributes", 0) & 0x400, "Aliased input/output ancestry refused: " + str(item))
    return path.resolve(strict=True)

def python_source_workspace(path):
    """Same actual source directory with ordinary Win32 spelling for Python include traversal.

    Rust/native FilePins keep their observed canonical verbatim paths. Python's
    maintained source parser intentionally checks unresolved relative includes;
    Win32 verbatim paths do not normalize their literal .. components.
    """
    supplied=os.fspath(path)
    need(Path(supplied).is_absolute() and not any(part in ('.','..') for part in re.split(r'[\\/]',supplied)), 'Absolute non-dot source workspace required')
    observed=unaliased(supplied)
    text=str(observed)
    if text.startswith('\\\\?\\UNC\\'):
        ordinary=Path('\\\\'+text[8:])
    elif text.startswith('\\\\?\\'):
        ordinary=Path(text[4:])
    else:
        ordinary=observed
    ordinary=unaliased(ordinary)
    need(observed.is_dir() and ordinary.is_dir() and observed.samefile(ordinary), 'Python/native source workspace identity differs')
    return ordinary

def native_path(path):
    """Observe the actual native final path; never assume a verbatim-prefix spelling."""
    path = unaliased(path)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    kernel.CreateFileW.restype = wintypes.HANDLE
    kernel.GetFinalPathNameByHandleW.argtypes = [wintypes.HANDLE, wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD]
    kernel.GetFinalPathNameByHandleW.restype = wintypes.DWORD
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.CreateFileW(str(path), 0, 7, None, 3, 0x02000000, None)
    need(handle not in (None, ctypes.c_void_p(-1).value), "Cannot observe native input handle")
    try:
        count = kernel.GetFinalPathNameByHandleW(handle, None, 0, 0)
        need(0 < count <= 32768, "Native path observation failed")
        buffer = ctypes.create_unicode_buffer(count + 1)
        written = kernel.GetFinalPathNameByHandleW(handle, buffer, len(buffer), 0)
        need(0 < written < len(buffer), "Native path changed while observing")
        return buffer.value
    finally:
        kernel.CloseHandle(handle)

def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def pin(path):
    path = unaliased(path)
    need(path.is_file(), "Expected exact regular pinned file")
    return {"path": native_path(path), "bytes": path.stat().st_size, "sha256": sha(path)}

def check_pin(value):
    path = unaliased(value["path"])
    need(path.stat().st_size == value["bytes"] and sha(path) == value["sha256"], "Frozen input bytes differ: " + str(path))

def write_new(path, value):
    path = Path(path)
    need(unaliased(path.parent).is_relative_to(unaliased(STATE)), "Output escapes the approved private base")
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())

def load_script(path, name):
    specification = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module

def static_inputs():
    freeze = json.loads((STATE / "inputs/static-freeze.json").read_bytes())
    corpus_path, roster_path = STATE / "inputs/cohort.json", STATE / "inputs/native-query-roster.json"
    need(sha(corpus_path) == freeze["cohort_sha256"] and sha(roster_path) == freeze["native_query_roster_sha256"], "Static cohort/roster freeze differs")
    corpus, roster = json.loads(corpus_path.read_bytes()), json.loads(roster_path.read_bytes())
    need(len(corpus["sources"]) == 10 and len(corpus["queries"]) == 9 and len(roster["questions"]) == 9, "Original population differs")
    need("index_control" not in corpus and "compact_plan_probe" not in corpus, "Mutation/probe cannot enter this baseline")
    need([q["id"] for q in corpus["queries"]] == freeze["expected_query_ids"] == [q["id"] for q in roster["questions"]], "Original task order differs")
    need([q["query"] for q in corpus["queries"]] == freeze["expected_query_texts"] == [q["query"] for q in roster["questions"]], "Original query text differs")
    for name, text in corpus["sources"].items():
        need({"bytes":len(text.encode()), "sha256":hashlib.sha256(text.encode()).hexdigest()} == freeze["source_roster"][name], "Original source bytes differ")
    for ordinal, (case, row) in enumerate(zip(corpus["queries"], roster["questions"])):
        need(case["modes"] == ["lexical", "vector", "hybrid"] and "path_prefix" not in case and not case["grades"], "Whole-index ungraded request differs")
        for offset, mode in enumerate(row["modes"]):
            expected = {"query":case["query"], "mode":case["modes"][offset], "limit":10, "max_bytes":65536, "response_mode":"full"}
            need(mode["arguments"] == expected and mode["capture_pointer"] == f"/representative/attempts/{ordinal*3+offset}", "Original preselected request/capture locator differs")
    return freeze, corpus, roster

def prepare(args):
    actor = identity()
    freeze, corpus, roster = static_inputs()
    source = python_source_workspace(args.source)
    quality = load_script(source / "scripts/check-retrieval-quality.py", "fresh_ann_quality")
    runtime = unaliased(args.runtime)
    need(quality.clients.classify_runtime(runtime)["topology"] == "split_pair", "Exact current paired runtime required")
    binary, cli = quality.clients.paired_images(runtime)
    build = json.loads((runtime / "airk-build.json").read_bytes())
    native_source = quality.clients.runtime_builder().source_identity()
    need(native_source == build["source"], "Candidate consumed C7 source differs from immutable runtime build; no source-identity fabrication")
    smoke = build["embedded_model_smoke"]
    catalog = json.loads((source / "crates/airk-retrieval/src/model_catalog.json").read_bytes())
    profile = next(p for p in catalog["profiles"] if p["id"] == smoke["profile"])
    model = unaliased(args.model)
    need(json.loads((model / "model.json").read_bytes()) == profile["manifest"], "Native smoke selected profile differs from exact model view")
    for asset in profile["assets"]:
        observed = pin(model / asset["name"])
        need(observed["bytes"] == asset["bytes"] and observed["sha256"] == asset["sha256"], "Selected catalog asset differs")
    view = json.loads((STATE / "inputs/model-view-receipt.json").read_bytes())
    need(view["profile"] == smoke["profile"] and sha(model / "model.json") == view["rows"][0]["sha256"], "Genuine native generated model.json view differs")
    diagnostic = json.loads(Path(args.diagnostic_receipt).read_bytes())
    image = pin(diagnostic["preserved_executable"])
    need(image["sha256"] == diagnostic["sha256"] and any("fresh_queries_compare_exact_all_live_to_diskann_with_same_embedding" in x for x in diagnostic["selector_inventory"]), "New diagnostic image/selector identity differs; old Kernel image is insufficient")
    name = args.name
    need(re.fullmatch(r"[a-zA-Z0-9_-]+", name), "Simple fresh plan name required")
    full_source = quality.source_identity()
    source_path = STATE / "inputs" / (name + "-source-dirty.json")
    write_new(source_path, full_source)
    recipe = STATE / "inputs/query-recipe.json"
    paths = {
        "cohort": STATE / "inputs/cohort.json", "query_roster": STATE / "inputs/native-query-roster.json",
        "static_freeze": STATE / "inputs/static-freeze.json", "model_view": STATE / "inputs/model-view-receipt.json",
        "source_dirty_manifest":source_path, "runtime_metadata":runtime / "airk-runtime.json",
        "runtime_build":runtime / "airk-build.json", "runtime_image":binary, "cli_image":cli,
        "read_budget_fixture":quality.BUDGET_FIXTURE, "software_v2_policy":Path(args.policy),
        "query_recipe":recipe, "diagnostic_receipt":Path(args.diagnostic_receipt), "diagnostic_image":Path(image["path"]),
        "application_source_bridge":ROOT / "artifacts/v190/final-plan/ann-query-implementation-r1.json",
        "candidate_transfer_bridge":Path(args.transfer_bridge), "candidate_build_receipt":Path(args.build_receipt),
        "producer":Path(__file__),
        "transport_owner":STATE / "execution-owner-r1/windows_job_8g.py",
        "custody_observer":STATE / "execution-owner-r1/custody.py",
        "execution_runner":STATE / "execution-owner-r1/runner.py",
        "transport_owner_controls":STATE / "execution-owner-r1/controls/result.json",
        "reference_owner":ROOT / "artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/reference_owned.py",
        "reference_parent":ROOT / "artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/reference_parent_no_site_r2.py",
        "reference_sid_reader":ROOT / "artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/reference_worker.py",
        **{f"model/{name}":model/name for name in ["model.json","model.onnx","tokenizer.json","LICENSE"]},
    }
    frozen = {key:pin(path) for key,path in paths.items()}
    plan = {"schema":"airk.private.fresh-ann-prepared/1", "execution_policy":{"transport_whole_seconds":1800,"host_committed_bytes":8<<30,"native_phase_deadline_ms":quality.native_read_budget()["operation_deadline_ms"],"transport_owner":"exact maintained multiprocess Job with8GiB and original absolute cutoff; no2process training restriction","native_owner":"unchanged reviewed reference_owned single-child predicate + actual suspended SID/image guard, original native cutoff before launch"}, "actor":actor, "name":name, "source":native_path(source), "source_identity":full_source,
        "candidate_consumed_c7":native_source, "candidate_build":build, "profile":profile, "native_read_budget":quality.native_read_budget(),
        "source_groups_new_diagnostic":diagnostic["source_groups"], "pins":frozen, "fixture":native_path(STATE / "fixture"),
        "model_directory":native_path(model), "tasks":9, "mode_slots":27, "detail":"balanced",
        "source_bridge_scope":"Immutable paired runtime keeps original C7 identity; selected candidate Python harness runs under its real full/dirty source identity. New separately built Kernel diagnostic owns the additive ann_query source/3 controls and never pretends to exist in the old image.",
        "authority":"This preparation supplies no execution permission or native ownership. Root review and same-user external ownership/deadline observer required before execute.", "runtime_or_nn_execution":False}
    output = STATE / "inputs" / (name + "-prepared.json")
    write_new(output, plan)
    print(json.dumps({"prepared":str(output), "sha256":sha(output), "selected_profile":profile["id"], "source_dirty":full_source["dirty"], "tasks":9, "mode_slots":27, "runtime_or_nn_execution":False}))

def execute(args):
    custody = load_script(STATE / "execution-owner-r1/custody.py", "fresh_ann_custody")
    actual_job = custody.require_transport_job()
    phase_deadline = float(os.environ.get("AIRK_FRESH_ANN_ABSOLUTE_DEADLINE", "nan"))
    need(math.isfinite(phase_deadline) and 0 < phase_deadline-time.monotonic() <= 1800, "Original bounded owned transport deadline required before service work")
    actor = identity()
    plan_path = unaliased(args.plan)
    need(sha(plan_path) == args.plan_sha256, "Prepared run SHA differs")
    plan = json.loads(plan_path.read_bytes())
    need(plan["schema"] == "airk.private.fresh-ann-prepared/1", "Prepared run schema differs")
    for value in plan["pins"].values(): check_pin(value)
    freeze, corpus, roster = static_inputs()
    quality = load_script(python_source_workspace(plan["source"]) / "scripts/check-retrieval-quality.py", "fresh_ann_quality")
    need(quality.source_identity() == plan["source_identity"], "Candidate real full/dirty source changed after prepare")
    need(quality.clients.runtime_builder().source_identity() == plan["candidate_consumed_c7"], "Candidate C7 consumed inputs changed")
    need(quality.native_read_budget() == plan["native_read_budget"], "Rust emitted native read budget changed")
    fixture_root = unaliased(plan["fixture"])
    need(fixture_root.is_relative_to(unaliased(STATE)) and not list(fixture_root.iterdir()), "Fixture must remain fresh and empty; never replay")
    output = STATE / "evidence" / (plan["name"] + "-transport.json")
    need(not output.exists(), "Never overwrite/retry an actual capture")
    rows = [{"case":q["id"], "mode":mode, "split":q["split"], "family":q["family"], "trial":0, "input":copy.deepcopy(q), "status":"not_run"} for q in corpus["queries"] for mode in q["modes"]]
    report = {"schema":"airk.private.fresh-ann-transport/1", "plan_sha256":args.plan_sha256, "actor":actor,
        "source":plan["source_identity"], "binary_sha256":plan["pins"]["runtime_image"]["sha256"], "airkctl_sha256":plan["pins"]["cli_image"]["sha256"],
        "read_budget":plan["native_read_budget"], "representative":{"attempts":rows, "scheduled_attempts":27, "complete":False},
        "original_task_denominator":9, "original_mode_denominator":27, "query_roster":roster, "native_events":[],
        "actual_transport_job":actual_job, "original_phase_deadline_monotonic":phase_deadline,
        "quality_claim":False, "score_disposition":"Opaque upstream scorer output retained; empty placeholder grades mean NDCG/recall unavailable. Negative-empty derivatives are NOT negative labels or quality acceptance.",
        "mode_actual_forwards":None, "mode_actual_forwards_status":"Not exposed by existing canonical read APIs; no inferred total"}
    captured = []
    progress_path = STATE / "evidence" / (plan["name"] + "-transport-progress.jsonl")
    progress = progress_path.open("x", encoding="utf-8", newline="\n")
    def event(value):
        json.dump(value, progress, ensure_ascii=False, separators=(",", ":"))
        progress.write("\n")
        progress.flush()
        os.fsync(progress.fileno())
    event({"event":"scheduled", "plan_sha256":args.plan_sha256, "query_roster":roster, "attempts":rows, "actor":actor})
    original = quality.clients.Fixture
    original_open = quality.open_session
    ordinal = 0
    failure_ordinal = None
    class StopCapture(BaseException):
        pass
    def observed_open(service, name):
        nonlocal ordinal, failure_ordinal
        event({"event":"pre_query_native_metadata", "semantic_status":report["representative"].get("semantic_status"), "source_oracle":report["representative"].get("source_oracle")})
        session = original_open(service, name)
        call = session.judged_search
        def capture_once(case, source_hashes):
            nonlocal ordinal, failure_ordinal
            current = ordinal
            need(current < len(rows) and case["id"] == rows[current]["case"] and case["mode"] == rows[current]["mode"] and case["query"] == rows[current]["input"]["query"], "Actual mode/query schedule differs before execution")
            event({"event":"mode_started", "ordinal":current, "case":case, "status":"in_progress"})
            try:
                result = call(case, source_hashes)
            except BaseException as failure:
                failure_ordinal = current
                event({"event":"mode_failed", "ordinal":current, "failure_type":type(failure).__name__, "failure_message":str(failure), "native_attempt":copy.deepcopy(session.last_attempt)})
                # The existing function catches ordinary scoring/read failures;
                # terminate this private batch without consuming later tasks.
                raise StopCapture() from failure
            event({"event":"mode_observed", "ordinal":current, "result":result, "native_attempt":copy.deepcopy(session.last_attempt)})
            ordinal += 1
            return result
        session.judged_search = capture_once
        return session
    class ObservedFixture(original):
        """Only retain actual existing owner results; no replacement behavior or native authority."""
        def __init__(self, *argv, **kwargs):
            super().__init__(*argv, **kwargs)
            self.original_process_handle = None
            captured.append(self)
        def start(self):
            result = super().start()
            need(time.monotonic() < phase_deadline, "Original phase expired during real service startup")
            health = self.managed_health
            need(health is not None, "Actual native managed Health required for service handle custody")
            sid_reader = custody.load_worker(plan["pins"]["reference_sid_reader"]["sha256"]).windows_sid
            self.original_process_handle = custody.OriginalProcess(health["process_id"], {"path":str(self.binary),"sha256":plan["pins"]["runtime_image"]["sha256"]}, SID, sid_reader)
            observed = {"event":"original_service_handle_retained", "actual_native_health":health,"process_proof":dict(self.original_process_handle.proof)}
            report["native_events"].append(observed)
            event(observed)
            return result
        def close(self):
            # Preserve original canonical close exactly; this observer cannot terminate a process.
            result = super().close()
            if self.original_process_handle is not None and self.original_process_handle.handle is not None:
                proof = self.original_process_handle.after_canonical_stop(self.managed_stop, phase_deadline)
                observed = {"event":"original_service_actual_exit_and_handle_close", "process_proof":proof,"actual_native_stop":self.managed_stop}
                report["native_events"].append(observed)
                event(observed)
            return result
        def register(self, *argv, **kwargs):
            result = super().register(*argv, **kwargs)
            observed = {"event":"registered", "projects":copy.deepcopy(self.projects), "scopes":copy.deepcopy(self.scopes)}
            report["native_events"].append(observed)
            event(observed)
            return result
        def job(self, *argv, **kwargs):
            result = super().job(*argv, **kwargs)
            observed = {"event":"native_job_result", "arguments":argv, "keyword_arguments":kwargs, "native_job":copy.deepcopy(result)}
            report["native_events"].append(observed)
            event(observed)
            return result
    error = None
    started = time.monotonic()
    quality.clients.Fixture = ObservedFixture
    quality.open_session = observed_open
    try:
        quality.representative_fixture(Path(plan["pins"]["runtime_image"]["path"]), fixture_root, corpus, plan["pins"]["cohort"]["sha256"], 1, plan["detail"], Path(plan["model_directory"]), report["representative"], Path(plan["pins"]["cli_image"]["path"]))
    except BaseException as failure:
        error = failure.__cause__ if isinstance(failure, StopCapture) else failure
        report["failure"] = {"type":type(error).__name__, "message":str(error), "traceback":traceback.format_exc()}
        if failure_ordinal is not None:
            report["representative"]["attempts"][failure_ordinal].update(status="error", failure_class=type(error).__name__, failure_message=str(error))
    finally:
        quality.clients.Fixture = original
        quality.open_session = original_open
        report["elapsed_seconds"] = time.monotonic()-started
        report["postcheck_errors"] = []
        for key, observe in [("source_after", quality.source_identity), ("binary_sha256_after", lambda:sha(plan["pins"]["runtime_image"]["path"]))]:
            try:
                report[key] = observe()
            except BaseException as failure:
                report[key] = None
                report["postcheck_errors"].append({"field":key,"type":type(failure).__name__,"message":str(failure)})
        report["fixture_preserved"] = str(fixture_root)
        if captured:
            service = captured[0]
            report["actual_native_fixture"] = {"projects":service.projects, "scopes":service.scopes, "health":service.managed_health, "stop":service.managed_stop, "control":str(service.control), "install":str(service.install), "original_process_proof":None if service.original_process_handle is None else dict(service.original_process_handle.proof)}
        report["actual_native_lease_proof"] = "Not fabricated in Python; the later zero-NN Kernel typed pin entry must acquire original ServiceLease and verify actual StopComplete/Health/PID/native ledger/catalog backing."
        report["capture_complete"] = error is None and report["source_after"] == plan["source_identity"] and all(r["status"] == "ok" for r in report["representative"]["attempts"])
        proof=report.get("actual_native_fixture",{}).get("original_process_proof")
        report["original_phase_expired"] = time.monotonic() >= phase_deadline
        report["capture_complete"] = (report["capture_complete"] and not report["postcheck_errors"] and not report["original_phase_expired"] and proof is not None and proof.get("exit_observed") is True and proof.get("handle_closed") is True and proof.get("exit_code") == 0)
        event({"event":"producer_finished", "capture_complete":report["capture_complete"], "failure":report.get("failure"), "actual_native_stop":report.get("actual_native_fixture",{}).get("stop")})
        write_new(output, report)
        progress.close()
    if error is not None: raise error
    need(not report["postcheck_errors"], "Post-capture identity observation failed; original report retained")
    need(len(captured) == 1, "Exactly one original fixture owner required")
    service = captured[0]
    need(service.managed_stop is not None, "Native Stop unavailable; preserve original state")
    need(report["capture_complete"], "Original capture/process-handle/deadline proof incomplete; preserve result without native seed/comparison")
    # Independent pre-query native status supplies publication, never a selected successful ranking.
    publication = report["representative"]["semantic_status"].get("publication")
    need(isinstance(publication, dict), "Original pre-query published status lacks complete native publication")
    registration = service.projects["relevance"]
    selection = service.scopes["relevance"]
    need([r["case"] for r in report["representative"]["attempts"]] == [q["id"] for q in corpus["queries"] for _ in range(3)], "Original schedule changed")
    index_jobs = [event for event in report["native_events"] if event["event"] == "native_job_result" and tuple(event["arguments"][:2]) == ("relevance", "index")]
    need(len(index_jobs) == 1, "Original single native index job receipt missing")
    index_receipt = STATE / "evidence" / (plan["name"] + "-native-index-job.json")
    write_new(index_receipt, index_jobs[0])
    inputs = {key:plan["pins"][key] for key in ["source_dirty_manifest","runtime_metadata","runtime_image","cli_image","query_recipe","software_v2_policy","read_budget_fixture"]}
    inputs["fresh_index_receipt"] = pin(index_receipt)
    seed = {"private_base":native_path(STATE), "project":registration, "control":native_path(service.control),
        "expected_namespace":selection["namespace"], "expected_runtime":service.managed_stop["record"]["request"]["runtime"],
        "expected_index":selection["index_id"], "expected_profile":plan["profile"]["id"], "expected_publication":publication,
        "expected_head":None, "expected_model":None,
        "expected_indexed_sources":{path:[row["bytes"],row["sha256"]] for path,row in freeze["source_roster"].items()},
        "expected_physical_sources":freeze["source_roster"], "model_directory":plan["model_directory"], "query_roster":plan["pins"]["query_roster"],
        "pre_stop_capture":pin(output), "stop_reply":pin(fixture_root / "managed-stop-result.json"), "stop_response_pointer":"",
        "pre_stop_health":pin(fixture_root / "managed-start.json"), "health_pointer":"/health", "identities":inputs}
    seed_path = STATE / "inputs" / (plan["name"] + "-seed.json")
    write_new(seed_path, seed)
    print(json.dumps({"transport":str(output), "transport_sha256":sha(output), "capture_complete":report["capture_complete"], "seed":str(seed_path), "seed_sha256":sha(seed_path), "native_pin_entry_still_required":True, "comparison_executed":False}))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("static-check")
    prep = commands.add_parser("prepare")
    prep.add_argument("--name", default="r1")
    prep.add_argument("--source", type=Path, default=CANDIDATE)
    prep.add_argument("--runtime", type=Path, default=ROOT / "artifacts/arbiter-1.8.207/windows-runtime-debug-r1")
    prep.add_argument("--model", type=Path, default=STATE / "model-view")
    prep.add_argument("--diagnostic-receipt", type=Path, default=EVIDENCE / "native-test-images-ann-r1.json")
    prep.add_argument("--policy", type=Path, default=CANDIDATE / "tools/retrieval-train/software-v2-freeze.json")
    prep.add_argument("--transfer-bridge", type=Path, default=EVIDENCE / "candidate-overlay-preparation-r1/final-transfer-proof-r2.json")
    prep.add_argument("--build-receipt", type=Path, default=EVIDENCE / "windows-matched-debug-build-r1.json")
    run = commands.add_parser("execute")
    run.add_argument("--plan", type=Path, required=True)
    run.add_argument("--plan-sha256", required=True)
    args = parser.parse_args()
    if args.command == "prepare": prepare(args)
    elif args.command == "execute": execute(args)
    else:
        identity()
        freeze, _, _ = static_inputs()
        print(json.dumps({"static_pass":True,"sources":len(freeze["source_roster"]),"tasks":9,"mode_slots":27,"runtime_or_nn_execution":False}))

if __name__ == "__main__":
    main()
