# SPDX-License-Identifier: Apache-2.0
"""Opt-in real MiniLM/source-watcher fairness qualification in an owned fixture."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import queue
import re
import statistics
import subprocess
import tempfile
import threading
import time
import tomllib
import uuid


spec = importlib.util.spec_from_file_location(
    "source_fairness_pool", Path(__file__).with_name("check-service-pool.py"))
pool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pool)
clients = pool.clients
package_spec = importlib.util.spec_from_file_location(
    "source_fairness_package", Path(__file__).with_name("package-native.py"))
package = importlib.util.module_from_spec(package_spec)
package_spec.loader.exec_module(package)
BRIDGE_FRAME_LIMIT = 128 * 1024
REVISION_TOKEN = re.compile(r"airk-r1:([0-9a-f]{32}):(0|[1-9][0-9]{0,19}):([0-9a-f]{64})", re.ASCII)
QUERY_RECIPE = {"mode": "lexical", "verification": "off", "response_mode": "full",
                "limit": 8, "max_bytes": 8192}


def publication_identity(publication):
    token = publication if isinstance(publication, str) else publication.get("token") if isinstance(publication, dict) else None
    match = REVISION_TOKEN.fullmatch(token) if isinstance(token, str) and len(token) <= 126 else None
    if match is None or int(match[2]) > 18_446_744_073_709_551_615:
        raise RuntimeError("Native status returned an invalid bounded publication token")
    return match[1], int(match[2])


def validate_summary_status(value, collection):
    if not isinstance(value, dict) or value.get("index_state") != "available":
        code = ((value.get("index_error") or {}).get("code")
                if isinstance(value, dict) and isinstance(value.get("index_error"), dict) else None)
        known = code if code in ("index_status_unavailable", "index_worker_unavailable") else "unknown"
        raise RuntimeError(f"Native status summary is unavailable: code={known}")
    if not isinstance(value.get("publication"), dict):
        raise RuntimeError("Native status summary lacks its publication object")
    _, generation = publication_identity(value.get("publication"))
    if (value.get("collection") != collection or type(value.get("generation")) is not int
            or value["generation"] != generation
            or any(type(value.get(key)) is not int or value[key] < 0 for key in ("files", "chunks"))):
        raise RuntimeError("Native status summary has inconsistent scope/count/publication fields")
    readiness = clients.semantic_readiness(value)
    if (type(readiness.get("selected_artifacts")) is not int
            or readiness["selected_artifacts"] < readiness["completed_artifacts"]):
        raise RuntimeError("Native status summary has inconsistent semantic coverage counts")
    return value


def search_read_metadata(value, collection):
    """Retain the producer's native publication/coverage without copying hit payloads."""
    if not isinstance(value, dict) or value.get("collection") != collection:
        raise RuntimeError("Native search returned foreign collection metadata")
    if not isinstance(value.get("publication"), dict):
        raise RuntimeError("Native search lacks its publication object")
    _, generation = publication_identity(value.get("publication"))
    if type(value.get("generation")) is not int or value["generation"] != generation:
        raise RuntimeError("Native search returned inconsistent publication generation")
    readiness = clients.semantic_readiness(value)
    if (type(readiness.get("selected_artifacts")) is not int
            or readiness["selected_artifacts"] < readiness["completed_artifacts"]):
        raise RuntimeError("Native search returned inconsistent semantic coverage counts")
    return {"publication": value["publication"], "semantic_readiness": readiness}


def require_source_publication(value, before, job, collection):
    metadata = search_read_metadata(value, collection)
    incarnation, generation = publication_identity(metadata["publication"])
    prior_incarnation, prior_generation = publication_identity(before)
    outcome = job.get("outcome")
    if not isinstance(outcome, dict) or outcome.get("state") != "succeeded":
        raise RuntimeError("Source job lacks its canonical successful publication outcome")
    revision = outcome.get("revision")
    job_incarnation, job_generation = publication_identity(revision)
    token = metadata["publication"]["token"]
    if (incarnation != prior_incarnation or incarnation != job_incarnation
            or generation <= prior_generation or generation < job_generation
            or (generation == job_generation and token != revision)):
        raise RuntimeError("Source search publication does not advance and bind the successful source job")
    return metadata


def require_sample_phase(samples, expected, minimum_selected):
    metadata = samples.get("read_metadata")
    timings = samples.get("samples_ms")
    if not isinstance(metadata, list) or len(metadata) != 30 or not isinstance(timings, list) or len(timings) != 30:
        raise RuntimeError("Matched producer observations require exactly 30 complete samples")
    for index, observed in enumerate(metadata):
        readiness = observed.get("semantic_readiness") if isinstance(observed, dict) else None
        if not isinstance(readiness, dict):
            raise RuntimeError("Matched sample lacks native producer readiness")
        completed, selected = readiness.get("completed_artifacts"), readiness.get("selected_artifacts")
        counts_valid = (type(completed) is int and type(selected) is int
                        and 0 <= completed <= selected and selected >= minimum_selected)
        correct_phase = False
        if counts_valid and expected == "backlog":
            correct_phase = readiness.get("state") in ("pending", "partial") and completed < selected
        elif counts_valid and expected == "quiet":
            correct_phase = readiness.get("state") == "ready" and completed == selected
        if not correct_phase:
            raise RuntimeError(f"Actual native sample {index} did not retain {expected} coverage; "
                               "increase --files if the backlog completed during observation")


def active_vector_with_inference(value, project):
    """A current, still-running grant already entered inference, even between batches."""
    if not isinstance(value, dict) or value.get("project") != project or value.get("project_paused") is not False:
        return False
    observation = value.get("observation")
    if not isinstance(observation, dict):
        return False
    job, work, worker = (observation.get(key) for key in ("job", "progress", "worker"))
    if not all(isinstance(item, dict) for item in (job, work, worker)):
        return False
    epoch = observation.get("current_fencing_generation")
    totals = work.get("phase_totals_ms")
    embedding_ms = totals.get("embedding") if isinstance(totals, dict) else None
    inferred = work.get("phase") == "embedding" or (type(embedding_ms) is int and embedding_ms > 0)
    return (job.get("kind") == "vectors" and job.get("state") == "running"
            and isinstance(job.get("id"), str) and bool(job["id"])
            and work.get("job") == job["id"]
            and job.get("project") == worker.get("project") == project
            and type(epoch) is int and epoch > 0
            and type(job.get("fencing_generation")) is int
            and type(worker.get("fencing_generation")) is int
            and epoch == job["fencing_generation"] == worker["fencing_generation"]
            and work.get("paused") is False and work.get("stalled") is False
            and work.get("phase") in ("preparing", "publishing", "embedding") and inferred)


class BridgeTransportFailure(RuntimeError):
    def __init__(self, observation):
        self.observation = observation
        super().__init__("Owned MCP transport failed: " + json.dumps(observation, sort_keys=True))


class ObservedBridge(pool.Bridge):
    """Fixture-local bounded transport instrumentation; never captures payload text."""
    _TERMINAL = object()

    def __init__(self, binary, runtime, binding, environment, collection, deadline=None):
        self.binding, self.collection = binding, collection
        self.responses = queue.Queue(maxsize=4)
        self.sequence = 0
        self._transport_lock = threading.Lock()
        self._transport = {"frame_limit_bytes": BRIDGE_FRAME_LIMIT, "frames_observed": 0,
                           "max_frame_prefix_bytes": 0, "terminal": None}
        self._closing = self._closed = False
        self.process = subprocess.Popen(
            [str(binary), "bridge", "--runtime-dir", str(runtime), "--project", binding],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            env=environment, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.reader = threading.Thread(target=self._read_frames, args=(self.process.stdout,), daemon=True)
        self.reader.start()
        try:
            self.call("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "airk-source-fairness-fixture", "version": "1"}}, deadline)
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except Exception:
            self.close()
            raise

    def transport_observation(self):
        with self._transport_lock:
            value = dict(self._transport)
            if isinstance(value["terminal"], dict):
                value["terminal"] = dict(value["terminal"])
        reader = getattr(self, "reader", None)
        value["reader_alive"] = reader is not None and reader.is_alive()
        value["process_exit_code"] = self.process.poll()
        return value

    def _terminal(self, kind, prefix_bytes=0):
        with self._transport_lock:
            if self._transport["terminal"] is None:
                self._transport["terminal"] = {"kind": kind, "observed_frame_prefix_bytes": prefix_bytes,
                                               "during_close": self._closing}
        try:
            self.responses.put_nowait(self._TERMINAL)
        except queue.Full:
            pass  # The sticky terminal state is checked before/after dequeue.

    def _read_frames(self, output):
        while True:
            try:
                line = output.readline(BRIDGE_FRAME_LIMIT + 1)
            except (OSError, ValueError):
                self._terminal("read_error")
                return
            if not line:
                self._terminal("eof")
                return
            with self._transport_lock:
                self._transport["frames_observed"] += 1
                self._transport["max_frame_prefix_bytes"] = max(
                    self._transport["max_frame_prefix_bytes"], len(line))
            if len(line) > BRIDGE_FRAME_LIMIT:
                self._terminal("oversized_frame", len(line))
                return
            try:
                value = json.loads(line)
            except (ValueError, RecursionError):
                self._terminal("invalid_json", len(line))
                return
            if not isinstance(value, dict):
                self._terminal("invalid_response_shape", len(line))
                return
            try:
                self.responses.put_nowait(value)
            except queue.Full:
                self._terminal("response_queue_full", len(line))
                return

    def call(self, method, params, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 120)
        clients.remaining(deadline, 120)
        if self.transport_observation()["terminal"] is not None:
            raise BridgeTransportFailure(self.transport_observation())
        self.sequence += 1
        try:
            self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        except (OSError, ValueError):
            self._terminal("write_error")
            raise BridgeTransportFailure(self.transport_observation()) from None
        try:
            response = self.responses.get(timeout=clients.remaining(deadline, 120))
        except queue.Empty:
            self._terminal("response_timeout")
            raise BridgeTransportFailure(self.transport_observation()) from None
        if response is self._TERMINAL or self.transport_observation()["terminal"] is not None:
            raise BridgeTransportFailure(self.transport_observation())
        if response.get("id") != self.sequence or "error" in response or "result" not in response:
            self._terminal("invalid_rpc_response")
            raise BridgeTransportFailure(self.transport_observation())
        return response["result"]

    def published_status(self, deadline=None):
        value = self.read("airk_status", {"response_mode": "full"}, deadline)
        return validate_summary_status(value, self.collection)

    def close(self):
        if self._closed:
            return
        self._closed = self._closing = True
        try:
            self.process.stdin.close()
        except OSError:
            pass
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=10)
        self.reader.join(timeout=5)
        self.process.stdout.close()


def staged_runtime_root(directory, *, approved_baseline_source=None):
    """Use the canonical package validator before any fixture process starts."""
    build_command = ("python scripts/build-service-runtime.py "
                     "--destination <staged-runtime> --profile debug")
    try:
        runtime = directory.resolve(strict=True)
        if not (runtime / "airk-runtime.json").is_file():
            raise ValueError("missing airk-runtime.json; raw target/debug binaries are not a verified pair")
        architecture = {"amd64": "x86_64", "x86_64": "x86_64",
                        "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
        suffix = {"Windows": "pc-windows-msvc", "Darwin": "apple-darwin",
                  "Linux": "unknown-linux-gnu"}.get(platform.system())
        target = f"{architecture}-{suffix}"
        if target not in package.TARGETS:
            raise ValueError("unsupported native qualification host")
        version = tomllib.loads((package.ROOT / "Cargo.toml").read_text(encoding="utf-8"))[
            "workspace"]["package"]["version"]
        source = (package.runtime_builder().source_identity() if approved_baseline_source is None
                  else approved_baseline_source)
        package.validate_runtime(runtime, version, target, source)
    except (ValueError, OSError) as error:
        if approved_baseline_source is not None:
            raise ValueError(f"--runtime failed canonical retained-baseline validation: {error}. "
                             "Supply the exact retained staged pair and its approved source manifest; "
                             "a current-source rebuild is a different candidate.") from error
        raise ValueError(f"--runtime failed canonical staged-pair validation: {error}. "
                         f"Build the current source with: {build_command}") from error
    return runtime


def source_snapshot(source):
    result = {}
    total_bytes = 0
    for entry, path in enumerate(source.rglob("*")):
        if entry >= 4096:
            raise RuntimeError("Owned source snapshot exceeded its entry bound")
        if path.is_symlink():
            raise RuntimeError("Unexpected redirected path appeared inside owned source fixture")
        if path.is_file():
            size = path.stat().st_size
            total_bytes += size
            if size > 64 * 1024 or total_bytes > 16 * 1024 * 1024:
                raise RuntimeError("Owned source snapshot exceeded its byte bound")
            result[path.relative_to(source).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return dict(sorted(result.items()))


def source_snapshot_digest(snapshot):
    return hashlib.sha256(json.dumps(snapshot, sort_keys=True).encode()).hexdigest()


def write_backlog_corpus(source, files):
    for number in range(files):
        content = "\n".join(
            f"Document {number:04d}, paragraph {paragraph:02d}: bounded copper beacon research keeps unique durable evidence."
            for paragraph in range(24)) + "\n"
        (source / f"backlog-{number:04d}.md").write_text(content, encoding="utf-8")


def baseline_load_contract(candidate_path, manifest_path, files):
    candidate = package.bounded_json(candidate_path)
    manifest = package.bounded_json(manifest_path)
    source = manifest.get("native_source")
    if (not isinstance(source, dict) or set(source) != {"sha256", "files"}
            or not isinstance(source.get("sha256"), str) or re.fullmatch(r"[0-9a-f]{64}", source["sha256"]) is None
            or type(source.get("files")) is not int or source["files"] < 1):
        raise ValueError("Approved baseline source manifest lacks its exact native_source identity")
    matched = candidate.get("matched_load")
    if (candidate.get("status") != "passed" or not isinstance(matched, dict)
            or matched.get("source_bytes_unchanged") is not True
            or matched.get("corpus_files") != files + 3
            or not isinstance(matched.get("corpus_sha256"), str)
            or re.fullmatch(r"[0-9a-f]{64}", matched["corpus_sha256"]) is None):
        raise ValueError("Candidate receipt does not contain a passed matching final-corpus observation")
    require_sample_phase(matched.get("backlog", {}), "backlog", files + 3)
    candidate_samples = matched["backlog"]
    if (any(candidate_samples.get(key) != QUERY_RECIPE[key] for key in ("mode", "verification", "limit", "max_bytes"))
            or candidate_samples.get("query", "copper beacon") != "copper beacon"):
        raise ValueError("Candidate query recipe/budgets differ from the fixed matched-load recipe")
    if (not isinstance(candidate.get("runtime_binary_sha256"), str)
            or re.fullmatch(r"[0-9a-f]{64}", candidate["runtime_binary_sha256"]) is None
            or any(item["semantic_readiness"]["selected_artifacts"] != files + 3
                   for item in matched["backlog"]["read_metadata"])):
        raise ValueError("Candidate runtime/corpus identity does not match the complete load cohort")
    completed = [item["semantic_readiness"]["completed_artifacts"]
                 for item in matched["backlog"]["read_metadata"]]
    return {"approved_native_source": source, "approved_source_manifest_sha256": package.sha256(manifest_path),
            "candidate_receipt_sha256": package.sha256(candidate_path),
            "candidate_runtime_binary_sha256": candidate.get("runtime_binary_sha256"),
            "query": "copper beacon",
            "candidate_query_basis": "explicit receipt" if "query" in candidate_samples else "retained fixture's fixed copper beacon recipe",
            "corpus_sha256": matched["corpus_sha256"], "corpus_files": matched["corpus_files"],
            "entry_completed_artifacts": {"minimum": completed[0], "maximum": max(completed)}}


def admission_order(events, source_job, window_start_unix_ms, original_vector, *,
                    boundary_sequence=None, original_witness_unix_ms=None,
                    original_witness_sequence_floor=None):
    """Select a durable writer interval at admission; never infer it from late progress."""
    if (not isinstance(events, list) or len(events) > 512
            or type(window_start_unix_ms) is not int or window_start_unix_ms < 0):
        raise RuntimeError("Invalid bounded admission timeline")
    if any(not isinstance(event, dict) or type(event.get("sequence")) is not int or event["sequence"] < 1
           or type(event.get("observed_unix_ms")) is not int or event["observed_unix_ms"] < 0 for event in events):
        raise RuntimeError("Unknown or duplicate admission event identity/clock")
    ordered = sorted(events, key=lambda event: event["sequence"])
    sequences = [event["sequence"] for event in ordered]
    if len(set(sequences)) != len(sequences):
        raise RuntimeError("Unknown or duplicate admission event identity/clock")
    source = next((event for event in ordered
                   if event.get("kind") == "job_running" and event.get("job") == source_job), None)
    if source is None:
        raise RuntimeError("No retained running event for the completed source job")
    prefix = [event for event in ordered if event["sequence"] <= source["sequence"]]
    if any(later["observed_unix_ms"] < earlier["observed_unix_ms"]
           for earlier, later in zip(prefix, prefix[1:])):
        raise RuntimeError("Admission event clock moved backwards; duration is unknown")
    manual = boundary_sequence is not None
    if manual:
        if type(boundary_sequence) is not int or boundary_sequence < 1:
            raise RuntimeError("Invalid manual admission sequence")
        pending = next((event for event in ordered if event["sequence"] == boundary_sequence
                        and event.get("job") == source_job and event.get("kind") == "job_pending"), None)
        if (pending is None or pending["observed_unix_ms"] != window_start_unix_ms
                or boundary_sequence >= source["sequence"]):
            raise RuntimeError("Manual admission boundary lacks its exact source pending event")
    elif source["observed_unix_ms"] == window_start_unix_ms:
        raise RuntimeError("Ambiguous equal-time automatic admission boundary")
    vector_events = [event for event in ordered if event.get("job_kind") == "vectors"]
    prior_starts = [event for event in vector_events if event.get("kind") == "job_running"
                    and event["sequence"] < source["sequence"]]
    for event in prior_starts:
        if not manual and event["observed_unix_ms"] == window_start_unix_ms:
            raise RuntimeError("Ambiguous equal-time automatic Vector start")
        after = (event["sequence"] > boundary_sequence if manual
                 else event["observed_unix_ms"] > window_start_unix_ms)
        if after:
            raise RuntimeError("A new Vector grant overtook admissible source work: " + json.dumps(event))
    jobs = {event["job"] for event in prior_starts}
    jobs.update(event["job"] for event in vector_events
                if event["sequence"] < source["sequence"]
                and event.get("kind") in ("job_succeeded", "job_failed", "job_canceled"))
    if original_vector is not None:
        jobs.add(original_vector)
    intervals = []
    ignored_completed = []
    for job in sorted(jobs):
        starts = [event for event in prior_starts if event.get("job") == job]
        terminals = [event for event in vector_events if event.get("job") == job
                     and event.get("kind") in ("job_succeeded", "job_failed", "job_canceled")]
        if len(starts) > 1 or len(terminals) != 1 or terminals[0].get("kind") != "job_succeeded":
            raise RuntimeError("Vector interval lacks a unique retained successful completion")
        end = terminals[0]
        if end["sequence"] >= source["sequence"]:
            raise RuntimeError("Source started before the Vector completion receipt")
        if not manual and end["observed_unix_ms"] == window_start_unix_ms:
            raise RuntimeError("Ambiguous equal-time automatic Vector completion")
        start = starts[0] if starts else None
        lower_bound = None
        if start is not None:
            if end["sequence"] <= start["sequence"] or end["observed_unix_ms"] < start["observed_unix_ms"]:
                raise RuntimeError("Unknown Vector interval order/clock")
        elif job == original_vector:
            if (type(original_witness_unix_ms) is not int
                    or type(original_witness_sequence_floor) is not int
                    or original_witness_sequence_floor < 0
                    or original_witness_unix_ms > end["observed_unix_ms"]
                    or original_witness_sequence_floor >= end["sequence"]):
                raise RuntimeError("Original running Vector lacks its known-running witness lower bound")
            lower_bound = {"observed_unix_ms": original_witness_unix_ms,
                           "event_sequence_floor": original_witness_sequence_floor,
                           "interpretation": "Known running here; actual start predates retained cursor and is not fabricated"}
        else:
            completed_before = (end["sequence"] < boundary_sequence if manual
                                else end["observed_unix_ms"] < window_start_unix_ms)
            if completed_before:
                ignored_completed.append(end)
                continue
            raise RuntimeError("Unknown Vector interval could contain the admission boundary")
        if manual:
            if start is not None:
                began_before = start["sequence"] < boundary_sequence
            else:
                if (original_witness_sequence_floor >= boundary_sequence
                        or original_witness_unix_ms > window_start_unix_ms):
                    raise RuntimeError("Known-running witness cannot order the manual boundary")
                began_before = True
            contains = began_before and end["sequence"] > boundary_sequence
        else:
            lower_time = start["observed_unix_ms"] if start is not None else original_witness_unix_ms
            if lower_time == window_start_unix_ms:
                raise RuntimeError("Ambiguous equal-time automatic writer lower bound")
            if lower_time > window_start_unix_ms:
                raise RuntimeError("Writer lower bound is after the automatic observation boundary")
            contains = end["observed_unix_ms"] > window_start_unix_ms
        intervals.append({"job": job, "running_event": start, "known_running_lower_bound": lower_bound,
                          "succeeded_event": end, "contains_boundary": contains})
    for index, left in enumerate(intervals):
        for right in intervals[index + 1:]:
            a, b = left["running_event"], right["running_event"]
            if a is not None and b is not None:
                overlaps = max(a["sequence"], b["sequence"]) < min(
                    left["succeeded_event"]["sequence"], right["succeeded_event"]["sequence"])
            else:
                left_lower = a["observed_unix_ms"] if a is not None else left["known_running_lower_bound"]["observed_unix_ms"]
                right_lower = b["observed_unix_ms"] if b is not None else right["known_running_lower_bound"]["observed_unix_ms"]
                overlaps = max(left_lower, right_lower) < min(
                    left["succeeded_event"]["observed_unix_ms"], right["succeeded_event"]["observed_unix_ms"])
            if overlaps:
                raise RuntimeError("Overlapping Vector writer intervals prevent a unique admission proof")
    active = [interval for interval in intervals if interval["contains_boundary"]]
    if len(active) > 1:
        raise RuntimeError("Overlapping Vector writer intervals contain the admission boundary")
    interval = active[0] if active else None
    end = interval["succeeded_event"] if interval is not None else None
    if end is not None and end["observed_unix_ms"] < window_start_unix_ms:
        raise RuntimeError("Active writer completion clock predates admission")
    remaining = (end["observed_unix_ms"] - window_start_unix_ms) / 1000 if end is not None else 0
    start_lag = max(0, (source["observed_unix_ms"] - window_start_unix_ms) / 1000)
    return {"source_running_event": source, "existing_vector_finished_event": end,
            "active_vector_at_boundary": interval["job"] if interval is not None else None,
            "active_vector_interval": interval, "vector_intervals": intervals,
            "completed_before_boundary_without_start": ignored_completed,
            "boundary_kind": "exact_manual_pending_sequence" if manual else "automatic_post_observation_timestamp",
            "boundary_sequence": boundary_sequence,
            "source_already_running_at_boundary": not manual and source["observed_unix_ms"] < window_start_unix_ms,
            "existing_turn_remaining_seconds": remaining, "source_start_lag_seconds": start_lag,
            "source_start_excess_over_turn_seconds": max(0, start_lag - remaining), "new_vector_overtakers": 0}


def automatic_reconcile_job(events, witness_unix_ms, after_sequence):
    """Bind an automatic source turn to new Reconcile admission, not an old queued job."""
    accepted = {event["job"]: event["sequence"] for event in events
                if event.get("kind") == "job_pending" and event.get("job_kind") == "reconcile"
                and event.get("observed_unix_ms", -1) >= witness_unix_ms
                and event.get("sequence", -1) > after_sequence}
    return next((event["job"] for event in events
                 if event.get("kind") == "job_running" and event.get("job_kind") == "reconcile"
                 and event.get("job") in accepted and event["sequence"] > accepted[event["job"]]
                 and event.get("observed_unix_ms", -1) >= witness_unix_ms), None)


def retain_wave_events(record, fresh):
    available = max(0, 512 - len(record["events"]))
    record["events"].extend(fresh[:available])
    if len(fresh) > available:
        record["events_truncated"] = True
        record["unretained_event_count"] = len(fresh) - available
        raise RuntimeError("Source wave exceeded the retained 512-event proof budget")


class Fixture:
    def __init__(self, binary, fixture, environment):
        self.binary, self.fixture, self.environment = binary, fixture, environment
        self.runtime = fixture / "runtime"
        self.log_path = fixture / "service.stderr"
        self.log = self.log_path.open("wb")
        self.service = None
        self.bridges = []
        self.minimum_selected = 0
        self.flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)

    def run(self, *arguments, timeout=120):
        result = subprocess.run([str(self.binary), *map(str, arguments)], capture_output=True,
                                env=self.environment, timeout=timeout, encoding="utf-8",
                                creationflags=self.flags)
        if result.returncode:
            raise RuntimeError(f"Owned native command exited {result.returncode}: {result.stderr[-4096:]}")
        if len(result.stdout) > 1024 * 1024:
            raise RuntimeError("Owned native command exceeded the fixture output bound")
        return json.loads(result.stdout)

    def rpc(self, method, deadline=None):
        path = self.fixture / "request.json"

        def invoke(exact, until):
            path.write_text(exact, encoding="utf-8")
            return self.run("service", "rpc", "--runtime-dir", self.runtime,
                            "--request-file", path, timeout=clients.remaining(until, 90))
        return clients.metadata_rpc(invoke, method, deadline)

    def start(self):
        self.service = subprocess.Popen(
            [str(self.binary), "serve", "--runtime-dir", str(self.runtime)],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=self.log,
            env=self.environment, creationflags=self.flags)
        until = time.monotonic() + 90
        while True:
            try:
                if self.run("service", "health", "--runtime-dir", self.runtime)["status"] == "health":
                    return
            except RuntimeError:
                pass
            if time.monotonic() >= until or self.service.poll() is not None:
                raise RuntimeError("Owned service did not become healthy")
            time.sleep(0.1)

    def register(self, source, data, collection):
        source.mkdir()
        data.mkdir()
        result = self.run("--root", source, "--data-dir", data, "--collection", collection,
                          "service", "register", "--runtime-dir", self.runtime)
        if result.get("status") != "registered":
            raise RuntimeError("Fixture project registration was not accepted")
        return result["project"]["registration"]

    def enqueue(self, project, kind):
        epoch = self.rpc("request_epoch")
        result = self.rpc({"enqueue": {"project": project, "kind": kind,
            "caller": uuid.uuid4().hex, "key": {"namespace": epoch["namespace"],
                "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}}})
        if result.get("status") != "job":
            raise RuntimeError(f"Fixture {kind} admission failed: {result}")
        return result["job"]

    def wait_job(self, job, timeout=180):
        until = time.monotonic() + timeout
        while True:
            receipt = self.rpc({"job": {"id": job}}, until)["job"]
            if receipt["state"] == "succeeded":
                return receipt
            if receipt["state"] in ("failed", "canceled") or time.monotonic() >= until:
                raise RuntimeError(f"Fixture job did not succeed: {receipt}")
            time.sleep(0.1)

    def bridge(self, project, deadline=None):
        bridge = ObservedBridge(self.binary, self.runtime, project["binding"], self.environment,
                                project["collection"], deadline)
        self.bridges.append(bridge)
        return bridge

    def events(self, project, cursor=None):
        events = []
        until = time.monotonic() + 30
        for _ in range(16):
            page = self.rpc({"events": {"project": project, "cursor": cursor, "limit": 32}}, until)["page"]
            if page.get("retained_prefix_lost"):
                raise RuntimeError("Fixture event prefix was lost; cannot prove admission order")
            events.extend(page["events"])
            if page["next"] is None:
                return events, page["tail"]
            cursor = page["next"]
        raise RuntimeError("Fixture event inventory exceeded 512 bounded events")

    def watch(self, project, deadline=None):
        return self.rpc({"watch": {"project": project}}, deadline)["observation"]

    def progress(self, project, deadline=None):
        return self.rpc({"progress": {"project": project}}, deadline)

    def running_vectors(self, project, bridge):
        until = time.monotonic() + 180
        last = None
        while True:
            status = bridge.published_status(until)
            readiness = clients.semantic_readiness(status)
            progress = self.progress(project, until)
            watch = self.watch(project, until)
            if (active_vector_with_inference(progress, project)
                    and readiness["state"] in ("pending", "partial")
                    and readiness["completed_artifacts"] < readiness["selected_artifacts"]
                    and watch["requested_revision"] == watch["completed_revision"]):
                return status, progress
            last = {"readiness": readiness, "progress": progress}
            if (readiness["state"] == "ready"
                    and readiness["selected_artifacts"] >= self.minimum_selected):
                raise RuntimeError("Corpus completed before a running Vector backlog was observed; "
                                   "increase --files to qualify this host")
            if readiness["state"] == "failed" or time.monotonic() >= until:
                raise RuntimeError(f"No running multi-turn Vector backlog: {last}")
            time.sleep(0.1)

    def close(self):
        failures = []
        for bridge in self.bridges:
            try:
                bridge.close()
            except Exception as error:
                failures.append(str(error)[-1024:])
        if self.service is not None and self.service.poll() is None:
            try:
                self.run("service", "shutdown", "--runtime-dir", self.runtime)
                self.service.wait(timeout=60)
            except (RuntimeError, subprocess.TimeoutExpired):
                self.service.kill()
                self.service.wait(timeout=10)
                failures.append("Owned service required forced fixture cleanup")
        self.log.close()
        return failures


def query_sample(bridge, query, deadline):
    begin = time.monotonic()
    result = bridge.read("airk_search", dict(QUERY_RECIPE, query=query), deadline)
    if not result.get("hits"):
        raise RuntimeError("Lexical fixture probe lost its citation")
    elapsed = (time.monotonic() - begin) * 1000
    return elapsed, search_read_metadata(result, bridge.collection)


def query_samples(bridge, query, count=30, first_sample=None):
    samples = []
    metadata = []
    until = time.monotonic() + 180
    if first_sample is not None:
        samples.append(first_sample[0])
        metadata.append(first_sample[1])
    for _ in range(count - len(samples)):
        elapsed, observed = query_sample(bridge, query, until)
        samples.append(elapsed)
        metadata.append(observed)
    return {"samples_ms": samples, "median_ms": statistics.median(samples), "query": query,
            "read_metadata": metadata,
            "verification": "off", "mode": "lexical", "limit": 8, "max_bytes": 8192,
            "deadline_seconds": 180, "retry_contract": "Canonical typed retry; all attempts included in elapsed time"}


def require_path(bridge, query, path, present=True, expected_hash=None):
    result = bridge.read("airk_search", {"query": query, "mode": "lexical", "verification": "off",
        "response_mode": "full", "limit": 8, "max_bytes": 8192})
    matching = [hit for hit in result.get("hits", []) if isinstance(hit, dict)
                and isinstance(hit.get("citation"), dict) and hit["citation"].get("path") == path]
    if bool(matching) != present:
        raise RuntimeError(f"Source reconciliation citation mismatch for {path}: {result}")
    if present and (not isinstance(expected_hash, str)
                    or any(hit["citation"].get("content_hash") != expected_hash for hit in matching)):
        raise RuntimeError(f"Source reconciliation citation hash mismatch for {path}")
    return result


def await_backlog_exists(fixture, bridge):
    until = time.monotonic() + 180
    while True:
        status = bridge.published_status(until)
        readiness = status["semantic_readiness"]
        if readiness["selected_artifacts"] >= fixture.minimum_selected:
            if (readiness["state"] in ("pending", "partial")
                    and readiness["completed_artifacts"] < readiness["selected_artifacts"]):
                return {"publication": status["publication"], "readiness": readiness}
            if readiness["state"] == "ready":
                raise RuntimeError("Backlog completed before the bounded diagnostics probe; increase --files")
        if readiness["state"] == "failed" or time.monotonic() >= until:
            raise RuntimeError("No incomplete full-corpus backlog for the diagnostics causality probe")
        time.sleep(0.1)


def full_diagnostics_probe(fixture, project, receipt):
    """One isolated observation; a timeout with a live reader remains a blocker."""
    started = time.monotonic()
    until = started + 10
    observation = {"tool": "airk_diagnostics", "response_mode": "full", "deadline_seconds": 10,
                   "stage": "initialize", "payload_captured": False,
                   "retry_contract": "Canonical complete typed read rejection only; no transport replay",
                   "historical_timeout": "Earlier uninstrumented timeout remains unattributed"}
    receipt["full_diagnostics_probe"] = observation
    probe = None
    try:
        probe = fixture.bridge(project, until)
        observation["stage"] = "diagnostics"
        probe.read("airk_diagnostics", {"response_mode": "full"}, until)
        observation["outcome"] = "complete"
        observation["transport"] = probe.transport_observation()
    except BridgeTransportFailure as failure:
        observation["outcome"] = "transport_failure"
        observation["transport"] = failure.observation
        kind = (failure.observation.get("terminal") or {}).get("kind")
        if kind == "response_timeout" and failure.observation.get("reader_alive"):
            raise RuntimeError("Bounded full-diagnostics probe timed out with its reader alive; "
                               "actual read responsiveness remains unqualified") from failure
        if kind != "oversized_frame":
            raise RuntimeError("Bounded full-diagnostics probe ended with an unexplained native/transport "
                               f"failure: kind={kind}") from failure
        observation["interpretation"] = "Observed reply exceeded this fixture's fixed reader cap; summary route remains separately qualified"
    except Exception as failure:
        observation["outcome"] = "read_failure"
        observation["failure_type"] = type(failure).__name__
        if probe is not None:
            observation["transport"] = probe.transport_observation()
        raise
    finally:
        observation["elapsed_seconds"] = time.monotonic() - started
        if probe is not None:
            probe.close()


def matched_query_observations(fixture, project, bridge, source, expected):
    """Observe the same final corpus during backlog and after it becomes ready."""
    if source_snapshot(source) != expected:
        raise RuntimeError("Source changed before matched-load observation")
    watch = fixture.watch(project["binding"])
    if watch["enabled"]:
        raise RuntimeError("Matched final-corpus observations require the disabled-watch manual wave")
    before = bridge.published_status()
    backlog_resources_before = fixture.rpc("resources")
    backlog = query_samples(bridge, "copper beacon")
    require_sample_phase(backlog, "backlog", fixture.minimum_selected)
    after = bridge.published_status()
    if source_snapshot(source) != expected:
        raise RuntimeError("Source changed during matched backlog observations")
    backlog_resources_after = fixture.rpc("resources")
    bridge.wait_semantics()
    quiet_before = bridge.published_status()
    quiet = query_samples(bridge, "copper beacon")
    require_sample_phase(quiet, "quiet", fixture.minimum_selected)
    quiet_after = bridge.published_status()
    if source_snapshot(source) != expected:
        raise RuntimeError("Source changed between matched backlog and quiet observations")
    all_metadata = backlog["read_metadata"] + quiet["read_metadata"]
    if any(publication_identity(observed["publication"])[0] != publication_identity(all_metadata[0]["publication"])[0]
           for observed in all_metadata):
        raise RuntimeError("Matched observations crossed an index incarnation")
    return {"corpus_sha256": source_snapshot_digest(expected),
            "corpus_files": len(expected), "source_bytes_unchanged": True,
            "watch": watch,
            "backlog": backlog, "quiet": quiet,
            "backlog_before": backlog["read_metadata"][0], "backlog_after": backlog["read_metadata"][-1],
            "quiet_before": quiet["read_metadata"][0], "quiet_after": quiet["read_metadata"][-1],
            "summary_observations": {"backlog_before": before, "backlog_after": after,
                                     "quiet_before": quiet_before, "quiet_after": quiet_after},
            "backlog_resources_before": backlog_resources_before,
            "backlog_resources_after": backlog_resources_after,
            "quiet_resources": fixture.rpc("resources"),
            "thresholds_applied": False,
            "cohort_labels": {"backlog": "SEMANTIC-BACKLOG", "quiet": "READY-STAGE"},
            "interpretation": "Same-candidate load effect only. READY-STAGE can include running jobs; "
                              "it is not an idle or prior-version control. Regression requires a matched baseline backlog."}


def baseline_load_observation(fixture, project, bridge, source, files, contract, record):
    record.update({"status": "running", "stage": "prepare_final_corpus",
                   "source_priority_assertions": False, "contract": contract})
    record["watch"] = fixture.watch(project["binding"])
    if record["watch"]["enabled"]:
        raise RuntimeError("Baseline final-corpus load observation requires watching disabled")
    write_backlog_corpus(source, files)
    (source / "memo.md").write_text("Manual heliotrope falcon source publication.\n", encoding="utf-8")
    (source / "added.md").write_text("Added cerulean orchard publication.\n", encoding="utf-8")
    (source / "deleted.md").unlink()
    (source / "rename-old.md").rename(source / "renamed.md")
    expected = source_snapshot(source)
    actual_corpus_hash = source_snapshot_digest(expected)
    record.update({"generated_corpus_files": len(expected), "generated_corpus_sha256": actual_corpus_hash})
    if len(expected) != contract["corpus_files"] or actual_corpus_hash != contract["corpus_sha256"]:
        raise RuntimeError("Baseline final corpus differs from the approved candidate corpus/hash")
    record["stage"] = "index_final_corpus"
    job = fixture.enqueue(project["binding"], "index")
    record["source_job"] = fixture.wait_job(job["id"])
    record["stage"] = "await_comparable_actual_producer_coverage"
    until = time.monotonic() + 180
    probes = 0
    minimum = contract["entry_completed_artifacts"]["minimum"]
    maximum = contract["entry_completed_artifacts"]["maximum"]
    while True:
        first = query_sample(bridge, "copper beacon", until)
        probes += 1
        metadata = first[1]
        readiness = metadata["semantic_readiness"]
        record.update({"entry_probes": probes, "last_entry_metadata": metadata})
        if readiness["selected_artifacts"] >= contract["corpus_files"]:
            completed = readiness["completed_artifacts"]
            if readiness["state"] not in ("pending", "partial") or completed >= readiness["selected_artifacts"]:
                raise RuntimeError("Baseline backlog completed before comparable producer coverage")
            if completed > maximum:
                raise RuntimeError("Baseline producer coverage passed the approved candidate entry range")
            if completed >= minimum:
                break
        if time.monotonic() >= until:
            raise RuntimeError("Baseline did not reach the approved producer coverage entry range")
        time.sleep(0.1)
    record["stage"] = "sample_matched_backlog"
    record["resources_before"] = fixture.rpc("resources")
    samples = query_samples(bridge, "copper beacon", first_sample=first)
    record["backlog"] = samples
    require_sample_phase(samples, "backlog", contract["corpus_files"])
    record["resources_after"] = fixture.rpc("resources")
    if source_snapshot(source) != expected:
        raise RuntimeError("Baseline source bytes changed during matched backlog observation")
    record.update({"status": "passed", "stage": "complete", "corpus_sha256": source_snapshot_digest(expected),
                   "corpus_files": len(expected), "source_bytes_unchanged": True,
                   "backlog_before": samples["read_metadata"][0], "backlog_after": samples["read_metadata"][-1],
                   "thresholds_applied": False,
                   "interpretation": "Retained-runtime backlog control only; no source-priority claim or current-source relabeling"})


def wave(fixture, project, bridge, source, expected, label, manual, max_overhead, record):
    record.update({"label": label, "manual_watch_disabled": manual,
                   "status": "running", "stage": "observe_event_cursor", "events": []})
    try:
        return _wave(fixture, project, bridge, source, expected, label, manual, max_overhead, record)
    except Exception as failure:
        record["status"] = "failed"
        record["failure_type"] = type(failure).__name__
        raise


def _wave(fixture, project, bridge, source, expected, label, manual, max_overhead, record):
    binding = project["binding"]
    # Start retaining events before the running-Vector witness, so an RPC gap
    # cannot hide a replacement Vector admitted after that observation.
    _, cursor = fixture.events(binding)
    record["event_cursor_before_witness"] = cursor
    source_after_sequence = cursor["after"]
    record["stage"] = "await_vector_witness"
    before, running = fixture.running_vectors(binding, bridge)
    before_completed = before["semantic_readiness"]["completed_artifacts"]
    active_vector = running["observation"]["job"]["id"]
    original_observed_unix_ms = running["observed_unix_ms"]
    record.update({"running_vector_before_edits": running, "publication_before": before["publication"],
                   "readiness_before": before["semantic_readiness"], "original_vector_job": active_vector,
                   "original_vector_observed_unix_ms": original_observed_unix_ms,
                   "stage": "observe_watch_before_edits"})
    baseline_watch = fixture.watch(binding)
    record["watch_before_edits"] = baseline_watch
    started = time.monotonic()
    if manual:
        record["stage"] = "disable_watch_for_manual_source"
        fixture.rpc({"configure_watch": {"project": binding, "enabled": False}})
        if fixture.watch(binding)["enabled"]:
            raise RuntimeError("Manual wave did not disable automatic source watching")
        path = source / "memo.md"
        path.write_text("Manual heliotrope falcon source publication.\n", encoding="utf-8")
        expected["memo.md"] = hashlib.sha256(path.read_bytes()).hexdigest()
        accepted = fixture.enqueue(binding, "index")
        record["accepted_source_job"] = accepted
        record["source_job_id"] = accepted["id"]
        due = fixture.progress(binding)
        due_clock = due["observed_unix_ms"]
        source_job = accepted["id"]
    else:
        record["stage"] = "apply_source_edit_wave"
        (source / "memo.md").write_text("Edited vermilion lighthouse publication.\n", encoding="utf-8")
        (source / "added.md").write_text("Added cerulean orchard publication.\n", encoding="utf-8")
        (source / "deleted.md").unlink()
        (source / "rename-old.md").rename(source / "renamed.md")
        for path in ("memo.md", "added.md"):
            expected[path] = hashlib.sha256((source / path).read_bytes()).hexdigest()
        del expected["deleted.md"]
        expected["renamed.md"] = expected.pop("rename-old.md")
        until = time.monotonic() + 45
        record["stage"] = "await_native_watch_revision"
        while True:
            observed = fixture.watch(binding, until)
            if observed["requested_revision"] > baseline_watch["requested_revision"]:
                if observed["backend"] != "native":
                    raise RuntimeError("Source fairness requires the actual native watcher")
                desired = observed["requested_revision"]
                record.update({"watch_after_edits": observed, "desired_revision": desired,
                               "stage": "observe_source_admissibility"})
                due = fixture.progress(binding)
                due_clock = due["observed_unix_ms"]
                break
            if time.monotonic() >= until:
                raise RuntimeError("Native watcher did not retain the fixture edit wave")
            time.sleep(0.05)
        source_job = None
    record.update({"due_progress": due, "source_admissibility_observed_unix_ms": due_clock,
                   "stage": "await_source_publication"})
    resources = fixture.rpc("resources")
    record["resources_under_backlog"] = resources
    until = time.monotonic() + 180
    all_events = record["events"]
    while True:
        fresh, cursor = fixture.events(binding, cursor)
        retain_wave_events(record, fresh)
        record["event_cursor"] = cursor
        if not manual and source_job is None:
            source_job = automatic_reconcile_job(all_events, original_observed_unix_ms, source_after_sequence)
        if source_job:
            record["source_job_id"] = source_job
            job = fixture.rpc({"job": {"id": source_job}}, until)["job"]
            record["source_job"] = job
            if job["state"] in ("failed", "canceled"):
                raise RuntimeError(f"Source wave failed: {job}")
            completed = job["state"] == "succeeded"
            if not manual:
                watch = fixture.watch(binding, until)
                record["last_watch"] = watch
                completed = completed and watch["completed_revision"] >= desired
            if completed:
                break
        if time.monotonic() >= until:
            raise RuntimeError(f"Source publication did not finish during backlog: {all_events[-8:]}")
        time.sleep(0.1)
    record["stage"] = "validate_source_publication_and_order"
    fresh, cursor = fixture.events(binding, cursor)
    retain_wave_events(record, fresh)
    record["event_cursor"] = cursor
    manual_pending_sequence = None
    if manual:
        acceptance = next((event for event in all_events
                           if event.get("kind") == "job_pending" and event.get("job") == source_job), None)
        if acceptance is None:
            raise RuntimeError("Manual source admission lacks its exact retained pending event")
        due_clock = acceptance["observed_unix_ms"]
        manual_pending_sequence = acceptance["sequence"]
        record["source_admissibility_observed_unix_ms"] = due_clock
    record["summary_status_after_source"] = bridge.published_status()
    record["stage"] = "read_source_evidence"
    query = "heliotrope falcon" if manual else "vermilion lighthouse"
    published = require_path(bridge, query, "memo.md", expected_hash=expected["memo.md"])
    published_at = time.monotonic()
    record["primary_source_read"] = published
    source_metadata = require_source_publication(published, before["publication"], job, project["collection"])
    readiness = source_metadata["semantic_readiness"]
    record.update({"publication_after_source": source_metadata["publication"], "readiness_after_source": readiness})
    if readiness["completed_artifacts"] >= readiness["selected_artifacts"]:
        raise RuntimeError("Source did not publish before the semantic backlog completed; "
                           "increase --files if the fixture lacked a persistent backlog")
    order = admission_order(
        all_events, source_job, due_clock, active_vector,
        boundary_sequence=manual_pending_sequence,
        original_witness_unix_ms=original_observed_unix_ms,
        original_witness_sequence_floor=source_after_sequence)
    record["order"] = order
    order["proof_window"] = ("exact manual source admission event" if manual
                             else "post-observation of increased native-watch requested revision")
    order["window_start_unix_ms"] = due_clock
    order["original_vector_observed_unix_ms"] = original_observed_unix_ms
    pre_observation = [event for event in all_events
                       if event.get("kind") == "job_running" and event.get("job_kind") == "vectors"
                       and event.get("job") != active_vector
                       and original_observed_unix_ms <= event["observed_unix_ms"] < due_clock]
    order["pre_observation_vector_starts"] = pre_observation
    order["original_vector_role"] = "Pre-edit backlog witness; not assumed to remain the writer at admission"
    if not manual:
        order["automatic_pre_observation_gap_ms"] = due_clock - original_observed_unix_ms
        order["automatic_gap_interpretation"] = (
            "Only ordering after the server observation timestamp is proved here; "
            "the preceding source-event observation gap is covered by separate Store admission/race tests")
    source_finished = next((event for event in all_events
                            if event.get("kind") == "job_succeeded" and event.get("job") == source_job), None)
    if source_finished is None:
        raise RuntimeError("Source completion lacks its exact retained success event")
    order["source_completion_event"] = source_finished
    order["source_execution_seconds"] = max(
        0, (source_finished["observed_unix_ms"] - order["source_running_event"]["observed_unix_ms"]) / 1000)
    publication_lag = published_at - started
    # This is an operator-selected allowance, separate from soft Vector turn
    # budgets. The receipt always reports raw timings for phase-gate evaluation.
    excess = max(0, publication_lag - order["existing_turn_remaining_seconds"])
    record.update({"publication_lag_seconds": publication_lag, "source_overhead_seconds": excess,
                   "selected_max_overhead_seconds": max_overhead, "stage": "validate_source_citations"})
    if max_overhead is not None and excess > max_overhead:
        raise RuntimeError(f"Source overhead {excess:.3f}s exceeds selected allowance {max_overhead:.3f}s")
    if not manual:
        require_path(bridge, "cerulean orchard", "added.md", expected_hash=expected["added.md"])
        require_path(bridge, "indigo sundial", "renamed.md", expected_hash=expected["renamed.md"])
        require_path(bridge, "indigo sundial", "rename-old.md", False)
        require_path(bridge, "saffron dragonfly", "deleted.md", False)
    after_source_count = readiness["completed_artifacts"]
    until = time.monotonic() + 180
    record["stage"] = "await_vector_resumption"
    while True:
        resumed = bridge.published_status(until)
        resumed_readiness = resumed["semantic_readiness"]
        record["last_resume_status"] = resumed
        resumed_incarnation, resumed_generation = publication_identity(resumed["publication"])
        source_incarnation, source_generation = publication_identity(published["publication"])
        if (resumed_readiness["completed_artifacts"] > after_source_count
                and resumed_incarnation == source_incarnation and resumed_generation > source_generation):
            break
        if resumed_readiness["state"] == "failed" or time.monotonic() >= until:
            raise RuntimeError("Saved Vector continuation did not advance after source publication")
        time.sleep(0.1)
    if source_snapshot(source) != expected:
        raise RuntimeError("AIRK changed fixture source files beyond explicit edit wave")
    record.update({"label": label, "manual_watch_disabled": manual,
            "running_vector_before_edits": running, "publication_before": before["publication"],
            "readiness_before": before["semantic_readiness"], "due_progress": due,
            "source_job": job, "order": order, "events": all_events,
            "publication_lag_seconds": publication_lag, "source_overhead_seconds": excess,
            "selected_max_overhead_seconds": max_overhead,
            "publication_after_source": published["publication"], "readiness_after_source": readiness,
            "publication_after_resume": resumed["publication"],
            "readiness_after_resume": resumed_readiness,
            "completed_before_edits": before_completed,
            "resources_under_backlog": resources, "status": "passed", "stage": "complete"})
    return record, expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--files", type=int, default=384)
    parser.add_argument("--max-overhead-seconds", type=float)
    parser.add_argument("--load-only-baseline", action="store_true",
                        help="Measure a retained runtime on the approved candidate's final backlog corpus; skip source-priority assertions")
    parser.add_argument("--candidate-receipt", type=Path,
                        help="Passed candidate source-fairness receipt defining the final corpus and producer coverage entry range")
    parser.add_argument("--approved-source-manifest", type=Path,
                        help="Explicit retained baseline native_source manifest; accepted only with --load-only-baseline")
    args = parser.parse_args()
    if not 128 <= args.files <= 2048:
        parser.error("--files must be between 128 and 2048 (multiple existing Vector turns)")
    if args.max_overhead_seconds is not None and (
            not math.isfinite(args.max_overhead_seconds) or args.max_overhead_seconds <= 0):
        parser.error("--max-overhead-seconds must be finite and positive")
    contract = None
    if args.load_only_baseline:
        if args.candidate_receipt is None or args.approved_source_manifest is None:
            parser.error("--load-only-baseline requires --candidate-receipt and --approved-source-manifest")
        if args.output.resolve() in (args.candidate_receipt.resolve(), args.approved_source_manifest.resolve()):
            parser.error("--output must preserve the candidate receipt and approved source manifest")
        try:
            contract = baseline_load_contract(args.candidate_receipt, args.approved_source_manifest, args.files)
        except (ValueError, OSError, RuntimeError, AssertionError) as error:
            parser.error(f"Invalid approved baseline inputs: {error}")
    elif args.candidate_receipt is not None or args.approved_source_manifest is not None:
        parser.error("Retained source/candidate inputs require explicit --load-only-baseline; ordinary qualification uses current source")
    try:
        runtime = staged_runtime_root(args.runtime, approved_baseline_source=(
            contract["approved_native_source"] if contract is not None else None))
    except ValueError as error:
        parser.error(str(error))
    binary = runtime / ("airk.exe" if os.name == "nt" else "airk")
    if not binary.is_file():
        binary = runtime / "AIRK.app/Contents/MacOS/airk"
    binary = binary.resolve(strict=True)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1",
                       HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    receipt = {"schema": "airk.native-source-fairness/1", "status": "running", "waves": [],
               "mode": "load-only-baseline" if args.load_only_baseline else "source-fairness",
               "source_priority_assertions": not args.load_only_baseline,
               "files": args.files, "runtime_binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "runtime_native_source": package.bounded_json(runtime / "airk-build.json", 65_536)["source"],
               "limits": {"operation_deadline_seconds": 180, "event_budget": 512,
                          "soft_vector_turn_artifacts": 32, "soft_vector_turn_seconds": 30,
                          "max_source_overhead_seconds": None if args.load_only_baseline else args.max_overhead_seconds}}
    error = None
    fixture_base = os.environ.get("AIRK_PRIVATE_CENSUS_BASE")
    if fixture_base is not None:
        fixture_base = Path(fixture_base).resolve(strict=True)
        if not fixture_base.is_dir():
            parser.error("AIRK_PRIVATE_CENSUS_BASE must name an existing owned fixture directory")
    with tempfile.TemporaryDirectory(prefix="airk-source-fairness-", dir=fixture_base) as temporary:
        fixture = Fixture(binary, Path(temporary), environment)
        source = Path(temporary) / "source"
        try:
            fixture.start()
            project = fixture.register(source, Path(temporary) / "data", "source-fairness")
            initial = {"memo.md": "Copper beacon quiet lexical fixture.\n",
                       "deleted.md": "Saffron dragonfly deletion fixture.\n",
                       "rename-old.md": "Indigo sundial rename fixture.\n"}
            for name, content in initial.items():
                (source / name).write_text(content, encoding="utf-8")
            fixture.wait_job(fixture.enqueue(project["binding"], "model_setup")["id"])
            fixture.wait_job(fixture.enqueue(project["binding"], "index")["id"])
            bridge = fixture.bridge(project)
            bridge.wait_semantics()
            if args.load_only_baseline:
                receipt["matched_load"] = {}
                baseline_load_observation(fixture, project, bridge, source, args.files, contract, receipt["matched_load"])
                receipt["source_snapshot_sha256"] = receipt["matched_load"]["corpus_sha256"]
                receipt["source_bytes_preserved"] = receipt["matched_load"]["source_bytes_unchanged"]
            else:
                receipt["small_corpus_control"] = {
                    "files": len(initial), "query_latency": query_samples(bridge, "copper beacon"),
                    "resources": fixture.rpc("resources"),
                    "interpretation": "Three-file control only; not the matched backlog comparison"}
                fixture.rpc({"configure_watch": {"project": project["binding"], "enabled": True}})
                until = time.monotonic() + 45
                while True:
                    watch = fixture.watch(project["binding"])
                    if watch["backend"] == "native" and watch["requested_revision"] == watch["completed_revision"]:
                        break
                    if time.monotonic() >= until:
                        raise RuntimeError("Native watch did not settle before backlog generation")
                    time.sleep(0.1)
                write_backlog_corpus(source, args.files)
                expected = source_snapshot(source)
                fixture.minimum_selected = args.files + len(initial)
                receipt["backlog_before_diagnostics_probe"] = await_backlog_exists(fixture, bridge)
                full_diagnostics_probe(fixture, project, receipt)
                automated = {}
                receipt["waves"].append(automated)
                _, expected = wave(fixture, project, bridge, source, expected,
                                   "native_watch_edit_add_delete_rename", False, args.max_overhead_seconds, automated)
                manual = {}
                receipt["waves"].append(manual)
                _, expected = wave(fixture, project, bridge, source, expected,
                                   "accepted_source_watch_disabled", True, args.max_overhead_seconds, manual)
                receipt["matched_load"] = matched_query_observations(
                    fixture, project, bridge, source, expected)
                receipt["source_snapshot_sha256"] = source_snapshot_digest(expected)
                receipt["source_bytes_preserved"] = source_snapshot(source) == expected
            receipt["status"] = "passed"
        except Exception as failure:
            error = failure
            receipt["status"] = "failed"
            receipt["error"] = str(failure)[-8192:]
            if isinstance(receipt.get("matched_load"), dict) and receipt["matched_load"].get("status") == "running":
                receipt["matched_load"]["status"] = "failed"
            if "project" in locals():
                diagnostics = {}
                for name, method in (
                        ("progress", {"progress": {"project": project["binding"]}}),
                        ("watch", {"watch": {"project": project["binding"]}}),
                        ("resources", "resources"), ("embedding", "embedding_status")):
                    try:
                        diagnostics[name] = fixture.rpc(method, time.monotonic() + 5)
                    except Exception as diagnostic_error:
                        diagnostics[name] = {"unavailable": str(diagnostic_error)[-512:]}
                receipt["failure_diagnostics"] = diagnostics
        finally:
            receipt["bridge_transports_before_cleanup"] = [bridge.transport_observation()
                                                           for bridge in fixture.bridges]
            receipt["cleanup_failures"] = fixture.close()
            with fixture.log_path.open("rb") as log:
                log.seek(max(0, fixture.log_path.stat().st_size - 8192))
                receipt["service_stderr_tail"] = log.read(8192).decode("utf-8", errors="replace")
            if receipt["cleanup_failures"] and error is None:
                receipt["status"] = "failed"
                error = RuntimeError("Owned fixture cleanup did not complete gracefully")
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": receipt["status"], "receipt": str(args.output)}), flush=True)
    if error is not None:
        raise RuntimeError(receipt.get("error", str(error))) from error


if __name__ == "__main__":
    main()
