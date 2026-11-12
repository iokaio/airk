# SPDX-License-Identifier: Apache-2.0
"""Binary/source-bound retrieval receipts through native HTTPS (historical stdio).

Default static vectors exercise adapter contracts; --corpus with --model-dir
measures a small original graded corpus using a catalog-pinned offline model.
Judgments stay outside indexed sources. Neither path proves general relevance.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import queue
import statistics
import subprocess
import socket
import uuid
from types import SimpleNamespace
import tempfile
import threading
import time

fixture_spec = importlib.util.spec_from_file_location("airk_service_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(fixture_spec)
fixture_spec.loader.exec_module(clients)
source_spec = importlib.util.spec_from_file_location("retrieval_reviewed_source", Path(__file__).with_name("reviewed-source-fixture.py"))
reviewed_source = importlib.util.module_from_spec(source_spec)
source_spec.loader.exec_module(reviewed_source)
judgments_spec = importlib.util.spec_from_file_location("retrieval_judgments", Path(__file__).with_name("retrieval_judgments.py"))
judgments = importlib.util.module_from_spec(judgments_spec)
judgments_spec.loader.exec_module(judgments)
evidence_spec = importlib.util.spec_from_file_location("combined_evidence", Path(__file__).with_name("combined_evidence.py"))
combined_evidence = importlib.util.module_from_spec(evidence_spec)
evidence_spec.loader.exec_module(combined_evidence)


http_spec = importlib.util.spec_from_file_location("retrieval_http", Path(__file__).with_name("http_fixture.py"))
http_fixture = importlib.util.module_from_spec(http_spec)
http_spec.loader.exec_module(http_fixture)
process_spec = importlib.util.spec_from_file_location("retrieval_processes", Path(__file__).with_name("owned-runtime-processes.py"))
processes = importlib.util.module_from_spec(process_spec)
process_spec.loader.exec_module(processes)


CASES = [
    {"name": "code", "query": "revoke_session", "mode": "lexical", "expected": "code/auth.rs"},
    {"name": "prose", "query": "recovery", "mode": "hybrid", "path_prefix": "docs/", "expected": "docs/recovery.md"},
    {"name": "synonym", "query": "login", "mode": "vector", "path_prefix": "code/", "expected": "code/auth.rs"},
    {"name": "minified", "query": "compacttoken", "mode": "lexical", "expected": "code/minified.js"},
    {"name": "long-line", "query": "tailmarker", "mode": "hybrid", "expected": "long.txt", "expected_text": "tailmarker"},
    {"name": "spanish", "query": "recuperar", "mode": "hybrid", "path_prefix": "es/", "expected": "es/access.txt"},
    {"name": "french", "query": "connexion", "mode": "hybrid", "path_prefix": "fr/", "expected": "fr/access.txt"},
    {"name": "alias", "query": "requirement", "mode": "hybrid", "path_prefix": "@specs/", "expected": "@specs/limits.md"},
    {"name": "sparse-scope", "query": "needle", "mode": "vector", "path_prefix": "wanted/", "expected": "wanted/only.txt"},
]


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def source_identity():
    repository = Path(__file__).resolve().parents[1]
    command = ["git", "-c", f"safe.directory={repository.as_posix()}", "-C", str(repository)]
    commit = subprocess.check_output(command + ["rev-parse", "HEAD"], text=True).strip()
    names = subprocess.check_output(command + ["ls-files", "-co", "--exclude-standard", "-z"]).decode().split("\0")
    source = hashlib.sha256()
    for name in sorted(set(names)):
        if not name or name.startswith(("temp/", "artifacts/", "target/", ".airk/")):
            continue
        path = repository / name
        if path.is_file() and not path.is_symlink():
            source.update(name.encode() + b"\0" + bytes.fromhex(digest(path)))
    dirty = bool(subprocess.check_output(command + ["status", "--porcelain", "--untracked-files=normal"]))
    return {"commit": commit, "dirty": dirty, "public_tree_sha256": source.hexdigest()}


def judge_combined(value, sources, grades):
    """Score an explicit index/root/path oracle without collapsing origins.

    `sources` maps (index_id, source_root_id, source_relative_path) to an exact
    source hash and its expected selected Project attribution. `grades` uses the
    same qualified keys. Callers freeze these judgments before candidate output.
    No canonical root is inferred from a virtual citation spelling.
    """
    if (value.get("schema") != "airk.combined-read/1" or not set(grades) <= set(sources)
            or any(type(grade) is not int or not 0 <= grade <= 3 for grade in grades.values())):
        raise AssertionError("qualified relevance oracle required")
    ranked = []
    members = value.get("members", [])
    member_ids = [member.get("index_id") for member in members]
    if len(member_ids) != len(set(member_ids)) or any(identity is None for identity in member_ids):
        raise AssertionError("qualified relevance member identity differs")
    vector = value.get("publication_vector", {}).get("members", [])
    if [member.get("index_id") for member in vector] != member_ids:
        raise AssertionError("qualified relevance publication vector differs")
    publications = {member["index_id"]: member.get("publication") for member in vector}
    try:
        for index in member_ids:
            combined_evidence.member_result(value, index)
    except (KeyError, TypeError, ValueError) as error:
        raise AssertionError("qualified relevance native metadata differs") from error
    for rank in value.get("ranking", []):
        try:
            origin = combined_evidence.ranked_origin(value, rank)
        except (KeyError, TypeError, ValueError) as error:
            raise AssertionError("qualified relevance citation source or scope differs") from error
        key = (origin.get("index_id"), origin.get("source_root_id"), origin.get("source_relative_path"))
        if (key not in sources or rank.get("index_id") != key[0] or key[0] not in member_ids
                or publications[key[0]] is None or origin.get("publication") != publications[key[0]]):
            raise AssertionError("qualified relevance citation source or scope differs")
        pointer = rank.get("result_pointer", "")
        if not isinstance(pointer, str) or not pointer.startswith("/members/"):
            raise AssertionError("qualified relevance citation locator differs")
        record = value
        try:
            parts = pointer[1:].split("/")
            if member_ids[int(parts[1])] != key[0]:
                raise AssertionError("qualified relevance citation member differs")
            for token in parts:
                token = token.replace("~1", "/").replace("~0", "~")
                record = record[int(token)] if isinstance(record, list) else record[token]
            source = sources[key]
            if (record["citation"]["content_hash"] != source["sha256"]
                    or origin.get("project_ids") != sorted(source["project_ids"])):
                raise AssertionError("qualified relevance citation source or scope differs")
        except (KeyError, TypeError, ValueError, IndexError) as error:
            raise AssertionError("qualified relevance citation locator differs") from error
        ranked.append(key)
    return {"ranking_units": "returned chunks; distinct index/root/path origins; repeated same-origin chunks consume rank",
            "partial": value.get("partial", False), "truncated": value.get("truncated", False),
            **judgments.score(ranked, grades)}


class Session:
    def __init__(self, command):
        environment = dict(os.environ)
        environment.pop("AIRK_USAGE_MODE", None)
        environment["AIRK_MCP_RESPONSE_MODE"] = "full"
        self.process = subprocess.Popen(command + ["mcp"], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=environment)
        self.messages = queue.Queue(maxsize=64)
        self.sequence = 0
        self.publication = None
        def read():
            try:
                while True:
                    line = self.process.stdout.readline(1_048_577)
                    if not line or len(line) > 1_048_576:
                        raise RuntimeError("MCP closed or exceeded response bound")
                    self.messages.put(json.loads(line), timeout=1)
            except (OSError, ValueError, RuntimeError, queue.Full) as error:
                try:
                    self.messages.put(error, timeout=1)
                except queue.Full:
                    pass
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        try:
            self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                       "clientInfo": {"name": "airk-qualification", "version": "1"}})
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except BaseException:
            self.close()
            raise

    def send(self, value):
        self.process.stdin.write(json.dumps(value).encode() + b"\n")
        self.process.stdin.flush()

    def request(self, method, arguments, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 120)
        clients.remaining(deadline, 120)
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": arguments})
        while True:
            try:
                message = self.messages.get(timeout=clients.remaining(deadline, 120))
            except queue.Empty as error:
                raise TimeoutError("MCP qualification deadline") from error
            if isinstance(message, BaseException):
                raise RuntimeError("MCP response reader failed") from message
            if not isinstance(message, dict):
                raise RuntimeError("MCP response is not an object")
            if message.get("id") == self.sequence:
                if "error" in message:
                    raise RuntimeError(str(message["error"]))
                return message["result"]
            if time.monotonic() > deadline:
                raise TimeoutError("MCP qualification deadline")

    def query(self, case, limit):
        arguments = {key: case[key] for key in ("query", "mode", "path_prefix", "project_ids", "index_ids", "member_availability") if key in case}
        arguments.update(limit=limit, max_bytes=65536, response_mode="full")
        start = time.perf_counter_ns()
        retry_deadline = time.monotonic() + 15
        params = {"name": "airk_search", "arguments": arguments}
        result, attempt = clients.read_envelope(lambda until: self.request("tools/call", params, until), retry_deadline, 601)
        elapsed = (time.perf_counter_ns() - start) / 1_000_000
        if result.get("isError"):
            raise RuntimeError(str(result["content"]))
        value = json.loads(result["content"][0]["text"])
        if value.get("schema") == "airk.combined-read/1":
            vector = value["publication_vector"]
            selected = (vector["authority"], tuple(member["index_id"] for member in vector["members"]))
            identity = json.dumps({"authority": vector["authority"], "members": vector["members"]}, sort_keys=True)
            observations = getattr(self, "combined_publications", {})
            if selected in observations and observations[selected] != identity:
                raise AssertionError("fixed fixture member publication changed during session")
            self.combined_publications = {**observations, selected: identity}
            return value, elapsed, attempt
        publication = value["publication"]["token"]
        if self.publication is not None and publication != self.publication:
            raise AssertionError("fixed fixture publication changed during session")
        self.publication = publication
        return value, elapsed, attempt

    def search(self, case):
        value, elapsed, attempt = self.query(case, 5)
        publication = self.publication
        paths = [hit["citation"]["path"] for hit in value["hits"]]
        rank = next((index + 1 for index, hit in enumerate(value["hits"])
                     if hit["citation"]["path"] == case["expected"]
                     and case.get("expected_text", "") in hit["text"]), None)
        if rank is None:
            raise AssertionError(f"fixed retrieval judgment failed: {case['name']}; returned {paths}")
        if value["hits"][rank - 1]["citation"]["content_hash"] != case["source_sha256"]:
            raise AssertionError(f"fixture citation hash differs: {case['name']}")
        return {"case": case["name"], "latency_ms": elapsed, "admission_retries": attempt, "recall_at_5": 1,
                "reciprocal_rank_at_5": 1 / rank, "publication": publication}

    def judged_search(self, case, source_hashes):
        value, elapsed, attempt = self.query(case, 10)
        paths = []
        for hit in value["hits"]:
            citation = hit["citation"]
            path = citation["path"]
            if (path not in source_hashes or not path.startswith(case.get("path_prefix", ""))
                    or citation["content_hash"] != source_hashes[path]):
                raise AssertionError("relevance citation source or scope differs")
            paths.append(path)
        return {"status": "ok", "latency_ms": elapsed, "admission_retries": attempt,
                "response_json_bytes": len(json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()),
                "truncated": value.get("truncated", False),
                **judgments.score(paths, case["grades"])}

    def judged_combined_search(self, case, sources, grades):
        value, elapsed, attempt = self.query(case, 10)
        return {"status": "ok", "latency_ms": elapsed, "admission_retries": attempt,
                "response_json_bytes": len(json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()),
                **judge_combined(value, sources, grades)}

    def compact_plan_probe(self, probe):
        """Retain the known compact/rich workflow comparison independently of rankings."""
        catalog = self.request("tools/list", {})
        parse = next(tool for tool in catalog["tools"] if tool["name"] in ("airk_parse", "parse"))
        probes = []
        for compact in (True, False):
            arguments = {"schema": "airk.parse.request/1", "mode": "execute", "response_mode": "full",
                         "plan": {"steps": [
                             {"id": "find", "tool": "symbols", "args": {
                                 "name": probe["name"], "path": probe["path"],
                                 "compact": compact, "limit": 5, "max_bytes": 8192}},
                             {"id": "structure", "tool": "file_info", "args": {}, "bind": [
                                 {"arg": "path", "from": "find", "select": "unique_definition_path"}]}]}}
            envelope, _ = clients.read_envelope(
                lambda until: self.request("tools/call", {"name": parse["name"], "arguments": arguments}, until),
                time.monotonic() + 15, 601)
            if envelope.get("isError"):
                probes.append({"compact": compact, "status": "error"})
                continue
            value = json.loads(envelope["content"][0]["text"])
            probes.append({"compact": compact, "status": value.get("status"),
                           "calls": value.get("calls"), "evidence_count": len(value.get("evidence", [])),
                           "steps": [{"id": step.get("id"), "status": step.get("status")}
                                     for step in value.get("steps", [])]})
        return {"plan_input_schema": parse["inputSchema"].get("properties", {}).get("plan"),
                "workflows": probes}

    def counters(self, metrics_module, binary):
        return metrics_module.process_metrics(self.process, binary)

    def close(self):
        try:
            if not self.process.stdin.closed:
                self.process.stdin.close()
            if self.process.wait(timeout=15) != 0:
                raise RuntimeError("MCP exited unsuccessfully during qualification shutdown")
        except (OSError, subprocess.TimeoutExpired) as error:
            self.process.kill()
            self.process.wait(timeout=15)
            raise RuntimeError("MCP did not shut down cleanly; child was terminated") from error
        finally:
            self.process.stdout.close()
            self.reader.join(timeout=2)



class HttpSession(Session):
    """Reuse ranking assertions over the canonical authenticated HTTPS fixture."""
    def __init__(self, service, name):
        self.service, self.sequence, self.publication = service, 0, None
        self.closed = False
        selected = {"kind": "scoped", "selection": service.scopes[name]}
        identity = uuid.uuid4().hex
        connection = service.enroll_http(selected, identity)
        expected, observed = dict(selected["selection"]), dict(connection["selection"]["selection"])
        expected.pop("expected_catalog_revision"); observed.pop("expected_catalog_revision")
        assert connection["connection_id"] == identity and connection["selection"]["kind"] == "scoped" and observed == expected, "Retrieval enrollment changed reviewed scope"
        health = service.rpc("health")["observation"]
        http_fixture.validate_native_identity(connection, health)
        assert health["instance"] == service.managed_health["instance"], "Retrieval service owner changed"
        self.network = http_fixture.McpClient(connection, service.control)
        try:
            self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                       "clientInfo": {"name": "airk-qualification", "version": "1"}})
            self.network.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            assert self.network.session is not None, "Native retrieval MCP session missing"
        except BaseException:
            self.close()
            raise

    def request(self, method, arguments, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 120)
        timeout = clients.remaining(deadline, 120)
        assert not self.closed, "Retrieval HTTP session is closed"
        self.network.connection.timeout = timeout
        if self.network.connection.sock is not None:
            self.network.connection.sock.settimeout(timeout)
        expired = threading.Event()
        def interrupt():
            expired.set()
            sock = self.network.connection.sock
            if sock is not None:
                try:
                    sock.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
        timer = threading.Timer(timeout, interrupt)
        timer.daemon = True
        timer.start()
        try:
            self.sequence += 1
            self.network.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": arguments})
            reply = self.network.receive(self.sequence)
            if expired.is_set() or time.monotonic() >= deadline:
                raise TimeoutError("Retrieval HTTP original deadline expired")
            assert reply.get("id") == self.sequence and "error" not in reply, "Native retrieval RPC refused"
            return reply["result"]
        finally:
            timer.cancel()
            timer.join()

    def counters(self, metrics_module, binary):
        health = self.service.rpc("health")["observation"]
        assert health["instance"] == self.service.managed_health["instance"]
        assert health["executable_sha256"] == digest(binary) == digest(self.service.binary)
        binary = self.service.binary.resolve(strict=True)
        resources = self.service.rpc("resources")
        if platform.system() not in ("Windows", "Linux"):
            return {"counter_basis": "OS counters unavailable in this harness; native service resource observation retained", "native_resources": resources, "scope": "shared native service, no MCP child process"}
        backend = processes._backend()
        owner = backend.open(health["process_id"])
        try:
            before = owner.observation()
            assert not owner.gone() and before["user"] == backend.user and not before.get("image_deleted")
            assert processes._key(before["path"]) == processes._key(binary)
            handle = SimpleNamespace(pid=owner.pid, _handle=getattr(owner, "handle", None))
            result = metrics_module.process_metrics(handle, binary)
            assert not owner.gone() and owner.observation() == before, "Measured service process identity changed"
            return {**result, "native_resources": resources, "scope": "shared native service, no MCP child process", "identity": before}
        finally:
            owner.close()

    def close(self):
        if not self.closed:
            self.closed = True
            self.network.close()


def open_session(service, name):
    if service.single_image:
        return HttpSession(service, name)
    return Session([str(service.binary), *service.arguments(name)])


def stats(records):
    values = sorted(record["latency_ms"] for record in records)
    return {"samples": len(values), "p50_ms": statistics.median(values),
            "p95_ms": values[math.ceil(len(values) * 0.95) - 1],
            "recall_at_5": statistics.mean(record["recall_at_5"] for record in records),
            "mrr_at_5": statistics.mean(record["reciprocal_rank_at_5"] for record in records)}


def require_source_change_contract(service):
    if service.catalog_authority and not service.single_image:
        raise ValueError("Historical catalog source-root review protocol is unsupported by this fixture")


def add_fixture_root(service, name, alias, root):
    require_source_change_contract(service)
    change = {"operation": "add_root", "alias": alias, "path": str(root)}
    if service.catalog_authority:
        # A reviewed Index requires native source admission and a newly reviewed
        # grant over the resulting source roster; never widen the prior grant.
        return reviewed_source.SourceReview(service, name).source_change(change)
    return service.configure(name, change)


def fixture(binary, base, count, trials, metrics_module, detail="balanced", cli=None):
    # owner/state is reserved for native-created, owner-private service state.
    root, state, alias = (base / name for name in ("root", "index-data", "alias"))
    root.mkdir(parents=True)
    alias.mkdir()
    sources = {
        "code/auth.rs": "fn revoke_session() {} // authenticate\n",
        "docs/recovery.md": "# Account access\nUse a recovery link after forgetting a password.\n",
        "code/minified.js": "const compacttoken=()=>{return 'authenticate'};" * 250,
        "long.txt": "banana " * 1500 + "tailmarker authenticate",
        "es/access.txt": "Puede recuperar su cuenta con una clave temporal.\n",
        "fr/access.txt": "La connexion utilise un code temporaire.\n",
        "wanted/only.txt": "needle banana\n",
    }
    for name, text in sources.items():
        path = root / name
        path.parent.mkdir(exist_ok=True)
        path.write_text(text, encoding="utf-8")
    (alias / "limits.md").write_text("The requirement sets a timeout of 30 seconds.\n", encoding="utf-8")
    vocabulary = {word: [1.0, 0.0] for word in ("authenticate", "login", "recovery", "password", "needle", "tailmarker", "recuperar", "connexion", "requirement")}
    vocabulary["banana"] = [0.0, 1.0]
    (root / "distractors").mkdir()
    for index in range(count):
        word = f"word{index}"
        vocabulary[word] = [1.0, (index + 1) / (count * 10)]
        (root / "distractors" / f"{index:05d}.txt").write_text(word + "\n", encoding="utf-8")
    state.mkdir()
    model = base / "model-input"
    model.mkdir(parents=True)
    (model / "vocabulary.json").write_text(json.dumps(vocabulary), encoding="utf-8")
    (model / "model.json").write_text(json.dumps({"schema": 1, "name": "original-qualification-fixture",
        "license": "Apache-2.0", "dimensions": 2, "sha256": digest(model / "vocabulary.json")}), encoding="utf-8")
    service = clients.Fixture(binary.parent, base, cli=cli)
    try:
        require_source_change_contract(service)
    except BaseException:
        service.log.close()
        raise
    assert service.binary == binary
    with service:
        service.register("qualification", root, state)
        add_fixture_root(service, "qualification", "specs", alias)
        service.configure("qualification", {"operation": "detail", "level": detail})
        service.configure("qualification", {"operation": "install_model", "directory": str(model)})
        service.job("qualification", "index")
        deadline = time.monotonic() + 300
        while True:
            status = service.published_status("qualification", deadline)
            readiness = clients.semantic_readiness(status)
            assert readiness["state"] != "failed", "Semantic work explicitly failed"
            if readiness["state"] == "ready" and readiness["completed_artifacts"] == count + 8:
                break
            assert time.monotonic() < deadline, readiness
            time.sleep(0.05)
        cases = [dict(case, source_sha256=digest(
            alias / case["expected"].removeprefix("@specs/")
            if case["expected"].startswith("@specs/") else root / case["expected"])) for case in CASES]
        modes = {}
        publications = set()
        for disabled in (True, False):
            service.configure("qualification", {"operation": "usage", "mode": "disabled" if disabled else "aggregates_only"})
            session = open_session(service, "qualification")
            try:
                cold = [session.search(cases[0])]
                warm = [session.search(case) for _ in range(trials) for case in cases]
                publications.add(session.publication)
                resources = session.counters(metrics_module, binary)
            finally:
                session.close()
            modes["usage_disabled" if disabled else "usage_enabled"] = {"cold": stats(cold), "warm": stats(warm),
                "process_counters": resources, "judgments": warm[:len(CASES)]}
        def concurrent(_):
            session = open_session(service, "qualification")
            try:
                return session.search(cases[-1])
            finally:
                session.close()
        with ThreadPoolExecutor(max_workers=4) as pool:
            parallel = list(pool.map(concurrent, range(8)))
        publications.update(record["publication"] for record in parallel)
        if len(publications) != 1:
            raise AssertionError("usage modes/processes observed different fixture publications")
        modes["four_https_clients" if service.single_image else "four_processes"] = stats(parallel)
        return {"distractors": count, "publication": publications.pop(),
                "source_hashes": {case["expected"]: case["source_sha256"] for case in cases},
                "modes": modes, "usage": json.loads(service.run(*service.management_arguments("qualification"), "usage", "report").stdout)["report"]}


def index_controls(service, root, source_hashes, publication, control):
    """Explicit owned-fixture jobs; never include this work in query latency."""
    def index():
        started = time.perf_counter_ns()
        job = service.job("relevance", "index")
        elapsed = (time.perf_counter_ns() - started) / 1_000_000
        page = service.rpc({"console": {"after": None, "revision": None, "limit": 32}})["page"]
        rows = [row for row in page["projects"]
                if row["project"]["registration"]["binding"] == service.projects["relevance"]["binding"]]
        progress = rows[0].get("progress") if len(rows) == 1 else None
        observed = progress.get("progress") if isinstance(progress, dict) else None
        if not isinstance(observed, dict) or observed.get("job") != job["id"]:
            observed = None
        return {"job_ms": elapsed, "publication": job["outcome"]["revision"],
                "source_progress": ({key: observed.get(key) for key in (
                    "captured_files", "extracted_files", "committed_changes", "publications", "phase_totals_ms")}
                    if observed is not None else None)}
    unchanged = index()
    assert unchanged["publication"] == publication, "No-op indexing changed the publication"
    if unchanged["source_progress"] is not None:
        assert unchanged["source_progress"]["extracted_files"] == 0, "No-op indexing repeated extraction"
        assert unchanged["source_progress"]["publications"] == 0, "No-op indexing repeated publication"
    # Change only generated fixture bytes and bind every expected post-edit hash.
    path = control["path"]
    original = (root / path).read_bytes()
    (root / path).write_bytes(original + control["append"].encode("utf-8"))
    expected = {**source_hashes, path: digest(root / path)}
    edited = index()
    assert edited["publication"] != publication, "Changed fixture failed to publish"
    if edited["source_progress"] is not None:
        assert edited["source_progress"]["extracted_files"] == 1, "One-file edit re-extracted unrelated sources"
    assert expected == {name: digest(root / name) for name in expected}, "Undeclared fixture source mutation"
    return {"unchanged": unchanged, "one_file_edit": edited, "edited_path": path,
            "before_hash": source_hashes[path], "after_hash": expected[path],
            "basis": "Explicit source-job elapsed time and exact-job native progress, after query samples. Semantic drain time is separate; no inference or physical-I/O claim."}


def representative_fixture(binary, base, corpus, corpus_hash, trials, detail, model, measurement=None, cli=None):
    """Measure actual pinned-model results; misses remain measurements, not test failures."""
    root, state = base / "root", base / "index-data"
    root.mkdir(parents=True)
    state.mkdir()
    for name, text in corpus["sources"].items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8", newline="\n")
    source_hashes = {name: digest(root / name) for name in corpus["sources"]}
    service = clients.Fixture(binary.parent, base, cli=cli)
    records = []
    measurement = measurement if measurement is not None else {}
    measurement.update(schema="airk.relevance-measurement/1", corpus_sha256=corpus_hash,
                       source_hashes=source_hashes, attempts=records, complete=False)
    with service:
        service.register("relevance", root, state)
        # Use the paired helper's canonical default object identity. Reimporting
        # an equivalent manifest with different JSON bytes is a custom identity,
        # which the existing semantic-upgrade guard deliberately does not adopt.
        print("Relevance fixture: prepare paired offline model", flush=True)
        service.job("relevance", "setup-model")
        service.configure("relevance", {"operation": "detail", "level": detail})
        service.configure("relevance", {"operation": "usage", "mode": "disabled"})
        service.job("relevance", "index")
        print("Relevance fixture: wait for complete semantic publication", flush=True)
        deadline = time.monotonic() + 300
        while True:
            status = service.published_status("relevance", deadline)
            readiness = clients.semantic_readiness(status)
            assert readiness["state"] != "failed", "Pinned-model semantic work failed"
            if readiness["state"] == "ready" and readiness["completed_artifacts"] == len(source_hashes):
                break
            assert time.monotonic() < deadline, readiness
            time.sleep(0.05)
        session = open_session(service, "relevance")
        try:
            print("Relevance fixture: compact/rich plans and graded queries", flush=True)
            plan_probe = (session.compact_plan_probe(corpus["compact_plan_probe"])
                          if corpus.get("compact_plan_probe") else {"state": "not_configured"})
            for trial in range(trials):
                for case in corpus["queries"]:
                    for mode in case["modes"]:
                        record = {"case": case["id"], "mode": mode, "split": case["split"],
                                  "family": case["family"], "trial": trial}
                        try:
                            record.update(session.judged_search({**case, "mode": mode}, source_hashes))
                        except (AssertionError, RuntimeError, TimeoutError, OSError, ValueError, KeyError, TypeError) as error:
                            # Keep every failed attempt; never export raw sources/error strings.
                            record.update(status="error", failure_class=type(error).__name__)
                        records.append(record)
        finally:
            session.close()
        assert source_hashes == {name: digest(root / name) for name in source_hashes}, "Relevance sources changed"
        controls = (index_controls(service, root, source_hashes, session.publication, corpus["index_control"])
                    if corpus.get("index_control") else {"state": "not_configured"})
    measurement.update({"schema": "airk.relevance-measurement/1", "corpus_sha256": corpus_hash,
            "source_hashes": source_hashes, "publication": session.publication,
            "verification_manifest_sha256": digest(model / "model.json"),
            "model_manifest": json.loads((model / "model.json").read_text(encoding="utf-8")),
            "model_source": "Native service bundled default; --model-dir independently verifies the same catalog-pinned manifest and assets, not a reimported model identity.",
            "ranking_unit": corpus["ranking_unit"], "judgment_scope": corpus["judgment_scope"],
            "limit": 10, "max_bytes": 65536, "attempts": records,
            "compact_plan_probe": plan_probe,
            "index_controls": controls,
            "cache_conditions": "One reused frontend after indexing and the compact/rich probes; these are warm observations, not cold startup timings.",
            "scores": judgments.summarize(records),
            "complete": all(record["status"] == "ok" for record in records)})
    return measurement


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="The runtime's airk service image")
    parser.add_argument("--airkctl", type=Path, help="A split pair's own airkctl (verified; derived from the pair when omitted)")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--detail", choices=("balanced", "agent_lean", "essential", "detailed"),
                        default="balanced", help="Explicit evidence mode under test")
    parser.add_argument("--corpus", type=Path, help="Original graded corpus; separate from synthetic static-vector qualification")
    parser.add_argument("--model-dir", type=Path, help="Verified pinned offline model directory, required with --corpus")
    args = parser.parse_args()
    if not 2 <= args.trials <= 30:
        parser.error("trials must be 2..30")
    if bool(args.corpus) != bool(args.model_dir):
        parser.error("--corpus and --model-dir must be supplied together")
    corpus, corpus_hash, model = None, None, None
    if args.corpus:
        corpus, corpus_hash = judgments.load(args.corpus)
        provisioner_spec = importlib.util.spec_from_file_location("pinned_model", Path(__file__).with_name("provision-local-model.py"))
        provisioner = importlib.util.module_from_spec(provisioner_spec)
        provisioner_spec.loader.exec_module(provisioner)
        model = args.model_dir.resolve(strict=True)
        manifest = json.loads((model / "model.json").read_text(encoding="utf-8"))
        profile = next((p for p in provisioner.CATALOG["profiles"] if p["manifest"] == manifest), None)
        if profile is None:
            parser.error("--model-dir must contain one catalog-pinned MiniLM profile")
        provisioner.provision(model, profile=profile)
    binary = args.binary.resolve(strict=True)
    # A split pair: model-catalog is an airkctl user command; airk answers
    # --version only behind the U93 interim selector.
    cli = clients.split_cli(binary, args.airkctl)
    if corpus:
        if cli:
            # Split airkctl answers model-catalog only from a running service; the offline
            # service-image smoke names the selected profile, as package-native.py reads it.
            smoke = json.loads(subprocess.check_output(
                [str(binary), "--verify-model-smoke"], text=True, encoding="utf-8", timeout=120))
            selected = [p for p in provisioner.CATALOG["profiles"] if p["id"] == smoke.get("profile")]
            runtime_manifest = selected[0]["manifest"] if len(selected) == 1 else None
        else:
            runtime_manifest = json.loads(subprocess.check_output(
                [str(binary), "model-catalog"], text=True, encoding="utf-8", timeout=15))["bundled_model"]["manifest"]
        if runtime_manifest != manifest:
            parser.error("--model-dir must match this runtime's selected bundled profile")
    spec = importlib.util.spec_from_file_location("scale_metrics", Path(__file__).with_name("benchmark-scale.py"))
    metrics = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(metrics)
    report = {"schema": "airk-retrieval-qualification/1", "source": source_identity(), "index_detail": args.detail,
        "binary_sha256": digest(binary), "binary_version": subprocess.check_output(
            [str(binary), *(clients.INTERIM_SELECTOR if cli else ()), "--version"], text=True, timeout=15).strip(),
        "airkctl_sha256": digest(cli) if cli else None,
        "platform": platform.platform(), "architecture": platform.machine(),
        "judgments_sha256": hashlib.sha256(json.dumps(CASES, sort_keys=True).encode()).hexdigest(),
        "cache_conditions": "cold means first query in a newly initialized MCP session; modern HTTPS shares the native service, historical stdio starts a frontend process; OS caches uncontrolled",
        "source_binding": "Observed checkout and binary digests must stay stable during this run. Local dirty-tree receipts do not prove binary build provenance; CI associates a clean checkout with its build.",
        "ranking_units": "Top five returned chunks, with each fixed judgment requiring its expected path and any expected text. Repeated paths retain their chunk rank.",
        "process_cleanup": "Modern HTTPS sessions send DELETE and close their sockets; historical MCP children must exit after EOF; each owned service must drain. No external provider or source deletion.",
        "scope": "Original synthetic static vectors and fixed judgments outside indexed sources. No production semantic-quality, larger-than-memory, physical-I/O, installed-editor or power-loss qualification.",
        "workloads": []}
    if corpus:
        report.update(judgments_sha256=corpus_hash, ranking_units=corpus["ranking_unit"],
                      cache_conditions="Reused frontend after indexing and compact/rich plan probes; OS caches uncontrolled. No cold-start claim.")
    retained_fixture = None
    try:
        with clients.owned_client_fixture() as temporary:
            retained_fixture = Path(temporary).name
            base = Path(temporary).resolve()
            if corpus:
                report["scope"] = "Small original file-judged corpus using the pinned offline model; lexical/vector/hybrid measured separately. No installed-editor or general relevance claim."
                report["representative"] = {}
                representative_fixture(binary, base, corpus, corpus_hash, args.trials, args.detail, model, report["representative"], cli)
            else:
                for count in (32, 256):
                    report["workloads"].append(fixture(binary, base / str(count), count, args.trials, metrics, args.detail, cli))
    except (AssertionError, RuntimeError, TimeoutError, OSError, ValueError, KeyError, TypeError) as error:
        report["failure_class"] = type(error).__name__
        if retained_fixture is not None:
            report["retained_fixture"] = retained_fixture
        # Diagnose fixture/contract failures without serializing source lines,
        # exception text, absolute private paths or frame locals.
        frames, frame, stage_failure = [], error.__traceback__, False
        while frame is not None:
            frames.append({"file": Path(frame.tb_frame.f_code.co_filename).name, "line": frame.tb_lineno})
            frames = frames[-12:]
            if Path(frame.tb_frame.f_code.co_filename).name == "check-service-clients.py" and frame.tb_frame.f_code.co_name in ("run", "service_run"):
                stage_failure = tuple(frame.tb_frame.f_locals.get("args", ())[:2]) == ("runtime", "stage")
            frame = frame.tb_next
        report["failure_frames"] = frames
        if stage_failure and os.environ.get("AIRK_PRIVATE_CENSUS_BASE"):
            # Runtime staging precedes enrollment and carries no credentials.
            # Preserve its bounded native refusal only in the operator's private
            # fixture area; the public receipt exposes no error text or paths.
            try:
                private = Path(os.environ["AIRK_PRIVATE_CENSUS_BASE"]).resolve(strict=True)
                directory = Path(tempfile.mkdtemp(prefix="airk-retrieval-stage-diagnostic-", dir=private))
                with (directory / "stage-error.txt").open("x", encoding="utf-8") as output:
                    output.write(str(error).encode("utf-8", errors="replace")[-8192:].decode("utf-8", errors="ignore"))
                report["private_stage_diagnostic"] = directory.name
            except (OSError, ValueError) as diagnostic_error:
                # Supplemental evidence must not replace the original failure
                # or prevent its source/binary-bound failed receipt. Keep even
                # the diagnostic's own message/path out of the public report.
                report["private_stage_diagnostic_error"] = type(diagnostic_error).__name__
    report["source_after"] = source_identity()
    report["binary_sha256_after"] = digest(binary)
    report["passed"] = ("failure_class" not in report and report["source"] == report["source_after"]
                        and report["binary_sha256"] == report["binary_sha256_after"]
                        and (not corpus or report["representative"]["complete"]))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "receipt": str(args.output),
                      "workloads": 1 if corpus else len(report["workloads"]), "cases": len(corpus["queries"]) if corpus else len(CASES)}))
    if not report["passed"]:
        raise SystemExit("Retrieval qualification failed; inspect the retained source/binary identities and failure/attempt receipts")


if __name__ == "__main__":
    main()
