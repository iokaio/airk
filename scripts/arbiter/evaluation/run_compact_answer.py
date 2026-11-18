# SPDX-License-Identifier: Apache-2.0
"""Measure one fixed private Compact Answer through an owned Windows fixture."""

import argparse
from contextlib import contextmanager
import ctypes
from ctypes import wintypes
import re
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
TEST_PREFIX = "service_runtime::evaluation_fixture_tests::"
SELECTORS = {
    "cpu": TEST_PREFIX + "actual_private_arbiter_fixture_software_v2",
    "cuda": TEST_PREFIX + "cuda_answer::actual_private_arbiter_fixture_cuda_answer",
}
HOST_BYTES = 32 << 30
ATTEMPT_SECONDS = 1800
ANSWER_SECONDS = 120
MAX_OBSERVER_BYTES = 64 << 20


def owner_module():
    spec = importlib.util.spec_from_file_location(
        "airk_static_runtime_owner", ROOT / "scripts/provision-static-onnx-runtime.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    with path.open("rb") as source:
        return json.load(source)


def write_json(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def pin(owner, value, artifacts=None):
    path = Path(value["path"])
    require(path.is_absolute(), "input path must be absolute")
    path = owner.plain(path, regular=True)
    require(artifacts is None or path.is_relative_to(artifacts), "private input escapes artifacts")
    require(owner.digest(path) == value["sha256"], f"input bytes changed: {path.name}")
    return path


def prepare(owner, plan_path, expected, output, *, worker=False):
    artifacts = owner.plain(ROOT / "artifacts")
    plan_path = pin(owner, {"path": plan_path, "sha256": expected}, artifacts)
    result = read_json(plan_path)
    require(result["schema"] == "airk.private.paired-compact-answer/1", "unknown Answer plan")
    private = ("answer", "scopes", "corpus", "inputs", "graph", "minilm")
    paths = {
        name: pin(owner, result[name], artifacts if name in private else None)
        for name in ("service", "cli", *private)
        if name in result
    }
    backend = result["backend"]
    require(backend in SELECTORS and ("graph" in paths) == (backend == "cuda"), "wrong backend graph")
    paths["sources"] = owner.plain(paths["corpus"].parent / "sources")
    corpus = read_json(paths["corpus"])
    require(paths["sources"].is_dir(), "shared sources missing")
    require(
        {str(child.relative_to(paths["sources"])) for child in paths["sources"].iterdir()}
        == {row["path"] for row in corpus["files"]},
        "shared corpus directory closure differs",
    )
    for row in corpus["files"]:
        member = paths["sources"] / owner.relative_name(row["path"])
        pin(owner, {"path": member, "sha256": row["sha256"]}, artifacts)
        require(member.stat().st_size == row["bytes"], "shared corpus length differs")
    require(paths["minilm"].name == "model.json", "offline semantic model descriptor missing")
    for part in result["minilm_assets"]:
        member = paths["minilm"].parent / owner.relative_name(part["name"])
        pin(owner, {"path": member, "sha256": part["sha256"]}, artifacts)
    request = read_json(paths["answer"])
    require(
        request["budget"]["input_tokens"] == 8192
        and request["budget"]["deadline_ms"] == 120000
        and request["response_mode"] == "full"
        and request["include_status"] is True,
        "Answer request budget or projection changed",
    )
    require(
        output.is_absolute() and output.is_relative_to(artifacts)
        and output.parent.is_dir() and (output.is_dir() if worker else not output.exists()),
        "private output must have a fresh checked parent under artifacts",
    )
    return result, paths


def ignored(service, selector):
    return [service, "--ignored", "--exact", TEST_PREFIX + selector, "--test-threads", "1"]


def fixture_command(service, backend):
    return [service, "--ignored", "--exact", SELECTORS[backend], "--test-threads", "1"]


def wait_observed(parent, observe, child, deadline, phase, *, file_changes=0x1 | 0x2):
    """Wake on a directory change, actual process exit, or the single deadline."""
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.FindFirstChangeNotificationW.argtypes = [wintypes.LPCWSTR, wintypes.BOOL, wintypes.DWORD]
    api.FindFirstChangeNotificationW.restype = wintypes.HANDLE
    api.FindNextChangeNotification.argtypes = [wintypes.HANDLE]
    api.FindNextChangeNotification.restype = wintypes.BOOL
    api.FindCloseChangeNotification.argtypes = [wintypes.HANDLE]
    api.FindCloseChangeNotification.restype = wintypes.BOOL
    api.WaitForMultipleObjects.argtypes = [
        wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE), wintypes.BOOL, wintypes.DWORD,
    ]
    api.WaitForMultipleObjects.restype = wintypes.DWORD
    change = api.FindFirstChangeNotificationW(str(parent), True, file_changes)
    if not change or change == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        events = (wintypes.HANDLE * 2)(int(child._handle), change)
        while True:
            result = observe()
            if result is not None:
                return result
            left = max(0, math.ceil((deadline - time.monotonic()) * 1000))
            observed = api.WaitForMultipleObjects(2, events, False, left)
            if observed == 0:
                raise ValueError(f"private service exited before {phase} ({child.wait()})")
            if observed == 258:
                raise TimeoutError(f"original {phase} deadline expired")
            if observed != 1 or not api.FindNextChangeNotification(change):
                raise ctypes.WinError(ctypes.get_last_error())
    finally:
        api.FindCloseChangeNotification(change)


def wait_warm(work, service, ready, selected, revision, deadline):
    def observe():
        log = work / "warm-completions.jsonl"
        require(log.stat().st_size <= 8 << 20, "native warm journal exceeded its bound")
        with log.open("rb") as source:
            for line in source:
                if not line.endswith(b"\n"):
                    return None
                row = json.loads(line)
                if row["stage"] == "context":
                    require(
                        row["observer_id"] == ready["warm_observer_id"]
                        and row["profile_manifest"] == selected
                        and row["accepted_configure_revision"] == revision,
                        "native warm completion differs from accepted configuration",
                    )
                    require(row["outcome"] == "ready", f"native Context warm failed: {row['error']}")
                    return row
        return None

    return wait_observed(work, observe, service, deadline, "Context warm completion", file_changes=0x18)


def run_logged(command, folder, label, env, deadline, *, maximum=ANSWER_SECONDS):
    timeout = min(maximum, deadline - time.monotonic())
    require(timeout > 0, f"{label} reached the original deadline")
    stdout_path = folder / f"{label}.stdout"
    stderr_path = folder / f"{label}.stderr"
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        started = time.perf_counter_ns()
        with subprocess.Popen(
            command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr,
            stdin=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW,
        ) as child:
            try:
                code = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
                raise TimeoutError(f"{label} missed its deadline; exact CLI child stopped") from None
        elapsed = time.perf_counter_ns() - started
    write_json(folder / f"{label}.command.json", {
        "argv": [str(part) for part in command], "pid": child.pid,
        "exit_code": code, "elapsed_ns": elapsed,
    })
    require(code == 0, f"{label} failed (exit {code}); inspect private stdout/stderr")
    return stdout_path, elapsed


def tool_body(envelope):
    require(envelope.get("isError") is not True, "native tool returned an error")
    if "structuredContent" in envelope:
        return envelope["structuredContent"]
    content = envelope["content"]
    require(len(content) == 1 and content[0]["type"] == "text", "unexpected tool projection")
    return json.loads(content[0]["text"])


def wait_job(cli, job_id, kind):
    sample = 0
    while True:
        job = cli(f"{kind}-job-{sample}", "jobs", "get", job_id)["job"]
        require(job["id"] == job_id and job["kind"] == kind, "job identity changed")
        if job["state"] == "succeeded":
            require(job["outcome"]["state"] == "succeeded", "index job has no successful outcome")
            return
        require(job["state"] in ("pending", "running"), f"index job {job['state']}")
        sample += 1
        require(sample < 240, "index status observation budget exceeded")
        # Bounded sampling of the actual job record. Index setup precedes timing.
        time.sleep(0.5)


def wait_semantic(cli, index_id):
    for sample in range(240):
        status = tool_body(cli(
            f"published-status-{sample}", "tools", "call", "airk_status", "--index", index_id,
            "--args", '{"response_mode":"full"}',
        ))
        state = status["semantic_readiness"]["state"]
        if state == "ready":
            require(status["publication"]["token"], "semantic index lacks an actual publication")
            return status
        require(state in ("pending", "partial"), f"semantic status became {state}")
        time.sleep(0.5)
    raise TimeoutError("semantic publication observation budget expired")


def observe_scores(owner, folder, manifest):
    summary = read_json(folder / "composed-observer.closed.json")
    require(
        summary["complete"] is True and summary["dropped"] == summary["writer_failed"] == 0,
        "native observer did not close completely",
    )
    log = folder / "composed-observer.jsonl"
    require(log.stat().st_size == summary["bytes"] <= MAX_OBSERVER_BYTES, "observer bytes differ")
    with log.open("rb") as stream:
        records = [json.loads(line) for line in stream]
    require(
        len(records) == summary["records"] > 0
        and records[-1]["sequence"] == summary["last_sequence"],
        "observer record count or sequence differs",
    )
    candidates = {}
    wanted = ("dispatch_admitted", "score_returned", "job_settled", "profile_released", "charge_released")
    binding = ("observer_id", "profile_manifest", "scope", "task", "input_digest",
               "tokens", "question_utf8_sha256", "original_deadline_unix_ms", "charge_identity", "charge_bytes")
    compiled_questions = set()
    for row in records:
        require(row["profile_manifest"] == manifest, "unexpected scoring profile")
        require(row["task"] == "airk.context_action.v1", "unexpected scoring task")
        compiled_questions.add(row["question_utf8_sha256"])
        group = candidates.setdefault(row["dispatch_identity"], [])
        require(
            len(group) < len(wanted) and row["kind"] == wanted[len(group)],
            "native dispatch evidence is incomplete, repeated or out of order",
        )
        require(not group or all(row[k] == group[0][k] for k in binding), "native binding changed")
        group.append(row)
    scored = []
    scopes = []
    for group in candidates.values():
        require(len(group) == len(wanted), "native scoring or resource release did not settle")
        admission, returned, settled, profile_released, charge_released = group
        require(
            profile_released["proof"] == charge_released["proof"]
            == "actual_final_reservation_drop_after_native_scorer_and_source_teardown",
            "native profile and charge release lack settlement proof",
        )
        work = returned["work"]
        require(
            returned["outcome"] == "success" and returned["finite_raw_scores"] is True
            and settled["return_observed"] is True and work is not None
            and work["input_digest"] == admission["input_digest"]
            and work["input_tokens"] == admission["tokens"] and 0 < admission["tokens"] <= 8192
            and work["inference_us"] > 0,
            "native score failed or measured the wrong input",
        )
        scored.append(work)
        scopes.append(admission["scope"])
    require(
        len(compiled_questions) == 1 and re.fullmatch(r"[a-f0-9]{64}", next(iter(compiled_questions))),
        "compiled action question digest differs or is malformed",
    )
    return {
        "work": scored, "scopes": scopes, "observer_sha256": owner.digest(log),
        "compiled_question_sha256": next(iter(compiled_questions)),
    }


@contextmanager
def capped_worker_job(output):
    """Nest memory limits inside the launcher's outer process-settlement Job."""
    class BasicLimits(ctypes.Structure):
        _fields_ = [
            ("process_time", ctypes.c_int64), ("job_time", ctypes.c_int64),
            ("flags", wintypes.DWORD), ("min_working", ctypes.c_size_t),
            ("max_working", ctypes.c_size_t), ("active_limit", wintypes.DWORD),
            ("affinity", ctypes.c_size_t), ("priority", wintypes.DWORD),
            ("scheduling", wintypes.DWORD),
        ]

    class ExtendedLimits(ctypes.Structure):
        _fields_ = [
            ("basic", BasicLimits), ("io_counters", ctypes.c_uint64 * 6),
            ("process_limit", ctypes.c_size_t), ("job_limit", ctypes.c_size_t),
            ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t),
        ]

    api = ctypes.WinDLL("kernel32", use_last_error=True)
    for name, args, restype in (
        ("CreateJobObjectW", (ctypes.c_void_p, wintypes.LPCWSTR), wintypes.HANDLE),
        ("GetCurrentProcess", (), wintypes.HANDLE),
        ("IsProcessInJob", (wintypes.HANDLE, wintypes.HANDLE, ctypes.POINTER(wintypes.BOOL)), wintypes.BOOL),
        ("SetInformationJobObject", (wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD), wintypes.BOOL),
        ("QueryInformationJobObject", (wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p,
                                       wintypes.DWORD, ctypes.c_void_p), wintypes.BOOL),
        ("AssignProcessToJobObject", (wintypes.HANDLE, wintypes.HANDLE), wintypes.BOOL),
        ("CloseHandle", (wintypes.HANDLE,), wintypes.BOOL),
    ):
        function = getattr(api, name)
        function.argtypes, function.restype = args, restype

    def checked(value):
        if not value:
            raise ctypes.WinError(ctypes.get_last_error())
        return value

    process = api.GetCurrentProcess()
    contained = wintypes.BOOL()
    checked(api.IsProcessInJob(process, None, ctypes.byref(contained)))
    require(contained.value, "benchmark worker must already be inside an outer Job")
    job = checked(api.CreateJobObjectW(None, None))
    limits = ExtendedLimits()
    size = ctypes.sizeof(limits)

    def observation():
        checked(api.QueryInformationJobObject(job, 9, ctypes.byref(limits), size, None))
        require(
            limits.basic.flags & 0x300 == 0x300 and limits.process_limit == limits.job_limit == HOST_BYTES,
            "actual process/aggregate Job memory limits differ",
        )
        return {
            "process_limit_bytes": limits.process_limit, "job_limit_bytes": limits.job_limit,
            "peak_process_bytes": limits.peak_process, "peak_job_bytes": limits.peak_job,
        }

    try:
        limits.basic.flags = 0x100 | 0x200  # Process and aggregate commitment.
        limits.process_limit = limits.job_limit = HOST_BYTES
        checked(api.SetInformationJobObject(job, 9, ctypes.byref(limits), size))
        checked(api.AssignProcessToJobObject(job, process))
        checked(api.IsProcessInJob(process, job, ctypes.byref(contained)))
        require(contained.value, "worker missing from the capped memory Job")
        observation()
        try:
            yield
        finally:
            write_json(output / "worker-memory.json", observation())
    finally:
        api.CloseHandle(job)


def execute(owner, plan, paths, output, *, setup_only):
    deadline = time.monotonic() + ATTEMPT_SECONDS
    work = output / "fixture"
    runs = output / "commands"
    runs.mkdir()
    control = work / "control"
    answer = read_json(paths["answer"])
    selected = plan["manifest"]
    env = {
        key: value for key, value in os.environ.items()
        if not key.upper().startswith(("AIRK_ARBITER_EVALUATION_", "AIRK_SELECTED_", "AIRK_NATIVE_DISPATCH_"))
    }
    env.update({
        "AIRK_ARBITER_EVALUATION_INPUTS": str(paths["inputs"]),
        "AIRK_ARBITER_EVALUATION_INPUTS_SHA256": plan["inputs"]["sha256"],
        "AIRK_ARBITER_EVALUATION_WORK": str(work),
        "AIRK_ARBITER_EVALUATION_SECONDS": str(ATTEMPT_SECONDS),
        "AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT": "throughput64gib4threads",
        "AIRK_SELECTED_CONTEXT_CAL_OBSERVER": "false",
        "AIRK_NATIVE_DISPATCH_OBSERVER": "true",
        "AIRK_NATIVE_WARM_OBSERVER": "false" if setup_only else "true",
        "AIRK_MCP_RESPONSE_MODE": "full",
    })
    if "graph" in paths:
        graph = read_json(paths["graph"])
        env.update({
            "AIRK_ARBITER_EVALUATION_CUDA_GRAPH": str(paths["graph"]),
            "AIRK_ARBITER_EVALUATION_CUDA_GRAPH_SHA256": plan["graph"]["sha256"],
            "AIRK_ARBITER_EVALUATION_CUDA_ADAPTER_UUID": json.dumps(graph["adapter_before"]["uuid"]),
        })
    base = [paths["cli"], "--output", "json", "--service-runtime-dir", control]

    def invoke(label, *arguments):
        return run_logged(base + list(arguments), runs, label, env, deadline)

    def cli(label, *arguments):
        return read_json(invoke(label, *arguments)[0])

    command = fixture_command(paths["service"], plan["backend"])
    with (output / "service.log").open("xb") as log:
        service = subprocess.Popen(
            command, cwd=ROOT, env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        write_json(output / "service-command.json", {"argv": [str(part) for part in command], "pid": service.pid})
        try:
            wait_observed(
                output, lambda: True if (work / "ready.json").is_file() else None,
                service, deadline, "service readiness",
            )
            ready = read_json(work / "ready.json")
            require(
                ready["health"]["process_id"] == service.pid
                and ready["health"]["executable_sha256"] == plan["service"]["sha256"]
                and ready["inputs_sha256"] == plan["inputs"]["sha256"]
                and control.samefile(ready["control_root"]),
                "ready receipt differs from the launched fixture",
            )
            (output / "index").mkdir()
            setup_log, _ = invoke(
                "setup", "--root", paths["sources"], "--data-dir", output / "index",
                "library", "setup", "--name", "Compact Answer", "--watch", "off", "--apply",
            )
            with setup_log.open("rb") as setup_output:
                draft, applied = map(json.loads, setup_output)
            require("draft" in draft and "receipt" in applied, "native setup draft/receipt missing")
            setup = applied["receipt"]
            index_id = setup["index_id"]
            project = setup["request"]["registration"]["binding"]
            drafts = output / "semantic-drafts"
            run_logged(
                ignored(paths["service"], "emit_private_selected_vector_configuration"),
                runs, "emit-mini-lm", env | {
                    "AIRK_SELECTED_MINILM_DIRECTORY": str(paths["minilm"].parent),
                    "AIRK_SELECTED_VECTOR_DRAFT_OUTPUT": str(drafts),
                }, deadline,
            )
            job = cli("install-mini-lm", "--service-project", project, "configure",
                      "--file", drafts / "minilm-install.json")["job"]
            wait_job(cli, job["id"], "configure")
            job = cli("index", "--service-project", project, "index")["job"]
            wait_job(cli, job["id"], "index")
            published = wait_semantic(cli, index_id)
            search = tool_body(cli(
                "default-search", "tools", "call", "airk_search", "--index", index_id,
                "--args", json.dumps({
                    "query": answer["question"], "limit": 8, "max_bytes": 4096,
                    "full_chunks": True, "response_mode": "full", "include_status": True,
                }),
            ))
            readiness = search["semantic_readiness"]
            require(
                search.get("hits") and readiness["state"] == "ready"
                and search["publication"]["token"] == published["publication"]["token"]
                and readiness["pipeline_digest"] and search["execution"]["requested_mode"] == "default"
                and search["execution"]["served_mode"] in ("hybrid", "vector")
                and search["execution"]["incomplete"] is False,
                "matched corpus lacks complete default semantic search",
            )

            prefix = "enable-compact"
            configuration_env = env | {
                "AIRK_ARBITER_EVALUATION_SCOPES": str(paths["scopes"]),
                "AIRK_ARBITER_EVALUATION_SCOPES_SHA256": plan["scopes"]["sha256"],
                "AIRK_ARBITER_EVALUATION_PROFILE": selected,
                "AIRK_ARBITER_EVALUATION_STAGE": "context",
                "AIRK_ARBITER_EVALUATION_ENABLED": "true",
                "AIRK_ARBITER_EVALUATION_CONTROL_PREFIX": prefix,
            }
            run_logged(
                ignored(paths["service"], "emit_private_arbiter_configuration"),
                runs, "emit-configuration", configuration_env, deadline,
            )
            if not setup_only:
                configure = cli("configure", "arbiter", "configure", "--legacy",
                                "--file", work / f"{prefix}.configure.json")
                revision = configure["configuration"]["revision"]
                wait_warm(work, service, ready, selected, revision, min(deadline, time.monotonic() + ANSWER_SECONDS))
                status = cli("ready-context", "arbiter", "status-legacy")
                context = status["context"]
                require(
                    status["focus"]["availability"] == "disabled" and context["availability"] == "ready"
                    and context["active_profile"] == context["selected_profile"] == selected,
                    "approved Compact profile is not active while Focus is disabled",
                )
                answer_path, elapsed = invoke(
                    "answer", "tools", "call", "airk_answer", "--index", index_id,
                    "--args-file", paths["answer"],
                )
                envelope = read_json(answer_path)
                packet = tool_body(envelope)
                require(elapsed <= ANSWER_SECONDS * 1_000_000_000, "Answer round trip exceeded 120 seconds")
        finally:
            if service.poll() is None and (work / "ready.json").is_file():
                invoke("shutdown", "service", "shutdown")
        require(service.wait(timeout=max(0, min(ANSWER_SECONDS, deadline - time.monotonic()))) == 0,
                "native fixture process failed to exit naturally")
    closed = read_json(work / "closed.json")
    require(
        closed["instance"] == ready["health"]["instance"]
        and closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True,
        "the same fixture did not prove natural lease release",
    )
    if setup_only:
        require((work / f"{prefix}.configure.json").is_file(), "native exact enrollment draft missing")
        write_json(output / "setup.json", {
            "schema": "airk.private.compact-answer-setup/1", "instance": closed["instance"],
            "index_id": index_id, "pipeline_digest": readiness["pipeline_digest"],
            "publication": search["publication"]["token"], "hit_count": len(search["hits"]),
            "enrollment_sha256": owner.digest(work / f"{prefix}.configure.json"),
            "compact_dispatch_requested": False,
        })
        return
    warm = read_json(work / "warm-completions.closed.json")
    require(warm["complete"] is True and warm["dropped"] == 0 and warm["observer_id"] == ready["warm_observer_id"],
            "native warm observation did not close")
    scores = observe_scores(owner, work, selected)
    require(packet["question"] == answer["question"] and packet["records"], "Answer evidence is missing")
    require(packet["stop"] not in ("deadline", "refused", "stale"), "Answer failed to finish normally")
    assessments = [event["result"] for event in packet["assessment_history"]]
    require(
        len(assessments) == len(scores["work"]) > 0
        and all(value["status"] == "abstained" for value in assessments),
        "observed native scores do not match completed Answer abstentions",
    )
    expected_runtime = (
        "onnxruntime-cuda/1.30.0/device-execution/" if plan["backend"] == "cuda"
        else "onnxruntime-cpu/1.30.0"
    )
    for assessment, score, scope in zip(assessments, scores["work"], scores["scopes"], strict=True):
        advice = assessment["advice"]
        require(
            advice["profile"] == selected and advice["runtime"].startswith(expected_runtime)
            and advice["scope"] == scope and advice["input_digest"] == score["input_digest"]
            and advice["observed_input_tokens"] == score["input_tokens"],
            "Answer advice disagrees with the observed native backend or scored input",
        )
    write_json(output / "measurement.json", {
        "schema": "airk.private.compact-answer-measurement/1", "backend": plan["backend"],
        "answer_round_trip_ns": elapsed, "request_sha256": plan["answer"]["sha256"],
        "response_sha256": owner.digest(answer_path), "corpus_sha256": plan["corpus"]["sha256"],
        "profile_manifest": selected, "instance": closed["instance"],
        "stop": packet["stop"], "runtime": assessments[0]["advice"]["runtime"],
        "search_pipeline": readiness["pipeline_digest"],
        **scores,
    })


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--plan", type=Path, required=True)
    args.add_argument("--plan-sha256", required=True)
    args.add_argument("--output", type=Path, required=True)
    args.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args.add_argument("--check", action="store_true", help="verify exact inputs without a launch")
    args.add_argument("--setup-only", action="store_true", help="validate indexing and search without timing Answer")
    parsed = args.parse_args()
    require(os.name == "nt", "private Answer coordinator requires Windows")
    owner = owner_module()
    output = owner.plain(parsed.output)
    # Only the outer owner creates the checked root. The worker uses it.
    plan, paths = prepare(owner, parsed.plan, parsed.plan_sha256, output, worker=parsed.worker)
    if parsed.worker:
        # An outer-owned process has already created the empty launch root.
        require(os.environ.get("AIRK_PAIRED_ANSWER_OWNED") == str(output),
                "private worker requires a checked outer Job launch")
        with capped_worker_job(output):
            execute(owner, plan, paths, output, setup_only=parsed.setup_only)
        return
    if parsed.check:
        print(json.dumps({"inputs_verified": True, "backend": plan["backend"], "execution": False}))
        return
    output.mkdir()
    handler = Path(__file__).resolve()
    values = os.environ | {"AIRK_PAIRED_ANSWER_OWNED": str(output)}
    source_sha256 = owner.digest(handler)
    owner_sha256 = owner.digest(Path(owner.__file__))
    command = [
        sys.executable, "-B", "-S", handler, "--worker", "--plan", parsed.plan,
        "--plan-sha256", parsed.plan_sha256, "--output", output,
    ]
    if parsed.setup_only:
        command.append("--setup-only")
    result = owner.run_evaluation_fixture(
        command,
        ROOT, output / "outer.log", ATTEMPT_SECONDS, values,
    )
    result["plan_sha256"] = parsed.plan_sha256
    result["source_sha256"] = source_sha256
    result["owner_sha256"] = owner_sha256
    result["source_seals_unchanged"] = (
        owner.digest(handler) == source_sha256 and owner.digest(Path(owner.__file__)) == owner_sha256
    )
    memory = output / "worker-memory.json"
    result["owned_job_memory"] = read_json(memory) if memory.is_file() else None
    result["results_rechecked"] = False
    if result["exit_code"] == 0:
        prepare(owner, parsed.plan, parsed.plan_sha256, output, worker=True)
        result["results_rechecked"] = True
    write_json(output / "owned.json", result)
    require(
        result["exit_code"] == 0 and not result["timed_out"] and not result["log_oversized"]
        and not result["owned_job_forced_termination"] and result["owned_descendants_settled"]
        and result["owned_job_final_active_processes"] == 0 and result["owned_job_final_member_pids"] == []
        and result["source_seals_unchanged"] and result["results_rechecked"]
        and result["owned_job_memory"] is not None
        and result["owned_job_memory"]["process_limit_bytes"] == HOST_BYTES
        and result["owned_job_memory"]["job_limit_bytes"] == HOST_BYTES
        and (output / ("setup.json" if parsed.setup_only else "measurement.json")).is_file(),
        "private measurement failed or did not settle naturally",
    )


if __name__ == "__main__":
    main()
