# SPDX-License-Identifier: Apache-2.0
"""Measure native project/client load in owned scopes, including explicit overload."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import re
import threading
import time
import uuid


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


clients = module("airk_load_clients", "check-service-clients.py")
retrieval = module("airk_load_sessions", "check-retrieval-quality.py")
http_fixture = module("airk_load_http", "http_fixture.py")
processes = module("airk_load_processes", "owned-runtime-processes.py")
int8_policy = module("airk_load_int8_policy", "int8-policy.py")


def error_observation(error):
    value = {"type": type(error).__name__, "message": str(error)[:2048]}
    if isinstance(error, http_fixture.HttpResponseError):
        value["http_response"] = error.evidence()
    value["outcome"] = http_fixture.error_outcome(error)
    return value


def immutable(value):
    if isinstance(value, dict):
        return {key: immutable(item) for key, item in value.items() if key != "expected_catalog_revision"}
    if isinstance(value, list):
        return [immutable(item) for item in value]
    return value


def enroll(fixture, name):
    selected = {"kind": "scoped", "selection": fixture.scopes[name]}
    identity = uuid.uuid4().hex  # Each independently initialized client owns a credential.
    connection = fixture.enroll_http(selected, identity, "unknown")
    assert connection["connection_id"] == identity and immutable(connection["selection"]) == immutable(selected)
    health = fixture.rpc("health")["observation"]
    http_fixture.validate_native_identity(connection, health)
    assert health["instance"] == fixture.managed_health["instance"]
    return connection


class HttpSession:
    """One real initialized HTTPS session; no frontend process or token artifact."""
    # Native listener permits 32 concurrent TLS handshakes. Keep transport
    # establishment bounded without serializing the 64 clients' actual reads.
    handshake_slots = threading.BoundedSemaphore(16)
    def __init__(self, connection, control, observer=None):
        self.network = http_fixture.McpClient(connection, control)
        self.sequence = 0
        self.closed = False
        self.uncertain = False
        self.delete_completed = False
        self.observer = observer
        try:
            self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                       "clientInfo": {"name": "airk-service-load", "version": "1"}})
            self.network.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            assert self.network.session is not None, "Native MCP session identity missing"
            self.session_sha256 = hashlib.sha256(self.network.session.encode()).hexdigest()
        except BaseException as primary:
            self.uncertain = not isinstance(primary, http_fixture.HttpResponseError) or primary.submission == "uncertain"
            self.__exit__(type(primary), primary, primary.__traceback__)
            raise

    def request(self, method, parameters):
        if self.closed or self.uncertain:
            raise RuntimeError("Closed or uncertain benchmark exchange cannot be replayed")
        # MCP sessions outlive individual TCP connections. Native HTTP stops
        # waiting for a new header after 15 s; source/vector jobs and sequential
        # cohort warmup can exceed that interval. Start each NEW exchange on
        # fresh TCP, keeping the original credential, session and TLS checks.
        # This is never an exception-triggered replay of a submitted request.
        fresh_tcp = self.sequence != 0
        self.sequence += 1
        try:
            if fresh_tcp:
                self.network.connection.close()
                with self.handshake_slots:
                    self.network.connection.connect()
            self.network.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": parameters})
            reply = self.network.receive(self.sequence)
            if "error" in reply:
                raise self.network.response_error(200, json.dumps(reply).encode(), [], "POST",
                    {"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": parameters},
                    "/mcp", failure="jsonrpc_error")
            return reply["result"]
        except http_fixture.HttpResponseError as error:
            self.uncertain = error.submission == "uncertain"
            raise
        except BaseException:
            self.uncertain = True
            raise

    def close(self):
        if not self.closed:
            try:
                if not self.uncertain:
                    self.network.connection.close()  # DELETE also gets fresh TCP.
                    with self.handshake_slots:
                        self.network.connection.connect()
                self.network.close(delete=not self.uncertain)
                self.delete_completed = not self.uncertain
            finally:
                self.closed = True  # McpClient.close always closes its socket.

    def __exit__(self, error_type, primary, traceback):
        cleanup_error = None
        cleanup_failure = None
        try:
            self.close()
        except BaseException as cleanup:
            cleanup_failure = cleanup
            cleanup_error = error_observation(cleanup)
            if primary is None:
                raise
            primary.add_note("Owned benchmark session cleanup also failed: " + str(cleanup_error))
        finally:
            if self.observer is not None:
                try:
                    self.observer({"socket_closed": self.closed, "delete_completed": self.delete_completed,
                                   "uncertain_exchange": self.uncertain, "cleanup_error": cleanup_error})
                except BaseException as recording:
                    retained_error = primary if primary is not None else cleanup_failure
                    if retained_error is None:
                        raise
                    retained_error.add_note("Benchmark cleanup evidence could not be written: " + type(recording).__name__)
        return False


class CaseEvidence:
    """Retain completed phases and the primary failure even when no case finishes."""
    def __init__(self, path, projects, client_count, rounds, semantic):
        self.path = path
        self.value = {"schema": "airk-service-load-case/1", "projects": projects, "clients": client_count,
                      "rounds": rounds, "semantic": semantic, "events": [], "phases": {}, "passed": False}
        self.lock = threading.Lock()
        self.phase_outcomes = {}

    def plan_phase(self, phase, planned):
        with self.lock:
            self.phase_outcomes[phase] = []
            self.value["phases"][phase] = http_fixture.phase_counts([], planned)

    def record_outcome(self, phase, outcome, code=None):
        with self.lock:
            self.phase_outcomes[phase].append({"outcome": outcome, "code": code})
            planned = self.value["phases"][phase]["planned"]
            self.value["phases"][phase] = http_fixture.phase_counts(self.phase_outcomes[phase], planned)

    def exchange(self, phase, work, *, accept_refusal=False):
        try:
            result = work()
        except BaseException as error:
            refusal = http_fixture.classify_refusal(error)
            self.record_outcome(phase, http_fixture.error_outcome(error), refusal["code"] if refusal else None)
            self.note(phase + "_error", error=error_observation(error))
            if accept_refusal and refusal is not None:
                return None
            raise
        self.record_outcome(phase, "succeeded")
        return result

    def __enter__(self):
        if self.path is not None:
            with self.path.open("x", encoding="utf-8") as output:
                json.dump(self.value, output, indent=2)
        return self

    def note(self, phase, **data):
        with self.lock:
            self.value["events"].append({"phase": phase, **data})
            if self.path is not None:
                self.path.write_text(json.dumps(self.value, indent=2) + "\n", encoding="utf-8")

    def __exit__(self, error_type, primary, traceback):
        self.value["passed"] = (primary is None and self.value.get("qualified", True)
                                and all(phase["passed"] for phase in self.value["phases"].values()))
        if primary is not None:
            self.value["failure"] = {**error_observation(primary), "notes": getattr(primary, "__notes__", [])}
        try:
            self.note("complete" if primary is None else "failed")
        except BaseException as recording:
            if primary is None:
                raise
            primary.add_note("Benchmark phase evidence could not be written: " + type(recording).__name__)
        return False


def process_observation(fixture):
    """Observe exact approved same-main images, not names or whole-machine roles."""
    try:
        backend = processes._backend()
    except RuntimeError:
        return {"available": False, "reason": "held_identity_backend_unavailable", "global_completeness": "not_certified"}
    path = fixture.binary.resolve(strict=True)
    expected = hashlib.sha256(path.read_bytes()).hexdigest()
    health = fixture.rpc("health")["observation"]
    assert health["executable_sha256"] == expected
    pids = backend.pids()
    assert len(pids) <= processes.MAX_SCANNED
    rows, unobserved = [], 0
    for pid in pids:
        owner = None
        try:
            owner = backend.open(pid)
            if owner.gone():
                continue
            row = owner.observation()
            if processes._key(row["path"]) != processes._key(path):
                continue
            assert row["user"] == backend.user and not row.get("image_deleted"), "Owned benchmark process identity changed"
            assert processes._digest(path) == expected
            assert row == owner.observation()
            if not owner.gone():
                rows.append({**row, "executable_sha256": expected})
                assert len(rows) <= processes.MAX_MATCHES
        except (OSError, ProcessLookupError):
            unobserved += 1
        finally:
            if owner is not None:
                owner.close()
    assert health["process_id"] in {row["pid"] for row in rows}, "Authenticated native service PID not observed"
    return {"available": True, "same_main_processes": rows, "enumerated": len(pids), "unobserved": unobserved,
            "global_completeness": "not_certified", "role_classification": "not_inferred_from_image_names",
            "private_physical_memory_and_cpu": "not_measured",
            "executable_sha256_basis": "verified_owned_path_file_bytes_not_mapped_process_memory"}


def snapshot(fixture, include_processes=True, observer=None):
    resources, provider = fixture.rpc("resources"), fixture.rpc("embedding_status")
    if observer is not None:
        observer({"resources": resources, "embedding_status": provider})
    assert type(provider.get("loads")) is int and provider["loads"] >= 0, "Native provider load count unavailable"
    if fixture.single_image and provider.get("provider") is not None:
        observation = provider["provider"]
        identity = observation["provider"]
        assert observation["schema"] == 1 and observation["kind"] == "in_process"
        assert identity["service_instance"] == fixture.managed_health["instance"]
        assert identity["approved_execution_sha256"] == fixture.managed_health["executable_sha256"]
        assert provider["process_id"] is None, "In-process provider unexpectedly reports a helper PID"
    return {"resources": resources, "embedding_status": provider,
            "processes": process_observation(fixture) if fixture.single_image and include_processes else {"available": False, "reason": "not_sampled_at_this_boundary" if fixture.single_image else "historical_fixture"}}


def retained_snapshot(fixture, journal, phase, include_processes=False):
    # Save native replies before snapshot's schema/identity checks, and save the
    # complete observation before callers assert workload invariants.
    observed = snapshot(fixture, include_processes=include_processes,
                        observer=lambda raw: journal.note(phase + "_raw", native=raw))
    journal.note(phase, native=observed)
    return observed


def semantic_pipeline(runtime):
    receipt = json.loads((runtime / "airk-runtime.json").read_text())
    assert receipt.get("schema") == 2, "Semantic shared-owner measurement requires the modern single image"
    smoke = json.loads((runtime / "airk-build.json").read_text()).get("embedded_model_smoke", {})
    pipeline = smoke.get("pipeline_digest")
    assert (smoke.get("source") == "embedded" and smoke.get("engine") == "onnxruntime-cpu/1.22.1"
            and smoke.get("dimensions") == 384 and smoke.get("repeat_equal") is True
            and smoke.get("profile") in int8_policy.APPROVED
            and isinstance(smoke.get("legacy_identity"), str) and re.fullmatch("[0-9a-f]{64}", smoke["legacy_identity"])
            and isinstance(pipeline, str) and re.fullmatch("[0-9a-f]{64}", pipeline)), "Approved embedded MiniLM pipeline receipt missing"
    return pipeline


def semantic_result(value, pipeline, require_ready=True):
    execution = value.get("execution", {})
    readiness = clients.semantic_readiness(value)
    assert (execution.get("requested_mode") == "vector" and execution.get("served_mode") == "vector"
            and type(execution.get("incomplete")) is bool), "Explicit semantic read fell back or omitted its execution contract"
    assert readiness.get("pipeline_digest") == pipeline, "Semantic read used another embedding pipeline"
    assert (type(readiness.get("selected_artifacts")) is int and readiness["selected_artifacts"] == 16
            and readiness["completed_artifacts"] <= readiness["selected_artifacts"]), "Semantic coverage differs from the original fixture"
    complete = (readiness["state"] == "ready" and readiness["completed_artifacts"] == readiness["selected_artifacts"]
                and not execution["incomplete"])
    if require_ready:
        assert complete, "Settled semantic read did not retain complete readiness"
    return complete


def semantic_provider(observed):
    status = observed["embedding_status"]
    owner = status.get("provider")
    assert (status.get("available") is True and status.get("process_id") is None
            and isinstance(owner, dict) and owner.get("schema") == 1 and owner.get("kind") == "in_process"
            and owner.get("admission_open") is True
            and owner.get("failure") is None and status["loads"] > 0), "Actual shared in-process MiniLM owner unavailable"
    return owner["provider"]


def stable_semantic_owner(baseline, observed):
    assert semantic_provider(observed) == semantic_provider(baseline), "Semantic workload replaced its inference owner"
    assert observed["embedding_status"]["loads"] == baseline["embedding_status"]["loads"], "Semantic clients loaded additional model copies"


def post_first_warmup_baseline(connected, observed):
    """Separate a proven idle-cache reload from loads caused by later clients."""
    assert semantic_provider(observed) == semantic_provider(connected), "First semantic client replaced its inference owner"
    old, new = connected["embedding_status"], observed["embedding_status"]
    owner, warmed = old["provider"], new["provider"]
    zero = lambda value: type(value) is int and value == 0
    idle = (zero(old.get("sessions")) and zero(owner.get("model_references_outstanding"))
            and all(zero(owner.get(key)) for key in
                    ("queued_outstanding", "running_outstanding", "callback_outstanding", "delivery_outstanding")))
    assert type(warmed.get("model_references_outstanding")) is int and warmed["model_references_outstanding"] == 1, "First semantic read did not leave one cached model reference"
    assert type(old["loads"]) is int and type(new["loads"]) is int
    expected = old["loads"] + (1 if idle else 0)
    assert new["loads"] == expected, "First semantic client did not match the observed cached/uncached load boundary"
    return {"kind": "observed_uncached_reload" if idle else "no_reload", "loads_before": old["loads"],
            "loads_after": new["loads"], "idle_uncached_proven": idle,
            "policy": "One existing warmup read; exactly one load only after observed uncached/no-outstanding state. Later clients retain this owner and load count."}


def prepare_semantic(fixture, names, pipeline, journal=None):
    before = (retained_snapshot(fixture, journal, "semantic_setup_before") if journal is not None
              else snapshot(fixture, include_processes=False))
    jobs = []
    for name in names:
        setup = fixture.job(name, "setup-model")
        configured = fixture.configure(name, {"operation": "vectors", "enabled": True})
        assert setup["state"] == configured["state"] == "succeeded"
        assert setup["project"] == configured["project"] == fixture.projects[name]["binding"]
        accepted = fixture.enqueue(name, "vectors")  # Submit each mutation once.
        completed = fixture.wait(accepted)
        assert completed["id"] == accepted["job"]["id"] and completed["project"] == fixture.projects[name]["binding"]
        deadline = time.monotonic() + 600
        while True:
            status = fixture.published_status(name, deadline)
            readiness = clients.semantic_readiness(status)
            assert readiness["state"] != "failed", "Semantic fixture preparation explicitly failed"
            if readiness["state"] == "ready" and readiness["completed_artifacts"] == readiness["selected_artifacts"] == 16:
                assert readiness.get("pipeline_digest") == pipeline, "Prepared vectors differ from approved MiniLM"
                break
            assert time.monotonic() < deadline, "Semantic fixture readiness deadline exhausted"
            time.sleep(.05)
        jobs.append({"project":name,"model_setup":setup,"vectors_configuration":configured,
                     "vectors_accepted":accepted,"vectors_completed":completed,"ready_status":status})
    after = (retained_snapshot(fixture, journal, "semantic_setup_after", include_processes=True) if journal is not None
             else snapshot(fixture))
    semantic_provider(after)
    # Reviewed index setup may already have warmed the same pipeline before
    # vectors are explicitly disabled for the fixture's initial lexical scan.
    # Preserve that observation instead of requiring a redundant model load.
    assert before["embedding_status"]["loads"] in (0, 1) and after["embedding_status"]["loads"] == 1, "Fresh same-pipeline fixture did not retain one cumulative MiniLM load"
    return {"pipeline_digest":pipeline,"jobs":jobs,"before":before,"after":after,
            "model_load_delta":after["embedding_status"]["loads"]-before["embedding_status"]["loads"]}


def request_attempt(session, tool, arguments):
    """Count a complete known preadmission refusal; never retry an exchange."""
    started = time.perf_counter()
    try:
        envelope = session.request("tools/call", {"name": tool, "arguments": arguments})
    except http_fixture.HttpRefused as error:
        refusal = http_fixture.classify_refusal(error)
        if refusal is None:
            raise
        return {"rejected_ms": (time.perf_counter()-started)*1000,
                "http_status": error.status, "body": error.body, "http_response": error.evidence(),
                "admission_stage": refusal["stage"], "admission_code": refusal["code"]}
    return {"elapsed_ms": (time.perf_counter()-started)*1000, "envelope": envelope}


def http_admission_counts(replies):
    """Count observed refusals separately; none are successful read samples."""
    counts = {}
    for reply in replies:
        if "admission_stage" in reply:
            key = (reply["admission_stage"], reply["admission_code"], reply["http_status"])
            counts[key] = counts.get(key, 0) + 1
    return [{"stage": stage, "code": code, "http_status": status, "count": count}
            for (stage, code, status), count in sorted(counts.items())]


def latency(samples):
    ordered = sorted(samples)
    return {"samples": len(ordered), "p95_ms": ordered[math.ceil(len(ordered) * .95) - 1],
            "maximum_ms": ordered[-1]} if ordered else {"samples": 0}


def search_arguments(project, semantic=False):
    return {"query": f"projectmarker{project}", "mode": "vector" if semantic else "lexical", "limit": 1,
            "max_bytes": 4096, "verification": "off", "response_mode": "full"}


def campaign(runtime, project_count, client_count, rounds, semantic=False, evidence_path=None):
    pipeline = semantic_pipeline(runtime) if semantic else None
    with CaseEvidence(evidence_path, project_count, client_count, rounds, semantic) as journal, \
            clients.owned_client_fixture() as temporary, ExitStack() as stack:
        for phase, planned in (("initialize", client_count), ("semantic_warmup", client_count if semantic else 0),
                               ("first", client_count), ("idle", client_count*rounds),
                               ("during", client_count*rounds), ("settled", client_count*rounds),
                               ("parity", min(project_count, client_count)), ("client_close", client_count)):
            journal.plan_phase(phase, planned)
        owner = Path(temporary).resolve(strict=True)
        fixture = stack.enter_context(clients.Fixture(runtime, owner, job_timeout=600))
        journal.note("native_started", owner=str(owner), health=fixture.managed_health)
        data = owner / "data"
        data.mkdir()
        originals = {}
        source_hashes = {}
        names = [f"load-{number}" for number in range(project_count)]
        for number, name in enumerate(names):
            root = owner / name
            root.mkdir()
            for index in range(16):
                path = root / f"file-{index}.txt"
                path.write_bytes((f"projectmarker{number} Original bounded load fixture {index}.\n" * 32).encode())
                originals[path] = path.read_bytes()
                source_hashes[path] = {hashlib.sha256(originals[path]).hexdigest()}
            fixture.register(name, root, data)
            fixture.rpc({"configure_watch": {"project": fixture.projects[name]["binding"], "enabled": False}})
            fixture.configure(name, {"operation": "vectors", "enabled": False})
            fixture.configure(name, {"operation": "usage", "mode": "disabled"})
            fixture.job(name, "index")
        original_instance = fixture.rpc("health")["observation"]["instance"]
        semantic_setup = prepare_semantic(fixture, names, pipeline, journal=journal) if semantic else None
        before = semantic_setup["after"] if semantic else retained_snapshot(fixture, journal, "prepared_native", include_processes=True)
        journal.note("prepared", semantic_setup=semantic_setup, native=before)
        sessions, initialize_ms, enrollment_ms = [], [], []
        connections = []
        if fixture.single_image:
            for index in range(client_count):
                started = time.perf_counter()
                connections.append(enroll(fixture, names[index % project_count]))
                enrollment_ms.append((time.perf_counter() - started) * 1000)
            assert len({info["connection_id"] for info in connections}) == client_count
        def initialize(index):
            started = time.perf_counter()
            name = names[index % project_count]
            session = journal.exchange("initialize", lambda: (
                HttpSession(connections[index], fixture.control,
                            observer=lambda value: journal.note("client_cleanup", client=index, observation=value)) if fixture.single_image
                else retrieval.Session([str(fixture.binary), *fixture.arguments(name)])))
            return session, (time.perf_counter() - started) * 1000
        failure = None
        # Initialize clients in parallel and keep every resulting session open.
        # Collect all futures, including after one failure, so none escape cleanup.
        initialization_concurrency = min(16, client_count)
        with ThreadPoolExecutor(max_workers=initialization_concurrency) as pool:
            futures = [pool.submit(initialize, index) for index in range(client_count)]
            for index, future in enumerate(futures):
                try:
                    session, elapsed = future.result()
                    if fixture.single_image:
                        stack.push(session)
                    else:
                        stack.callback(session.close)
                    sessions.append((session, index % project_count))
                    initialize_ms.append(elapsed)
                except BaseException as error:
                    if http_fixture.classify_refusal(error) is None:
                        failure = failure or error
        if failure is not None:
            raise failure
        if fixture.single_image:
            assert len({session.session_sha256 for session, _ in sessions}) == len(sessions)
        connected = retained_snapshot(fixture, journal, "connected_native", include_processes=True)
        journal.note("connected", native=connected, initialized_sessions=len(sessions))
        assert connected["resources"]["observation"]["resident_workers"] <= before["resources"]["observation"]["resident_workers"]
        assert connected["embedding_status"]["loads"] == before["embedding_status"]["loads"], "Idle clients unexpectedly load inference models"
        if semantic:
            stable_semantic_owner(before, connected)

        def query(selected, require_ready=True):
            session, number = selected
            attempt = request_attempt(session, "search" if fixture.single_image else "airk_search", search_arguments(number, semantic))
            if "rejected_ms" in attempt:
                return attempt
            envelope, elapsed = attempt["envelope"], attempt["elapsed_ms"]
            if envelope.get("isError"):
                # HTTP 200 tool errors are complete failures, not documented
                # HTTP status/code pre-admission refusals. Keep the evidence.
                if fixture.single_image:
                    raise session.network.response_error(200, json.dumps(envelope).encode(), [], "POST",
                        {"jsonrpc": "2.0", "id": session.sequence, "method": "tools/call",
                         "params": {"name": "search", "arguments": search_arguments(number, semantic)}},
                        "/mcp", failure="tool_error")
                raise http_fixture.HttpResponseError(200, envelope, failure="tool_error")
            value = clients.tool_value(envelope)
            assert value["collection"] == names[number] and value["hits"], value
            hit = value["hits"][0]
            source = owner / names[number] / hit["citation"]["path"]
            assert hit["citation"]["content_hash"] in source_hashes[source]
            if not semantic:
                assert f"projectmarker{number}" in hit["text"]
            else:
                assert isinstance(hit["text"], str) and hit["text"], "Semantic hit text is missing"
            if semantic:
                return {"success_ms":elapsed,"semantic_complete":semantic_result(value,pipeline,require_ready),
                        "session_sha256":session.session_sha256,"tool_result":envelope}
            return {"success_ms": elapsed}

        def recorded_query(phase, selected, require_ready=True):
            try:
                reply = query(selected, require_ready)
            except BaseException as error:
                journal.record_outcome(phase, http_fixture.error_outcome(error))
                raise
            if "success_ms" in reply:
                journal.record_outcome(phase, "succeeded")
            else:
                code = reply.get("admission_code")
                if code is None:
                    value = clients.tool_value(reply["tool_result"])
                    code = value.get("error", value)["code"]
                journal.record_outcome(phase, "refused", code)
            return reply

        def wave(pool, phase, count=rounds, require_ready=True):
            replies, health = [], []
            journal.note(phase + "_started", rounds=count)
            for ordinal in range(count):
                futures = [pool.submit(recorded_query, phase, selected, require_ready) for selected in sessions]
                started = time.perf_counter()
                failure, observed = None, []
                try:
                    assert fixture.rpc("health")["status"] == "health"
                except BaseException as error:
                    failure = error
                    observed.append({"health_error": error_observation(error)})
                health.append((time.perf_counter() - started) * 1000)
                for index, future in enumerate(futures):
                    try:
                        reply = future.result()
                        replies.append(reply)
                        observed.append({"client": index, "result": reply})
                    except BaseException as error:
                        failure = failure or error
                        observed.append({"client": index, "error": error_observation(error)})
                journal.note(phase + "_wave", ordinal=ordinal, observations=observed, health_ms=health[-1])
                if failure is not None:
                    raise failure
            result = {"queries": latency([reply["success_ms"] for reply in replies if "success_ms" in reply]),
                    "rejections": latency([reply["rejected_ms"] for reply in replies if "rejected_ms" in reply]),
                    "rejection_details": [reply for reply in replies if "rejected_ms" in reply],
                    "http_admission_refusals": http_admission_counts(replies),
                    "accounting": journal.value["phases"][phase],
                    "health_cli": latency(health)}
            if semantic:
                result.update(successful_results=[reply for reply in replies if "success_ms" in reply],
                              semantically_partial_results=sum(reply.get("semantic_complete") is False for reply in replies))
            return result

        semantic_observations, semantic_warmup, warm_baseline, cache_transition = {}, None, connected, None
        if semantic:
            journal.note("semantic_warmup_started")
            warmup = []
            for index, selected in enumerate(sessions):
                journal.note("semantic_client_warmup_started", client=index)
                result = recorded_query("semantic_warmup", selected)
                warmup.append(result)
                journal.note("semantic_client_warmup_complete", client=index, result=result)
                if index == 0:
                    warm_baseline = retained_snapshot(fixture, journal, "after_first_existing_warmup")
                    cache_transition = post_first_warmup_baseline(connected, warm_baseline)
                    journal.note("warm_cache_transition", observation=cache_transition)
            semantic_warmup = {"order":"sequential_once_per_client","results":warmup,
                               "latency":latency([reply["success_ms"] for reply in warmup if "success_ms" in reply]),
                               "accounting":journal.value["phases"]["semantic_warmup"]}
            semantic_observations["after_first_existing_warmup"] = warm_baseline
            semantic_observations["after_sequential_warmup"] = retained_snapshot(fixture, journal, "after_sequential_warmup")
            stable_semantic_owner(warm_baseline, semantic_observations["after_sequential_warmup"])
            journal.note("semantic_warmup_complete", results=warmup, native=semantic_observations["after_sequential_warmup"])
        with ThreadPoolExecutor(max_workers=client_count) as pool:
            first_query = wave(pool, "first", 1)
            if semantic:
                semantic_observations["after_first_concurrent_wave"] = retained_snapshot(fixture, journal, "after_first_concurrent_wave")
                stable_semantic_owner(warm_baseline, semantic_observations["after_first_concurrent_wave"])
            idle = wave(pool, "idle")
            if semantic:
                semantic_observations["after_idle_concurrent_waves"] = retained_snapshot(fixture, journal, "after_idle_concurrent_waves")
                stable_semantic_owner(warm_baseline, semantic_observations["after_idle_concurrent_waves"])
            for path in originals:
                content = originals[path] + b"Updated original load text.\n" * 1024
                path.write_bytes(content)
                originals[path] = content
                source_hashes[path].add(hashlib.sha256(content).hexdigest())
            accepted = [fixture.enqueue(name, "index") for name in names]
            journal.note("source_update_accepted", receipts=accepted)
            during = wave(pool, "during", require_ready=not semantic)
            if semantic:
                semantic_observations["after_source_update_concurrent_waves"] = retained_snapshot(fixture, journal, "after_source_update_concurrent_waves")
                stable_semantic_owner(warm_baseline, semantic_observations["after_source_update_concurrent_waves"])
            observed = [fixture.rpc({"job": {"id": receipt["job"]["id"]}})["job"]["state"] for receipt in accepted]
            for receipt in accepted:
                fixture.wait(receipt)
            journal.note("source_index_jobs_complete")
            if semantic:
                for name in names:
                    deadline = time.monotonic() + 600
                    while True:
                        readiness = clients.semantic_readiness(fixture.published_status(name, deadline))
                        assert readiness["state"] != "failed", "Updated vectors explicitly failed"
                        if readiness["state"] == "ready" and readiness["completed_artifacts"] == readiness["selected_artifacts"] == 16:
                            assert readiness.get("pipeline_digest") == pipeline
                            break
                        assert time.monotonic() < deadline, "Updated vectors never regained complete readiness"
                        time.sleep(.05)
            retained_snapshot(fixture, journal, "source_readiness_settled")
            settled = wave(pool, "settled")
        parity = []
        if fixture.single_image:
            for number, name in enumerate(names):
                session = next((session for session, project in sessions if project == number), None)
                if session is None:
                    continue
                arguments = search_arguments(number, semantic)
                def compare():
                    # Each transport submits once. Native-owned safe-read retry
                    # remains at the canonical boundary, never in this harness.
                    mcp = session.request("tools/call", {"name": "search", "arguments": arguments})
                    api = session.network.api_read("search", arguments)
                    def cli_read(until):
                        result = fixture.run(*fixture.arguments(name), "tools", "call", "search", "--args", json.dumps(arguments), check=False, timeout=clients.remaining(until, 90))
                        assert result.returncode in (0, 1)
                        return json.loads(result.stdout)
                    cli = cli_read(time.monotonic()+90)
                    for transport, envelope in (("mcp", mcp), ("json_api", api), ("native_cli", cli)):
                        if envelope.get("isError"):
                            if transport == "native_cli":
                                raise http_fixture.HttpResponseError(200, envelope, failure="tool_error",
                                    request={"transport": "native_cli", "project": name, "tool": "search"})
                            value = ({"jsonrpc": "2.0", "id": session.sequence, "method": "tools/call",
                                      "params": {"name": "search", "arguments": arguments}}
                                     if transport == "mcp" else arguments)
                            raise session.network.response_error(200, json.dumps(envelope).encode(), [], "POST", value,
                                "/mcp" if transport == "mcp" else "/api/v1/tools/search", failure="tool_error")
                    if semantic:
                        for envelope in (mcp,api,cli):
                            semantic_result(clients.tool_value(envelope),pipeline)
                    assert http_fixture.parity_envelope(mcp) == http_fixture.parity_envelope(api) == http_fixture.parity_envelope(cli), "Complete read parity differs"
                    return {"project": name, "mcp": mcp, "json_api": api, "native_cli": cli}
                result = journal.exchange("parity", compare, accept_refusal=True)
                if result is not None:
                    parity.append(result)
                    journal.note("full_result_parity", **result)
        else:
            # Historical stdio has no JSON API parity contract.
            journal.plan_phase("parity", 0)
        final = retained_snapshot(fixture, journal, "settled_native", include_processes=True)
        session_ids = [session.session_sha256 for session, _ in sessions] if fixture.single_image else None
        for session, _ in sessions:
            journal.exchange("client_close", session.close)
        after_close = retained_snapshot(fixture, journal, "after_client_close_native", include_processes=True)
        if semantic:
            semantic_observations.update(before_clients=before, connected=connected, settled=final, after_client_close=after_close)
            for phase, observation in semantic_observations.items():
                baseline = before if phase in ("before_clients", "connected") else warm_baseline
                stable_semantic_owner(baseline, observation)
        assert fixture.rpc("health")["observation"]["instance"] == original_instance, "Client close changed the native service instance"
        assert all(path.read_bytes() == content for path, content in originals.items())
        assertion_coverage = {
            "complete_result_parity": {"required": fixture.single_image,
                "verified": not fixture.single_image or len(parity) == min(project_count, client_count),
                "unverified_projects": [name for name in names[:min(project_count, client_count)] if name not in {entry["project"] for entry in parity}]},
            "semantic_warmup_baseline": {"required": semantic, "verified": not semantic or cache_transition is not None}}
        journal.value["assertion_coverage"] = assertion_coverage
        qualified = (all(phase["passed"] for phase in journal.value["phases"].values())
                     and all(not value["required"] or value["verified"] for value in assertion_coverage.values()))
        journal.value["qualified"] = qualified
        return {"projects": project_count, "clients": client_count, "rounds": rounds,
                "phases": journal.value["phases"],
                "assertion_coverage": assertion_coverage, "qualified": qualified,
                "transport": "https" if fixture.single_image else "historical_stdio",
                "simultaneously_initialized_sessions": len(sessions), "session_identity_sha256": session_ids,
                "initialization_concurrency": initialization_concurrency,
                "per_connection_credential_count": len(connections) if fixture.single_image else None,
                "enrollment": latency(enrollment_ms), "initialize": latency(initialize_ms),
                ("first_concurrent_query_after_semantic_warmup" if semantic else "first_query_after_fixture_setup"): first_query, "idle": idle, "during": during,
                "native_observations": {"before": before, "connected": connected, "settled": final, "after_client_close": after_close},
                "model_load_delta_idle_connections": connected["embedding_status"]["loads"]-before["embedding_status"]["loads"],
                "source_job_states_after_wave": observed, "settled": settled, "full_result_parity": parity,
                "parity_coverage": {"projects_with_initialized_client": [name for number, name in enumerate(names) if any(project == number for _, project in sessions)],
                                    "verified": assertion_coverage["complete_result_parity"]["verified"],
                                    "compared_projects": [entry["project"] for entry in parity],
                                    "unverified_projects": [name for name in names if name not in {entry["project"] for entry in parity}],
                                    "unrepresented_projects": [name for number, name in enumerate(names) if not any(project == number for _, project in sessions)] if fixture.single_image else names},
                "all_owned_client_sessions_closed": all(session.closed for session, _ in sessions) if fixture.single_image else True,
                "all_owned_client_sessions_deleted": all(session.delete_completed for session, _ in sessions) if fixture.single_image else None,
                "tcp_policy": "New TCP before each new MCP request and DELETE; original credential/MCP session retained; no exchange replay; TLS handshake cost included." if fixture.single_image else None,
                "max_concurrent_handshakes": 16 if fixture.single_image else None,
                "source_bytes_preserved": True,
                **({"semantic": {"mode":"explicit_vector","setup":semantic_setup,"warmup":semantic_warmup,
                                 "cache_transition":cache_transition,"original_baseline":before,"post_first_warmup_baseline":warm_baseline,
                                 "observations":semantic_observations,
                                 "measurement_limits":"Models loaded during fixture vector preparation; sequential warmup precedes concurrent waves. During-update vector results may disclose partial readiness; all settled reads require complete readiness. This is not a ranking/recall evaluation. Native aggregate RSS only; no private physical memory measurement or global zero-helper certification."}} if semantic else {})}



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=5)
    parser.add_argument("--clients", type=int, nargs="+", choices=(1, 10, 64), default=[1, 10, 64])
    parser.add_argument("--projects", type=int, nargs="+", choices=(1, 4, 10), default=[1, 4, 10])
    parser.add_argument("--semantic", action="store_true", help="Qualify the approved embedded MiniLM pipeline and shared in-process owner with explicit vector reads")
    args = parser.parse_args()
    if not 1 <= args.rounds <= 30:
        parser.error("rounds must be 1..30")
    runtime = clients.runtime_root(args.runtime_dir)
    report = {"schema": "airk-service-load/2", "platform": platform.platform(),
              "logical_cpus": os.cpu_count(), "machine": platform.machine(),
              "runtime_manifest_sha256": hashlib.sha256((runtime / "airk-install.json").read_bytes()).hexdigest(),
              "limitations": "Single host; lexical original fixtures; uncontrolled OS caches (first query is not a cold filesystem or cold service); health includes CLI startup; typed read rejections counted separately without mutation retries; during-wave jobs may finish before all samples; native observed_rss_bytes is the native aggregate telemetry only, not measured private physical memory; CPU not measured; process scans cannot certify unobserved global processes; no commercial client UI qualification.",
              "tcp_policy": "Fresh TCP before each new MCP request/DELETE; original credential and MCP session retained; no submitted exchange replay; TLS handshake included in latency.",
              "max_concurrent_handshakes": 16,
              "selection": {"client_counts": args.clients, "project_counts": args.projects,
                            "excluded_client_counts": [value for value in (1, 10, 64) if value not in args.clients],
                            "excluded_project_counts": [value for value in (1, 4, 10) if value not in args.projects]},
              "cases": [], "passed": False}
    report["semantic"] = args.semantic
    evidence_dir = args.output.parent / (args.output.stem + "-cases")
    evidence_dir.mkdir(parents=True, exist_ok=False)
    report["case_evidence"] = []
    if args.semantic:
        report["limitations"] = report["limitations"].replace("lexical original fixtures", "explicit-vector original fixtures; model loaded by setup/index jobs before clients; sequential per-client warmup precedes concurrent reads")
    try:
        for projects in args.projects:
            for count in args.clients:
                evidence_path = evidence_dir / f"projects-{projects}-clients-{count}.json"
                report["case_evidence"].append(str(evidence_path))
                args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
                case = campaign(runtime, projects, count, args.rounds, semantic=args.semantic, evidence_path=evidence_path)
                report["cases"].append(case)
                print(json.dumps({"projects": projects, "clients": count, "completed": True}), flush=True)
        report["passed"] = all(case["qualified"] for case in report["cases"])
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report["passed"]


if __name__ == "__main__":
    raise SystemExit(0 if main() else 1)
