# SPDX-License-Identifier: Apache-2.0
"""Private observational taps; native SDK, assertions, dispatch and replay remain owners."""
import base64
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import threading
import time


class Capture:
    def __init__(self, path):
        self.origin = time.monotonic()
        self.path = Path(path)
        self.stream = self.path.open("x", encoding="utf-8")
        self.lock = threading.Lock()
        self.rows, self.errors = [], []
        self.local = threading.local()
        self.active_admission = None

    def emit(self, kind, **fields):
        # Observation failures never replace an original native/parser exception.
        # In-memory full rows remain available and final capture_complete refuses.
        with self.lock:
            row = {"kind": kind, "monotonic": time.monotonic(), **fields}
            try:
                raw = json.dumps(row, ensure_ascii=False, allow_nan=False)
                frozen = json.loads(raw)
                self.rows.append(frozen)
                self.stream.write(raw + "\n")
                self.stream.flush()
            except BaseException as error:
                self.errors.append({"type": type(error).__name__, "message": str(error)})

    def close(self):
        try:
            self.stream.close()
        except BaseException as error:
            self.errors.append({"type": type(error).__name__, "message": str(error)})


def failure(error):
    return {"type": type(error).__name__, "message": str(error)}


def tap_network(network, capture, admission, client, fixture):
    # Patch only this actual connection's observation methods. Timeout/socket
    # assignments continue reaching the real SDK connection without a proxy.
    connection = network.connection
    request, getresponse, receive = connection.request, connection.getresponse, network.receive
    state = {"attempt": 0, "request": None}
    label = {"admission": admission, "client_kind": "https-mcp", "client": client,
             "fixture": str(fixture)}

    def observed_request(method, path, body=None, headers=None, *args, **kwargs):
        raw = body.encode() if isinstance(body, str) else body
        if raw is not None:
            try:
                value = json.loads(raw)
            except (ValueError, UnicodeError):
                value = None
        else:
            value = None
        if isinstance(value, dict) and value.get("method") == "tools/call":
            state["attempt"] += 1
        state["request"] = {"method": method, "path": path, "value": value}
        # Use the actual SDK's redaction owner, including this client's session.
        try:
            safe_headers = network.response_error(0, b"", list((headers or {}).items()), method,
                                                   value, path, failure="capture").evidence()
            capture.emit("wire_request", **label, attempt=state["attempt"],
                         method=method, path=path, json=value,
                         body_base64=base64.b64encode(raw).decode() if raw is not None else None,
                         body_sha256=hashlib.sha256(raw).hexdigest() if raw is not None else None,
                         headers=safe_headers["headers"],
                         header_redactions=safe_headers["header_redactions"])
        except BaseException as observer:
            capture.errors.append(failure(observer))
        return request(method, path, body, headers, *args, **kwargs)

    def observed_getresponse(*args, **kwargs):
        response = getresponse(*args, **kwargs)
        read = response.read

        def observed_read(*read_args, **read_kwargs):
            raw = read(*read_args, **read_kwargs)  # Same exact read; no extra bytes.
            req = state["request"]
            try:
                evidence = network.response_error(response.status, raw, response.getheaders(),
                    req["method"], req["value"], req["path"], failure="capture",
                    truncated=len(raw) > 1_048_576).evidence()
                capture.emit("wire_response_before_sdk_validation", **label,
                             attempt=state["attempt"], original_body_sha256=hashlib.sha256(raw).hexdigest(),
                             observed_bytes=len(raw), sdk_read_arguments=list(read_args), evidence=evidence)
            except BaseException as observer:
                capture.errors.append(failure(observer))
            return raw
        response.read = observed_read
        return response

    def observed_receive(identifier, *args, **kwargs):
        value = receive(identifier, *args, **kwargs)
        try:
            encoded = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()
            safe = network.response_error(0, encoded, [], "POST", state["request"]["value"],
                                          "/mcp", failure="capture").evidence()
            capture.emit("native_reply_before_caller_validation", **label,
                         attempt=state["attempt"], identifier=identifier, envelope=safe["body"],
                         body_redacted=safe["body_redacted"])
        except BaseException as observer:
            capture.errors.append(failure(observer))
        return value  # Preserve exact object and all future/unknown fields.

    connection.request, connection.getresponse, network.receive = observed_request, observed_getresponse, observed_receive
    network._cold_capture_label = label
    return network


def tracked_executor(capture):
    class ObservedExecutor(ThreadPoolExecutor):
        def map(self, function, *iterables, timeout=None, chunksize=1):
            # Executor.map's generator cancels queued siblings after first error.
            # Keep the exact selected inputs, join every future, then re-raise
            # the first original exception in input order. No retry/new budget.
            deadline = None if timeout is None else time.monotonic() + timeout
            futures = [self.submit(function, *args) for args in zip(*iterables)]
            def all_results():
                values, first = [], None
                for future in futures:
                    try:
                        value = future.result(timeout=None if deadline is None else max(0, deadline-time.monotonic()))
                    except BaseException as error:
                        if not future.done():
                            raise  # Caller interruption/timeout is not a worker result.
                        values.append(None)
                        if first is None:
                            first = error
                    else:
                        values.append(value)
                if first is not None:
                    raise first
                yield from values
            return all_results()

        def submit(self, function, /, *args, **kwargs):
            def observed():
                label = getattr(args[0], "_cold_capture_label", None) if args else None
                label = label or {"admission": capture.active_admission, "client_kind": "https-mcp",
                                  "client_ordinal": args[0] if args and type(args[0]) is int else None}
                capture.local.label = label
                try:
                    value = function(*args, **kwargs)
                except BaseException as error:
                    capture.emit("sibling_outcome", function=function.__name__, **label,
                                 outcome="error", error=failure(error))
                    raise
                else:
                    capture.emit("sibling_outcome", function=function.__name__, **label,
                                 outcome="complete", value=value if isinstance(value, (dict, list, str, int, bool, type(None))) else {"python_type":type(value).__name__})
                    return value
                finally:
                    capture.local.label = {}
            return super().submit(observed)
    return ObservedExecutor


@contextmanager
def observe_clients(clients, processes, capture, expected_user):
    original_fixture = clients.Fixture
    original_cold = clients.cold_first_status
    original_read = clients.read_envelope
    original_executor = clients.ThreadPoolExecutor

    class ObservedFixture(original_fixture):
        def start(self):
            self._held = None
            super().start()
            health = self.managed_health
            backend = processes._backend()
            held = backend.open(health["process_id"])
            self._held = held
            observed = held.observation()
            if (held.gone() or observed["user"] != backend.user or backend.user != expected_user
                    or health["user"] != expected_user or not Path(observed["path"]).samefile(self.binary)):
                raise AssertionError("Private diagnostic handle differs from native owner")
            capture.emit("native_process_handle_acquired", fixture=str(self.owner),
                         native_health=health, actual_process=observed, signals_sent=0)

        def open_http(self, name, client=None):
            network = super().open_http(name, client)
            return tap_network(network, capture, self._cold_admission, client, self.owner)

        def close(self):
            primary = None
            try:
                return super().close()
            except BaseException as error:
                primary = error
                raise
            finally:
                held = getattr(self, "_held", None)
                if held is not None:
                    try:
                        # StopComplete is the native authority. Read the retained
                        # OS handle once afterward; no signal or new wait budget.
                        gone = held.gone()
                        capture.emit("native_process_handle_after_stop", fixture=str(self.owner),
                            process_id=self.managed_health["process_id"], handle_signaled=gone,
                            native_stop=self.managed_stop,
                            source_files=[p.relative_to(self.owner/"source").as_posix() for p in (self.owner/"source").rglob("*") if p.is_file()],
                            primary_cleanup_error=failure(primary) if primary else None, signals_sent=0)
                    except BaseException as observer:
                        capture.errors.append(failure(observer))
                    finally:
                        try:
                            held.close()
                        except BaseException as observer:
                            capture.errors.append(failure(observer))
                        self._held = None

    def observed_cold(fixture, admission, *, case_deadline=None):
        fixture._cold_admission = admission
        capture.active_admission = admission
        capture.emit("cold_case", admission=admission, fixture=str(fixture.owner),
                     original_case_deadline_monotonic=case_deadline)
        return original_cold(fixture, admission, case_deadline=case_deadline)

    def observed_read(read, deadline, *args, **kwargs):
        capture.emit("original_read_clock", **getattr(capture.local, "label", {}),
                     original_deadline_monotonic=deadline)
        return original_read(read, deadline, *args, **kwargs)

    clients.Fixture = ObservedFixture
    clients.cold_first_status = observed_cold
    clients.read_envelope = observed_read
    clients.ThreadPoolExecutor = tracked_executor(capture)
    try:
        yield
    finally:
        clients.Fixture, clients.cold_first_status = original_fixture, original_cold
        clients.read_envelope, clients.ThreadPoolExecutor = original_read, original_executor
