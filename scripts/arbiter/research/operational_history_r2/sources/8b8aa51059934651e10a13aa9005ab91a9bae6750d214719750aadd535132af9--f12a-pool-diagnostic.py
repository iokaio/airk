# SPDX-License-Identifier: Apache-2.0
"""Offline two-project native service/MiniLM smoke test; never packages or installs a VSIX."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time
import uuid


spec = importlib.util.spec_from_file_location("service_clients", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


class Bridge:
    def __init__(self, binary, runtime, binding, environment, collection):
        self.binding, self.collection = binding, collection
        self.process = subprocess.Popen(
            [str(binary), "bridge", "--runtime-dir", str(runtime), "--project", binding],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            env=environment, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.responses = queue.Queue(maxsize=4)
        self.sequence = 0

        def read():
            while True:
                line = self.process.stdout.readline(128 * 1024 + 1)
                if not line or len(line) > 128 * 1024:
                    return
                try:
                    self.responses.put_nowait(json.loads(line))
                except (ValueError, queue.Full):
                    return

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        self.call("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                "clientInfo": {"name": "airk-pool-original-fixture", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, value):
        self.process.stdin.write((json.dumps(value) + "\n").encode("utf-8"))
        self.process.stdin.flush()

    def call(self, method, params, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 120)
        clients.remaining(deadline, 120)
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        response = self.responses.get(timeout=clients.remaining(deadline, 120))
        if response.get("id") != self.sequence or "error" in response:
            raise RuntimeError("MCP bridge returned an invalid response")
        return response["result"]

    def read(self, name, arguments, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 120)
        params = {"name": name, "arguments": arguments}
        result, _ = clients.read_envelope(lambda until: self.call("tools/call", params, until), deadline)
        return clients.tool_value(result)

    def search(self, mode, query="copper beacon"):
        result = self.read("airk_search", {"query": query, "mode": mode, "max_bytes": 4096})
        if "memo.md" not in json.dumps(result):
            raise RuntimeError(f"{mode} retrieval lost the original collection citation")

    def published_status(self, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 120)
        return clients.observe_publication(lambda until: self.read("airk_diagnostics", {"response_mode": "full"}, until),
                                           self.collection, self.binding, deadline)

    def readiness(self, deadline=None):
        return clients.semantic_readiness(self.published_status(deadline))

    def wait_semantics(self, expected="ready"):
        deadline = time.monotonic() + 180
        while True:
            readiness = self.readiness(deadline)
            if readiness["state"] == expected:
                return
            if readiness["state"] == "failed" or time.monotonic() >= deadline:
                raise RuntimeError(f"Independent semantic phase did not reach {expected}: state={readiness['state']}")
            time.sleep(0.2)

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=10)
        self.reader.join(timeout=5)
        self.process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--diagnostic-dir", type=Path, required=True)
    args = parser.parse_args()
    diagnostic_dir = args.diagnostic_dir.resolve(strict=True)
    if not diagnostic_dir.is_dir() or diagnostic_dir.is_symlink():
        raise RuntimeError("Diagnostic directory must be an existing owned directory")
    binary = args.runtime.resolve(strict=True) / ("airk.exe" if os.name == "nt" else "airk")
    if not binary.is_file():
        binary = args.runtime.resolve(strict=True) / "AIRK.app/Contents/MacOS/airk"
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1",
                       HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)

    def run(*arguments, timeout=240):
        result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                                env=environment, timeout=timeout, encoding="utf-8", creationflags=flags)
        if result.returncode:
            raise RuntimeError(result.stderr[-3000:])
        return json.loads(result.stdout)

    with tempfile.TemporaryDirectory(prefix="airk-pool-smoke-") as temporary:
        fixture = Path(temporary)
        runtime = fixture / "runtime"
        log = (fixture / "service.stderr").open("wb")
        service = subprocess.Popen([str(binary), "serve", "--runtime-dir", str(runtime)],
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                   stderr=log, env=environment, creationflags=flags)
        bridges = []
        projects = []
        accepted = None
        snapshot_sequence = 0

        def rpc(method, *, deadline=None, expected_failure=None):
            path = fixture / "request.json"
            def invoke(exact, until):
                path.write_text(exact, encoding="utf-8")
                return run("service", "rpc", "--runtime-dir", runtime, "--request-file", path,
                           timeout=clients.remaining(until, 90))
            return clients.metadata_rpc(invoke, method, deadline, expected_failure=expected_failure)

        def snapshot(stage):
            nonlocal snapshot_sequence
            sequence = snapshot_sequence
            snapshot_sequence += 1
            result = {"stage": stage, "observed_unix_ms": int(time.time()*1000), "responses": {}}
            until = time.monotonic()+15
            def observed(label, method):
                try:
                    path = fixture / "diagnostic-request.json"
                    path.write_text(json.dumps(method), encoding="utf-8")
                    return run("service", "rpc", "--runtime-dir", runtime, "--request-file", path,
                               timeout=clients.remaining(until, 5))
                except Exception as error:
                    result["responses"][label] = {"observation_failed": True, "exception_type": type(error).__name__}
                    return None
            def job(value):
                if not isinstance(value,dict):return None
                return {key:value.get(key) for key in ["id","kind","state","attempts","outcome"]}
            if projects:
                binding = projects[0]["binding"]
                value = observed("progress", {"progress":{"project":binding}})
                if value is not None:
                    observation = value.get("observation") or {}
                    progress = observation.get("progress") or {}
                    result["responses"]["progress"] = {
                        "pending_job":job(value.get("pending_job")), "queued_jobs":[job(row) for row in value.get("queued_jobs",[])],
                        "queue_truncated":value.get("queue_truncated"), "project_paused":value.get("project_paused"),
                        "active_job":job(observation.get("job")),
                        "progress":{key:progress.get(key) for key in ["phase","sequence","paused","stalled","elapsed_ms","captured_files","extracted_files","prepared_changes","committed_changes"]}}
                value = observed("console", {"console":{"after":None,"revision":None,"limit":32}})
                if value is not None:
                    rows=[]
                    for row in value.get("page",{}).get("projects",[]):
                        if row.get("project",{}).get("registration",{}).get("binding") == binding:
                            rows.append({"pending_job":job(row.get("pending_job")),"recovery":[{key:r.get(key) for key in ["reason","state","attempts","limit","job","next_check_unix_ms"]} for r in row.get("recovery",[])],"failed_observations":row.get("failed_observations",[])})
                    result["responses"]["console"]={"selected_rows":rows,"complete":value.get("page",{}).get("after") is None}
                    recovery_jobs=list(dict.fromkeys(r.get("job") for row in rows for r in row["recovery"] if r.get("job")))
                    for job_id in recovery_jobs[:1]:
                        recovered=observed("recovery_job", {"job":{"id":job_id}})
                        if recovered is not None:result["responses"]["recovery_job"]=job(recovered.get("job"))
                value = observed("events", {"events":{"project":binding,"cursor":None,"limit":32}})
                if value is not None:
                    result["responses"]["events"]={"events":[{key:row.get(key) for key in ["sequence","kind","job","job_kind","attempts","code"]} for row in value.get("page",{}).get("events",[])],"more":value.get("page",{}).get("next") is not None}
            value = observed("embedding", "embedding_status")
            if value is not None:result["responses"]["embedding"]={key:value.get(key) for key in ["status","available","worker","loads","sessions"]}
            if accepted:
                value=observed("source_job", {"job":{"id":accepted["job"]["id"]}})
                if value is not None:result["responses"]["source_job"]=job(value.get("job"))
            with (diagnostic_dir/f"snapshot-{sequence:02d}-{stage}.json").open("x",encoding="utf-8") as output:
                json.dump(result,output,indent=2)

        try:
            deadline = time.monotonic() + 90
            while True:
                try:
                    if run("service", "health", "--runtime-dir", runtime)["status"] == "health":
                        break
                except RuntimeError:
                    pass
                if time.monotonic() >= deadline or service.poll() is not None:
                    raise RuntimeError("Native service did not become ready")
                time.sleep(0.1)

            projects = []
            originals = []
            for number in range(2):
                source = fixture / f"source-{number}"
                data = fixture / f"data-{number}"
                source.mkdir()
                data.mkdir()
                original = f"# Copper beacon {number}\nOriginal copper beacon maintenance fixture {number}.\n"
                document = source / "memo.md"
                document.write_text(original, encoding="utf-8")
                originals.append((document, document.read_bytes()))
                base = ["--root", source, "--data-dir", data, "--collection", f"pool-fixture-{number}"]
                response = run(*base, "service", "register", "--runtime-dir", runtime)
                if response.get("status") != "registered":
                    raise RuntimeError(f"Project registration failed: {response}")
                projects.append(response["project"]["registration"])
            print("Canonical registration passed without project-local model provisioning.", flush=True)

            epoch = rpc("request_epoch")
            # Explicit trusted setup is a durable service job. The helper installs
            # one shared set directly from embedded bytes; no project owns weights.
            for project in projects:
                accepted = rpc({"enqueue": {"project": project["binding"], "caller": uuid.uuid4().hex,
                    "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                    "kind": "model_setup"}})
                if accepted.get("status") != "job":
                    raise RuntimeError(f"Model setup not accepted: {accepted}")
                deadline = time.monotonic() + 180
                while True:
                    receipt = rpc({"job": {"id": accepted["job"]["id"]}}, deadline=deadline)["job"]
                    if receipt["state"] == "succeeded":
                        break
                    if receipt["state"] in ("failed", "canceled") or time.monotonic() >= deadline:
                        raise RuntimeError(f"Shared offline setup failed: {receipt}")
                    time.sleep(0.2)
            jobs = []
            for project in projects:
                caller = uuid.uuid4().hex
                accepted = rpc({"enqueue": {"project": project["binding"], "caller": caller,
                    "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                    "kind": "index"}})
                if accepted.get("status") != "job":
                    raise RuntimeError(f"Index job was not accepted: {accepted}")
                jobs.append(accepted["job"]["id"])
            deadline = time.monotonic() + 180
            while jobs:
                for job in list(jobs):
                    response = rpc({"job": {"id": job}}, deadline=deadline)
                    state = response.get("job", {}).get("state")
                    if state == "succeeded":
                        jobs.remove(job)
                    elif state in ("failed", "canceled"):
                        raise RuntimeError(f"Supervised indexing failed: {response}")
                if time.monotonic() >= deadline:
                    raise RuntimeError("Supervised indexing did not complete within the fixture budget")
                time.sleep(0.2)

            for project in projects:
                bridge = Bridge(binary, runtime, project["binding"], environment, project["collection"])
                bridges.append(bridge)
                bridge.wait_semantics()
                bridge.search("hybrid")
            observation = rpc("embedding_status")
            if (not observation.get("available") or observation.get("loads") != 1
                    or observation.get("reuses", 0) < 1):
                raise RuntimeError(f"Projects did not share one verified MiniLM load: {observation}")
            print("Two supervised projects reused one MiniLM session and returned hybrid citations.", flush=True)

            replacement = rpc({"enqueue": {"project": projects[0]["binding"], "caller": uuid.uuid4().hex,
                "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                "kind": "rebuild"}})
            if replacement.get("status") != "job":
                raise RuntimeError("Replacement rebuild was not accepted")
            deadline = time.monotonic() + 180
            while True:
                receipt = rpc({"job": {"id": replacement["job"]["id"]}}, deadline=deadline)["job"]
                if receipt["state"] == "succeeded":
                    break
                if receipt["state"] in ("failed", "canceled") or time.monotonic() >= deadline:
                    raise RuntimeError(f"Supervised replacement did not complete: {receipt}")
                time.sleep(0.2)
            for bridge in bridges:
                bridge.wait_semantics()
                bridge.search("hybrid")
            observation = rpc("embedding_status")
            if not observation.get("available") or observation.get("loads") != 1:
                raise RuntimeError("Replacement stopped compatible loaded-session reuse")
            deadline = time.monotonic() + 90
            while True:
                maintenance = rpc({"maintenance": {"project": projects[0]["binding"]}}, deadline=deadline)
                if (maintenance.get("observation") or {}).get("pending_incarnations") == 0:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Loaded model prevented old-incarnation retirement: {maintenance}")
                time.sleep(0.2)
            sets = [path for path in (runtime / "model-files-1.4" / "sets").iterdir()
                    if len(path.name) == 64 and path.is_dir()]
            for project in projects:
                data = Path(project["data_root"])
                if (data / "models").exists() or any(data.glob("collections-1.4/**/objects/models/*")):
                    raise RuntimeError("Project retained an unexpected model-weight copy")
            if len(sets) != 1 or not (sets[0] / "model.onnx").is_file() or not (sets[0] / "installed.json").is_file():
                raise RuntimeError("Compatible projects did not share one immutable model-file set")
            if rpc("embedding_status").get("loads") != 1:
                raise RuntimeError("Retirement unnecessarily discarded the compatible loaded model")
            print("Replacement preserved direct clients and one MiniLM load; old incarnation retired while shared files stayed pinned.", flush=True)

            before = bridges[0].published_status()["publication"]
            plan = rpc({"plan_create": {"caller": uuid.uuid4().hex,
                "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                "request": {"selection": {"selected": [projects[0]["binding"]]}, "operation": "vectors"}}})["plan"]
            deadline = time.monotonic() + 180
            while plan["phase"] != "ready":
                if plan["phase"] != "planning" or time.monotonic() >= deadline:
                    raise RuntimeError(f"Vector regeneration preview failed: {plan}")
                time.sleep(0.2)
                plan = rpc({"plan": {"id": plan["id"]}}, deadline=deadline)["plan"]
            page = rpc({"plan_items": {"id": plan["id"], "after": None, "limit": 32}})["page"]
            if page["after"] is not None or len(page["items"]) != 1 or page["items"][0]["action"] != "vectors":
                raise RuntimeError(f"Vector regeneration preview selected unexpected work: {page}")
            rpc({"plan_execute": {"id": plan["id"], "preview": plan["preview_digest"], "allow_fallback": False}})
            while True:
                plan = rpc({"plan": {"id": plan["id"]}}, deadline=deadline)["plan"]
                if plan["phase"] == "finished":
                    if plan["counts"]["succeeded"] != 1:
                        raise RuntimeError(f"Vector regeneration did not succeed: {plan}")
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Vector regeneration did not finish: {plan}")
                time.sleep(0.2)
            bridges[0].wait_semantics()
            after = bridges[0].published_status()["publication"]
            if after["incarnation"] != before["incarnation"] or after["generation"] <= before["generation"]:
                raise RuntimeError("Vector regeneration did not advance the same index incarnation")
            for bridge in bridges:
                bridge.search("lexical")
                bridge.search("vector")
            if rpc("embedding_status").get("loads") != 1:
                raise RuntimeError("Vector regeneration discarded the shared MiniLM session")
            print("Approved vector regeneration recomputed completed evidence while preserving lexical citations, index incarnation and one MiniLM load.", flush=True)

            def event_tail(cursor=None, deadline=None):
                events = []
                for _ in range(16):
                    page = rpc({"events": {"project": projects[0]["binding"], "cursor": cursor, "limit": 32}}, deadline=deadline)["page"]
                    events.extend(page["events"])
                    if page["next"] is None:
                        return events, page["tail"]
                    cursor = page["next"]
                raise RuntimeError("Fixture event observation exceeded its bounded pages")

            _, event_cursor = event_tail()
            # Ask the native owner to drain its exact opaque worker identity.
            # A stale/foreign identity must never stop the current helper.
            worker_identities = [rpc({"worker": {"project": project["binding"]}})["identity"] for project in projects]
            if any(identity is None for identity in worker_identities):
                raise RuntimeError("Expected resident project worker identity before helper stop")
            observation = rpc("embedding_status")
            rejected = rpc({"stop_embedding": {"worker": uuid.uuid4().hex, "force": True}}, expected_failure="embedding_worker_changed")
            if rejected.get("code") != "embedding_worker_changed":
                raise RuntimeError(f"Foreign helper stop was not rejected: {rejected}")
            stopped = rpc({"stop_embedding": {"worker": observation["worker"], "force": True}})
            if stopped.get("status") != "embedding_stopped":
                raise RuntimeError(f"Owned embedding worker did not confirm native drain: {stopped}")
            for bridge in bridges:
                bridge.search("lexical")
            if run("service", "health", "--runtime-dir", runtime)["status"] != "health":
                raise RuntimeError("Embedding failure blocked independent service health")
            # New lexical source work must also publish without the dead helper.
            document, original = originals[0]
            edited = original + b"\nQuartz lighthouse revised source fixture.\n"
            document.write_bytes(edited)
            originals[0] = (document, edited)
            accepted = rpc({"enqueue": {"project": projects[0]["binding"], "caller": uuid.uuid4().hex,
                "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                "kind": "index"}})
            if accepted.get("status") != "job":
                raise RuntimeError("Post-failure source job was not accepted")
            deadline = time.monotonic() + 180
            while True:
                receipt = rpc({"job": {"id": accepted["job"]["id"]}}, deadline=deadline)["job"]
                if receipt["state"] == "succeeded":
                    break
                if receipt["state"] == "failed":
                    raise RuntimeError(f"Semantic helper loss failed independent source publication: {receipt}")
                if receipt["state"] == "canceled" or time.monotonic() >= deadline:
                    raise RuntimeError(f"Post-failure source job did not finish: {receipt}")
                time.sleep(0.2)
            bridges[0].search("lexical", "quartz lighthouse")
            # Readiness describes persisted coverage, while the independent job
            # receipt proves failure even if its diagnostic publication was busy.
            deadline = time.monotonic() + 180
            snapshot("after-source")
            diagnostic_after = time.monotonic()+10
            while True:
                if diagnostic_after is not None and time.monotonic() >= diagnostic_after:
                    snapshot("waiting-event")
                    diagnostic_after = None
                events, event_cursor = event_tail(event_cursor, deadline)
                failed = next((event for event in events if event["kind"] == "job_failed"
                               and event.get("job_kind") == "vectors"), None)
                if failed is not None:
                    failed_semantic = rpc({"job": {"id": failed["job"]}}, deadline=deadline)["job"]
                    if failed_semantic["state"] != "failed":
                        raise RuntimeError("Failed semantic event disagreed with its receipt")
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError("Post-stop semantic continuation did not report failure: " + json.dumps({
                        "events": events, "progress": rpc({"progress": {"project": projects[0]["binding"]}}),
                        "resources": rpc("resources"), "embedding": rpc("embedding_status")}))
                time.sleep(0.2)
            if bridges[0].readiness()["state"] == "ready":
                raise RuntimeError("Failed semantic job claimed complete coverage")
            if rpc({"job": {"id": accepted["job"]["id"]}})["job"]["state"] != "succeeded":
                raise RuntimeError("Later semantic failure changed the successful source receipt")
            if any(document.read_bytes() != original for document, original in originals):
                raise RuntimeError("Native service changed an original source document")
            print("Embedding-process failure preserved lexical retrieval, service health and original source bytes.", flush=True)
            if rpc("embedding_status").get("available"):
                raise RuntimeError("Explicit helper stop was automatically undone")
            starting = rpc("start_embedding")
            if starting.get("status") != "embedding_starting":
                raise RuntimeError(f"Explicit helper recovery was not accepted: {starting}")
            deadline = time.monotonic() + 90
            while True:
                recovered = rpc("embedding_status")
                if recovered.get("available"):
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Embedding replacement did not become ready: {recovered}")
                time.sleep(0.2)
            if recovered["worker"] == observation["worker"]:
                raise RuntimeError("Embedding replacement reused an old process incarnation")
            for project, identity in zip(projects, worker_identities):
                if rpc({"worker": {"project": project["binding"]}})["identity"] != identity:
                    raise RuntimeError("Helper recovery replaced an independent project worker")
            # Failed semantic jobs require an explicit new request; recovery never
            # silently replays an uncertain operation or cancels a source receipt.
            accepted = rpc({"retry": {"id": failed_semantic["id"], "caller": uuid.uuid4().hex,
                "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}}})
            if accepted.get("status") != "job":
                raise RuntimeError(f"Semantic recovery was not accepted: {accepted}")
            deadline = time.monotonic() + 180
            while True:
                receipt = rpc({"job": {"id": accepted["job"]["id"]}}, deadline=deadline)["job"]
                if receipt["state"] == "succeeded":
                    break
                if receipt["state"] in ("failed", "canceled") or time.monotonic() >= deadline:
                    raise RuntimeError(f"Explicit semantic recovery failed: {receipt}")
                time.sleep(0.2)
            bridges[0].wait_semantics()
            for bridge in bridges:
                bridge.search("vector")
            if rpc({"job": {"id": failed_semantic["id"]}})["job"]["state"] != "failed":
                raise RuntimeError("Explicit semantic retry rewrote the original failure receipt")
            recovered = rpc("embedding_status")
            if recovered.get("loads") != 1:
                raise RuntimeError(f"Replacement helper did not share one compatible model: {recovered}")
            if any(document.read_bytes() != original for document, original in originals):
                raise RuntimeError("Helper recovery changed original source bytes")
            print("Explicit helper recovery rebound existing project sessions and restored vectors with one new shared MiniLM load.", flush=True)

        except Exception:
            try:
                snapshot("failure")
            except Exception as diagnostic_error:
                with (diagnostic_dir/"snapshot-failure-error.txt").open("x") as output:
                    output.write(type(diagnostic_error).__name__)
            raise
        finally:
            for bridge in bridges:
                bridge.close()
            if service.poll() is None:
                try:
                    run("service", "shutdown", "--runtime-dir", runtime)
                    service.wait(timeout=60)
                except (RuntimeError, subprocess.TimeoutExpired):
                    service.kill()
                    service.wait(timeout=10)
            log.close()
            try:
                with (diagnostic_dir/"service-stderr-tail.bin").open("xb") as output:
                    with (fixture/"service.stderr").open("rb") as source_log:
                        source_log.seek(max(0,source_log.seek(0,2)-65536))
                        output.write(source_log.read(65536))
            except Exception as diagnostic_error:
                print("Diagnostic stderr retention failed: "+type(diagnostic_error).__name__, flush=True)


if __name__ == "__main__":
    main()
