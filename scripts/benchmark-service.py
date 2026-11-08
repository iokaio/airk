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
    def __init__(self, connection, control):
        self.network = http_fixture.McpClient(connection, control)
        self.sequence = 0
        self.closed = False
        try:
            self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                       "clientInfo": {"name": "airk-service-load", "version": "1"}})
            self.network.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            assert self.network.session is not None, "Native MCP session identity missing"
            self.session_sha256 = hashlib.sha256(self.network.session.encode()).hexdigest()
        except BaseException:
            self.close()
            raise

    def request(self, method, parameters):
        self.sequence += 1
        self.network.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": parameters})
        reply = self.network.receive(self.sequence)
        assert "error" not in reply, "Native benchmark JSON-RPC refused"
        return reply["result"]

    def close(self):
        if not self.closed:
            self.network.close()
            self.closed = True


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


def snapshot(fixture, include_processes=True):
    resources, provider = fixture.rpc("resources"), fixture.rpc("embedding_status")
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


def prepare_semantic(fixture, names, pipeline):
    before = snapshot(fixture, include_processes=False)
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
    after = snapshot(fixture)
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
        body = error.body
        if (error.status != 429 or not isinstance(body, dict)
                or body.get("schema") != "airk.local-api-error/1"
                or not isinstance(body.get("error"), dict)
                or body["error"].get("code") != "query_admission_busy"):
            raise
        return {"rejected_ms": (time.perf_counter()-started)*1000,
                "http_status": error.status, "body": body}
    return {"elapsed_ms": (time.perf_counter()-started)*1000, "envelope": envelope}


def latency(samples):
    ordered = sorted(samples)
    return {"samples": len(ordered), "p95_ms": ordered[math.ceil(len(ordered) * .95) - 1],
            "maximum_ms": ordered[-1]} if ordered else {"samples": 0}


def search_arguments(project, semantic=False):
    return {"query": f"projectmarker{project}", "mode": "vector" if semantic else "lexical", "limit": 1,
            "max_bytes": 4096, "verification": "off", "response_mode": "full"}


def campaign(runtime, project_count, client_count, rounds, semantic=False):
    pipeline = semantic_pipeline(runtime) if semantic else None
    with clients.owned_client_fixture() as temporary, ExitStack() as stack:
        owner = Path(temporary).resolve(strict=True)
        fixture = stack.enter_context(clients.Fixture(runtime, owner, job_timeout=600))
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
        semantic_setup = prepare_semantic(fixture, names, pipeline) if semantic else None
        before = semantic_setup["after"] if semantic else snapshot(fixture)
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
            session = (HttpSession(connections[index], fixture.control) if fixture.single_image
                       else retrieval.Session([str(fixture.binary), *fixture.arguments(name)]))
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
                    stack.callback(session.close)
                    sessions.append((session, index % project_count))
                    initialize_ms.append(elapsed)
                except BaseException as error:
                    failure = failure or error
        if failure is not None:
            raise failure
        if fixture.single_image:
            assert len({session.session_sha256 for session, _ in sessions}) == client_count
        connected = snapshot(fixture)
        assert connected["resources"]["observation"]["resident_workers"] <= before["resources"]["observation"]["resident_workers"]
        assert connected["embedding_status"]["loads"] == before["embedding_status"]["loads"], "Idle clients unexpectedly load inference models"

        def query(selected, require_ready=True):
            session, number = selected
            attempt = request_attempt(session, "search" if fixture.single_image else "airk_search", search_arguments(number, semantic))
            if "rejected_ms" in attempt:
                return attempt
            envelope, elapsed = attempt["envelope"], attempt["elapsed_ms"]
            if envelope.get("isError"):
                assert clients.read_retry_delay(envelope) is not None, envelope
                return {"rejected_ms": elapsed, "tool_result": envelope}
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

        def wave(pool, count=rounds, require_ready=True):
            replies, health = [], []
            for _ in range(count):
                futures = [pool.submit(query, selected, require_ready) for selected in sessions]
                started = time.perf_counter()
                assert fixture.rpc("health")["status"] == "health"
                health.append((time.perf_counter() - started) * 1000)
                replies.extend(future.result() for future in futures)
            result = {"queries": latency([reply["success_ms"] for reply in replies if "success_ms" in reply]),
                    "rejections": latency([reply["rejected_ms"] for reply in replies if "rejected_ms" in reply]),
                    "rejection_details": [reply for reply in replies if "rejected_ms" in reply],
                    "health_cli": latency(health)}
            if semantic:
                result.update(successful_results=[reply for reply in replies if "success_ms" in reply],
                              semantically_partial_results=sum(reply.get("semantic_complete") is False for reply in replies))
            return result

        semantic_observations, semantic_warmup = {}, None
        if semantic:
            warmup = [query(selected) for selected in sessions]
            assert all("success_ms" in reply for reply in warmup), "Every distinct semantic client must execute its warm read"
            semantic_warmup = {"order":"sequential_once_per_client","results":warmup,
                               "latency":latency([reply["success_ms"] for reply in warmup])}
            semantic_observations["after_sequential_warmup"] = snapshot(fixture, include_processes=False)
            stable_semantic_owner(before, semantic_observations["after_sequential_warmup"])
        with ThreadPoolExecutor(max_workers=client_count) as pool:
            first_query = wave(pool, 1)
            if semantic:
                semantic_observations["after_first_concurrent_wave"] = snapshot(fixture, include_processes=False)
            idle = wave(pool)
            if semantic:
                semantic_observations["after_idle_concurrent_waves"] = snapshot(fixture, include_processes=False)
            for path in originals:
                content = originals[path] + b"Updated original load text.\n" * 1024
                path.write_bytes(content)
                originals[path] = content
                source_hashes[path].add(hashlib.sha256(content).hexdigest())
            accepted = [fixture.enqueue(name, "index") for name in names]
            during = wave(pool, require_ready=not semantic)
            if semantic:
                semantic_observations["after_source_update_concurrent_waves"] = snapshot(fixture, include_processes=False)
            observed = [fixture.rpc({"job": {"id": receipt["job"]["id"]}})["job"]["state"] for receipt in accepted]
            for receipt in accepted:
                fixture.wait(receipt)
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
            settled = wave(pool)
            assert settled["queries"]["samples"] > 0, settled
        parity = []
        if fixture.single_image:
            for number, name in enumerate(names):
                session = sessions[number % len(sessions)][0]
                # For fewer clients than projects, each representative read is
                # selected only from that actual client's immutable authority.
                if sessions[number % len(sessions)][1] != number:
                    continue
                arguments = search_arguments(number, semantic)
                mcp = clients.read_envelope(lambda _: session.request("tools/call", {"name": "search", "arguments": arguments}), time.monotonic()+90)[0]
                api = clients.read_envelope(lambda _: session.network.api_read("search", arguments), time.monotonic()+90)[0]
                def cli_read(until):
                    result = fixture.run(*fixture.arguments(name), "tools", "call", "search", "--args", json.dumps(arguments), check=False, timeout=clients.remaining(until, 90))
                    assert result.returncode in (0, 1)
                    return json.loads(result.stdout)
                cli = clients.read_envelope(cli_read, time.monotonic()+90)[0]
                if semantic:
                    for envelope in (mcp,api,cli):
                        semantic_result(clients.tool_value(envelope),pipeline)
                assert http_fixture.parity_envelope(mcp) == http_fixture.parity_envelope(api) == http_fixture.parity_envelope(cli), "Complete read parity differs"
                parity.append({"project": name, "mcp": mcp, "json_api": api, "native_cli": cli})
        final = snapshot(fixture)
        session_ids = [session.session_sha256 for session, _ in sessions] if fixture.single_image else None
        for session, _ in sessions:
            session.close()
        after_close = snapshot(fixture)
        if semantic:
            semantic_observations.update(before_clients=before, connected=connected, settled=final, after_client_close=after_close)
            for observation in semantic_observations.values():
                stable_semantic_owner(before, observation)
        assert fixture.rpc("health")["observation"]["instance"] == original_instance, "Client close changed the native service instance"
        assert all(path.read_bytes() == content for path, content in originals.items())
        return {"projects": project_count, "clients": client_count, "rounds": rounds,
                "transport": "https" if fixture.single_image else "historical_stdio",
                "simultaneously_initialized_sessions": len(sessions), "session_identity_sha256": session_ids,
                "initialization_concurrency": initialization_concurrency,
                "per_connection_credential_count": len(connections) if fixture.single_image else None,
                "enrollment": latency(enrollment_ms), "initialize": latency(initialize_ms),
                ("first_concurrent_query_after_semantic_warmup" if semantic else "first_query_after_fixture_setup"): first_query, "idle": idle, "during": during,
                "native_observations": {"before": before, "connected": connected, "settled": final, "after_client_close": after_close},
                "model_load_delta_idle_connections": connected["embedding_status"]["loads"]-before["embedding_status"]["loads"],
                "source_job_states_after_wave": observed, "settled": settled, "full_result_parity": parity,
                "parity_coverage": {"projects_with_initialized_client": [entry["project"] for entry in parity],
                                    "unrepresented_projects": names[min(client_count, project_count):] if fixture.single_image else names},
                "all_owned_client_sessions_closed": all(session.closed for session, _ in sessions) if fixture.single_image else True,
                "source_bytes_preserved": True,
                **({"semantic": {"mode":"explicit_vector","setup":semantic_setup,"warmup":semantic_warmup,
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
              "cases": [], "passed": False}
    report["semantic"] = args.semantic
    if args.semantic:
        report["limitations"] = report["limitations"].replace("lexical original fixtures", "explicit-vector original fixtures; model loaded by setup/index jobs before clients; sequential per-client warmup precedes concurrent reads")
    try:
        for projects in args.projects:
            for count in args.clients:
                case = campaign(runtime, projects, count, args.rounds, semantic=args.semantic)
                report["cases"].append(case)
                print(json.dumps({"projects": projects, "clients": count, "completed": True}), flush=True)
        report["passed"] = True
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
