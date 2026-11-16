# SPDX-License-Identifier: Apache-2.0
"""A bounded actual HTTPS MCP test client, never a deployed proxy or config writer.

U110 (split-plan §2.1, contracts §2.11): the service's loopback HTTPS endpoint
takes no credential. This client connects with the native endpoint record and
the installation's AIRK CA only, keeps strict TLS and peer pinning, and never
sends Authorization. Every session is the service's built-in local agent
session, so read tools name their scope per call (index_ids, index_set_ids or
workspace); McpClient can attach a fixed per-call scope for that.
"""
import base64
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import ssl
import stat
import time
import uuid

# Only transport IDs and observed timing vary between equivalent native reads.
# Delivery result_digest binds those timing fields; callers retain complete raw
# proofs and validate native delivery before using this comparison copy.
TIMING_FIELDS = {"request_id", "observed_unix_ms", "observed_from_unix_ms", "observed_to_unix_ms", "cache_age_ms", "cached", "elapsed_ms"}


def parity_value(value):
    if isinstance(value, list):
        return [parity_value(item) for item in value]
    if not isinstance(value, dict):
        return value
    result = {key: parity_value(item) for key, item in value.items() if key not in TIMING_FIELDS}
    if isinstance(result.get("allocation"), dict):
        result["allocation"]["deadline_ms"] = 0
    if isinstance(result.get("delivery_observation"), dict):
        result["delivery_observation"]["receipt"].pop("result_digest", None)
    return result


def parity_envelope(envelope):
    """Keep all result fields; normalize JSON text blocks as their native body.

    Callers retain the original wire reply before using this comparison copy.
    Non-JSON text, nontext blocks and future fields remain exact.
    """
    value = json.loads(json.dumps(envelope))
    for block in value.get("content", []):
        if isinstance(block, dict) and block.get("type") == "text" and isinstance(block.get("text"), str):
            try:
                block["text"] = json.loads(block["text"])
            except ValueError:
                pass
    return parity_value(value)


def endpoint_of(record):
    """The endpoint descriptor of a tokenless endpoint record, or of a legacy
    (pre-U110) connection record that embeds one. No credential is read."""
    if isinstance(record, dict) and isinstance(record.get("endpoint"), dict):
        return record["endpoint"]
    if not isinstance(record, dict):
        raise ValueError("HTTP fixture endpoint record is not an object")
    return record


ENDPOINT_URL = r"https://127\.0\.0\.1:([1-9][0-9]{0,4})/mcp"
MAX_ENDPOINT_BYTES = 65_536


def checked_endpoint(record):
    endpoint = endpoint_of(record)
    matched = re.fullmatch(ENDPOINT_URL, endpoint.get("url") or "")
    if (endpoint.get("schema") != 1 or not matched or not 0 < int(matched[1]) <= 65535
            or endpoint.get("state") != "ready"):
        raise ValueError("HTTP fixture endpoint identity is invalid or unavailable")
    return endpoint, int(matched[1])


def read_endpoint(control):
    """Discovery without a connection: the owner-private `<control>/http-1/endpoint.json`."""
    private = Path(control) / "http-1"
    endpoint = json.loads(private_bytes(private / "endpoint.json", control, MAX_ENDPOINT_BYTES))
    checked_endpoint(endpoint)
    return endpoint


def validate_native_identity(connection, health):
    endpoint = endpoint_of(connection)
    if (endpoint["service_instance"] != health["instance"] or endpoint["user"] != health["user"]
            or endpoint["executable_sha256"] != health["executable_sha256"]
            or endpoint["namespace"] != health["catalog_clock"]["namespace"]):
        raise ValueError("HTTP fixture endpoint differs from the authenticated native service")


def validate_enrollment(connection, control, selection, connection_id, *, reference_resolver=None):
    """Legacy (pre-U110) enrollment metadata check, kept for callers that have
    not moved to read_endpoint. McpClient never reads the credential it names."""
    if connection.get("schema") != 1 or connection.get("connection_id") != connection_id:
        raise ValueError("native HTTPS enrollment identity changed")
    def same(wanted, actual):
        if isinstance(wanted, dict):
            return isinstance(actual, dict) and wanted.keys() == actual.keys() and all(
                (type(wanted[key]) is int and type(actual[key]) is int and actual[key] >= wanted[key])
                if key == "expected_catalog_revision" else same(wanted[key], actual[key]) for key in wanted)
        if isinstance(wanted, list):
            return isinstance(actual, list) and len(wanted) == len(actual) and all(same(a, b) for a, b in zip(wanted, actual))
        return type(wanted) is type(actual) and wanted == actual
    if not same(selection, connection.get("selection")):
        raise ValueError("native HTTPS enrollment changed immutable selection")
    resolve = reference_resolver or (lambda path: Path(path).resolve(strict=True))
    private = resolve(control) / "http-1"
    for field, name in (("credential_file", f"credential-{connection_id}.token"), ("ca_certificate_file", "ca.pem")):
        if reference_resolver is None:
            actual = contained_private_path(connection[field], control)
            matches = actual.samefile(private / name)
        else:
            # Remote adapters separately verify filesystem identity in their
            # container; a host path cannot authenticate that remote namespace.
            matches = resolve(connection[field]) == resolve(private / name)
        if not matches:
            raise ValueError("native HTTPS enrollment reference changed")
    return connection


def enroll_native(run, install, control, selection, connection_id, client="unknown", *, deadline=None):
    """Legacy (pre-U110): submit once via the installer. run follows Fixture.run's argument API.
    Since U110 no read needs this; use read_endpoint and a per-call scope.

    Returns bare HttpConnectionInfo, not the private control response envelope.
    The native reply has no client field; attribution is the exact supplied CLI
    label and must be checked through usage observations by attribution tests.
    """
    labels = {"unknown": "unknown", "codex": "codex", "claude_code": "claude-code",
              "vscode": "vscode", "copilot_cli": "copilot-cli", "cli": "cli",
              "explorer": "airk-explorer", "workbench": "airk-workbench"}
    if client not in labels or not re.fullmatch(r"[0-9a-f]{32}", connection_id):
        raise ValueError("invalid native enrollment client or connection ID")
    until = time.monotonic() + 60 if deadline is None else deadline
    private = Path(control).resolve(strict=True) / "http-1"
    metadata = private.lstat()
    if (not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode)
            or getattr(metadata, "st_file_attributes", 0) & 0x400
            or os.name != "nt" and (metadata.st_uid != os.geteuid() or metadata.st_mode & 0o077)):
        raise ValueError("native enrollment parent is not the owned private HTTP directory")
    payload = json.dumps(selection, separators=(",", ":")).encode("utf-8")
    if len(payload) > 49152:
        raise ValueError("native enrollment selection exceeds protected input bound")
    path = private / ("fixture-selection-" + uuid.uuid4().hex + ".json")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(payload)
        private_bytes(path, control, 49152)
        remaining = until - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("native enrollment deadline exhausted before submission")
        result = run("runtime", "http-connection", "--install-dir", str(install),
                     "--selection-file", str(path), "--connection-id", connection_id,
                     "--client", labels[client], timeout=remaining)
        if not isinstance(result, dict):
            encoded = result.stdout.encode("utf-8") if isinstance(result.stdout, str) else result.stdout
            if result.returncode or len(encoded) > 61440:
                raise ValueError("native HTTPS enrollment refused or exceeded metadata bound")
            result = json.loads(encoded)
        elif len(json.dumps(result, separators=(",", ":")).encode("utf-8")) > 61440:
            raise ValueError("native HTTPS enrollment exceeded metadata bound")
        if time.monotonic() >= until:
            raise TimeoutError("native enrollment deadline exhausted after submission; outcome unknown")
        return validate_enrollment(result, control, selection, connection_id)
    finally:
        path.unlink()


def contained_private_path(reference, root):
    """Prove ancestry by existing filesystem identities, including Win32 paths.

    Python preserves the native verbatim prefix in resolve(), so textual path
    equality/containment is insufficient. No spelling is stripped or trusted.
    Every traversed component must remain free of symlinks/reparse points.
    """
    path = Path(reference)
    root = Path(root).resolve(strict=True)
    if not path.is_absolute():
        raise ValueError("HTTP fixture reference escaped its owned control root")
    for component in (path, *path.parents):
        metadata = component.lstat()
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("HTTP fixture reference contains a redirected component")
        if component.samefile(root):
            return path
    raise ValueError("HTTP fixture reference escaped its owned control root")


def private_bytes(reference, root, limit):
    path = contained_private_path(reference, root)
    metadata = path.stat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1 or metadata.st_size > limit
            or os.name != "nt" and (metadata.st_uid != os.geteuid() or metadata.st_mode & 0o077)):
        raise ValueError("HTTP fixture reference is not a bounded private regular file")
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("HTTP fixture reference grew beyond its bound")
    return data


MAX_RESPONSE_BYTES = 1_048_576
MAX_RESPONSE_MESSAGES = 128


def response_messages(data, content_type):
    """Decode bounded JSON or a finite Streamable HTTP SSE response verbatim."""
    if len(data) > MAX_RESPONSE_BYTES:
        raise ValueError("HTTP fixture response exceeds native output bound")
    if not data:
        return []
    mime = (content_type or "").split(";", 1)[0].strip().lower()
    if mime == "application/json":
        return [json.loads(data)]
    if mime != "text/event-stream":
        raise ValueError("HTTP fixture response Content-Type is unsupported")
    text = data.decode("utf-8-sig").replace("\r\n", "\n").replace("\r", "\n")
    messages, fields = [], []
    for line in text.split("\n"):
        if not line:
            if fields:
                if len(messages) >= MAX_RESPONSE_MESSAGES:
                    raise ValueError("HTTP fixture SSE message bound exceeded")
                messages.append(json.loads("\n".join(fields)))
                fields.clear()
        elif not line.startswith(":"):
            field, _, value = line.partition(":")
            if field == "data":
                fields.append(value[1:] if value.startswith(" ") else value)
    if fields:
        raise ValueError("HTTP fixture SSE ended within a message")
    return messages


class HttpResponseError(ValueError):
    def __init__(self, status, body, *, headers=None, request=None, raw_body=None,
                 header_redactions=None, body_redacted=False, failure="http_response",
                 body_truncated=False, submission="observed"):
        super().__init__("HTTP fixture " + failure + " with status " + str(status))
        self.status, self.body = status, body
        self.headers = headers
        self.request = request
        self.raw_body = raw_body
        self.header_redactions = header_redactions or []
        self.body_redacted = body_redacted
        self.failure, self.body_truncated, self.submission = failure, body_truncated, submission

    def evidence(self):
        """Complete bounded response metadata, never bearer/session material."""
        return {"status": self.status, "body": self.body, "headers": self.headers,
                "failure": self.failure, "body_truncated": self.body_truncated, "submission": self.submission,
                "request": self.request, "header_redactions": self.header_redactions,
                "body_redacted": self.body_redacted,
                "raw_body_base64": base64.b64encode(self.raw_body).decode("ascii") if self.raw_body is not None else None,
                "raw_body_sha256": hashlib.sha256(self.raw_body).hexdigest() if self.raw_body is not None else None,
                "error_code": self.body.get("error", {}).get("code")
                    if isinstance(self.body, dict) and isinstance(self.body.get("error"), dict) else None}


class HttpRefused(HttpResponseError):
    pass


REFUSAL_STAGES = {(429, "query_admission_busy"): "query",
                  (503, "authentication_busy"): "authentication",
                  (503, "native_validation_busy"): "native_validation"}


def classify_refusal(error):
    """Only documented complete pre-admission responses prove non-submission."""
    if not isinstance(error, HttpRefused) or error.body_truncated:
        return None
    body = error.body
    if not isinstance(body, dict) or body.get("schema") != "airk.local-api-error/1" or not isinstance(body.get("error"), dict):
        return None
    code = body["error"].get("code")
    stage = REFUSAL_STAGES.get((error.status, code)) if isinstance(code, str) else None
    if stage is None or any("query_submitted" in item and item["query_submitted"] is not False for item in (body, body["error"])):
        return None
    return {"code": code, "stage": stage, "status": error.status, "query_submitted": False}


def error_outcome(error):
    """Do not infer non-submission from unknown codes, fences or disconnections."""
    if classify_refusal(error) is not None:
        return "refused"
    if isinstance(error, HttpResponseError):
        return "uncertain" if error.submission == "uncertain" else "failed"
    return "uncertain" if isinstance(error, (OSError, TimeoutError)) else "failed"


def phase_counts(outcomes, planned):
    """Every planned exchange has one outcome; no success-count threshold."""
    if len(outcomes) > planned or planned < 0:
        raise ValueError("phase outcome count exceeds planned exchanges")
    counts = {key: 0 for key in ("succeeded", "refused", "failed", "uncertain")}
    codes = {}
    for outcome in outcomes:
        kind = outcome["outcome"]
        if kind == "refused" and outcome.get("code") not in {code for _, code in REFUSAL_STAGES}:
            raise ValueError("phase refusal code is not documented")
        counts[kind] += 1
        if kind == "refused":
            code = outcome["code"]
            codes[code] = codes.get(code, 0) + 1
    return {**counts, "refused_by_code": codes, "not_executed": planned-len(outcomes),
            "planned": planned, "passed": not counts["failed"] and not counts["uncertain"] and len(outcomes) == planned}


def request_protocol(value, initialized, negotiated):
    # Initial negotiation belongs to the untouched initialize body. The SDK
    # permits omitting this header, and rejects a mismatching initial header.
    if not initialized and isinstance(value, dict) and value.get("method") == "initialize":
        return None
    params = value.get("params") if isinstance(value, dict) else None
    meta = params.get("_meta") if isinstance(params, dict) else None
    protocol = meta.get("io.modelcontextprotocol/protocolVersion", negotiated) if isinstance(meta, dict) else negotiated
    if not isinstance(protocol, str) or not re.fullmatch(r"[!-~]{1,64}", protocol):
        raise ValueError("HTTP fixture protocol header is invalid")
    return protocol


def standard_headers(value, protocol):
    """SEP-2243 routing headers derived from the unchanged JSON request."""
    if protocol is None or protocol < "2026-07-28" or not isinstance(value, dict):
        return {}
    method = value.get("method")
    if not isinstance(method, str):
        return {}
    headers = {"Mcp-Method": method}
    key = {"tools/call": "name", "prompts/get": "name", "resources/read": "uri",
           "resources/subscribe": "uri", "resources/unsubscribe": "uri",
           "tasks/get": "taskId", "tasks/update": "taskId", "tasks/cancel": "taskId"}.get(method)
    params = value.get("params")
    name = params.get(key) if key and isinstance(params, dict) else None
    if isinstance(name, str):
        if (not name.isascii() or any(ord(char) < 32 or ord(char) > 126 for char in name)
                or name[:1] in (" ", "\t") or name[-1:] in (" ", "\t")
                or (name.startswith("=?base64?") and name.endswith("?="))):
            name = "=?base64?" + base64.b64encode(name.encode("utf-8")).decode("ascii") + "?="
        headers["Mcp-Name"] = name
    return headers


# U110: tools that take no index scope. help reads no index, list_scopes
# refuses index selectors, and the assistance/template tools are not native
# evidence reads. Every other tool call is an evidence read that names a scope.
UNSCOPED_TOOLS = frozenset({
    "help", "list_scopes", "list_templates", "get_template", "validate_draft", "proposal_status",
    "get_handoff", "draft_search", "draft_strategy", "draft_plan", "suggest_view"})
SCOPE_KEYS = ("index_ids", "index_set_ids", "workspace")


def scope_value(index_ids=None, index_set_ids=None, workspace=None):
    """A per-call U110 scope: named indexes/index sets (ids or names) or one absolute workspace."""
    scope = {}
    for key, values in (("index_ids", index_ids), ("index_set_ids", index_set_ids)):
        if values:
            if (not isinstance(values, (list, tuple)) or len(values) > 32
                    or not all(isinstance(item, str) and 0 < len(item) <= 256 for item in values)):
                raise ValueError("HTTP fixture scope " + key + " is invalid")
            scope[key] = list(values)
    if workspace is not None:
        if not isinstance(workspace, str) or not os.path.isabs(workspace):
            raise ValueError("HTTP fixture scope workspace must be an absolute folder")
        scope["workspace"] = workspace
    return scope


def scoped_arguments(tool, arguments, scope):
    """Add scope to a read's arguments unless the call already names one or takes none."""
    if not scope or not isinstance(arguments, dict) or not isinstance(tool, str):
        return arguments
    if tool.removeprefix("airk_") in UNSCOPED_TOOLS or any(key in arguments for key in SCOPE_KEYS):
        return arguments
    return {**arguments, **json.loads(json.dumps(scope))}


class McpClient:
    def response_error(self, status, data, response_headers, method, value, path, *, failure="http_response", truncated=False):
        """Bound and redact error evidence before it can enter any receipt.

        U110: no credential is held or sent; MCP session ids stay redacted."""
        session_values = [self.session] if self.session else []
        session_values.extend(item for name, item in response_headers if name.lower() == "mcp-session-id")
        secrets = [(item, "[REDACTED_FIXTURE_SESSION]") for item in session_values if item]
        secrets.sort(key=lambda pair: len(pair[0]), reverse=True)
        def redact(item):
            for secret, replacement in secrets:
                if secret:
                    item = item.replace(secret, replacement)
            return item
        protected = {"authorization", "proxy-authorization", "set-cookie", "cookie", "mcp-session-id"}
        redactions = [name for name, item in response_headers if name.lower() in protected or redact(item) != item]
        headers = [[name, "[REDACTED]" if name.lower() in protected else redact(item)] for name, item in response_headers]
        # Redact before bounding the prefix so a secret cut by the prefix cannot leak.
        redacted_data = data
        for secret, replacement in secrets:
            if secret:
                redacted_data = redacted_data.replace(secret.encode(), replacement.encode())
        body_redacted = redacted_data != data
        if truncated:
            # read(limit+1) can end inside a secret. Its observed prefix must
            # also be removed, including the byte beyond the retained limit.
            for secret, _ in secrets:
                encoded_secret = secret.encode()
                for length in range(min(len(encoded_secret)-1, len(redacted_data)), 0, -1):
                    if redacted_data.endswith(encoded_secret[:length]):
                        redacted_data = redacted_data[:-length] + b"[REDACTED_PARTIAL_FIXTURE_SECRET]"
                        body_redacted = True
                        break
        redacted_data = redacted_data[:MAX_RESPONSE_BYTES]
        try:
            body = json.loads(redacted_data) if redacted_data else None
        except ValueError:
            body = redacted_data.decode("utf-8", errors="replace")
        encoded = None if value is None else json.dumps(value, separators=(",", ":")).encode()
        request = {"http_method": method, "path": path,
                   "session_sha256": hashlib.sha256(self.session.encode()).hexdigest() if self.session is not None else None,
                   "jsonrpc_method": value.get("method") if isinstance(value, dict) else None,
                   "jsonrpc_id": value.get("id") if isinstance(value, dict) else None,
                   "request_sha256": hashlib.sha256(encoded).hexdigest() if encoded is not None else None}
        error_type = HttpRefused if failure == "http_response" else HttpResponseError
        return error_type(status, body, headers=headers, request=request, raw_body=redacted_data,
                          header_redactions=redactions, body_redacted=body_redacted,
                          failure=failure, body_truncated=truncated,
                          submission="uncertain" if truncated else "observed")

    def __init__(self, endpoint, control_root, timeout=125, *, scope=None):
        """endpoint: the tokenless endpoint record (`<control>/http-1/endpoint.json`,
        `airkctl agents endpoint`), or a legacy connection record embedding one.
        The CA is the record's ca_certificate_file when present, else
        `<control>/http-1/ca.pem`; either must be an owner-private file under
        control_root. No credential file is read. scope: optional per-call
        scope (scope_value keywords) attached to evidence reads that name none;
        a legacy scoped connection record defaults it to its enrolled index.
        The 125-second transport default permits the 120-second operation plus
        five seconds of transport overhead; it grants no new native work time.
        An explicit shorter timeout stays the caller's original deadline."""
        record = endpoint
        endpoint, port = checked_endpoint(record)
        legacy = record is not endpoint
        selection = record.get("selection") if legacy else None
        if (isinstance(selection, dict) and isinstance(selection.get("selection"), dict)
                and selection["selection"].get("namespace") != endpoint.get("namespace")):
            raise ValueError("HTTP fixture endpoint identity is invalid or unavailable")
        reference = record.get("ca_certificate_file") if legacy else None
        ca_file = reference if reference is not None else Path(control_root) / "http-1" / "ca.pem"
        ca = private_bytes(ca_file, control_root, 8192).decode("ascii")
        if ca.count("-----BEGIN CERTIFICATE-----") != 1 or ca.count("-----END CERTIFICATE-----") != 1:
            raise ValueError("HTTP fixture needs exactly one reviewed issuer")
        if hashlib.sha256(ssl.PEM_cert_to_DER_cert(ca)).hexdigest() != endpoint["ca_sha256"]:
            raise ValueError("HTTP fixture issuer differs from native control")
        if scope is None and isinstance(selection, dict) and selection.get("kind") == "scoped" \
                and isinstance(selection["selection"].get("index_id"), str):
            # Bridge for pre-U110 callers that still pass an enrolled
            # single-index connection: read the index it was enrolled for.
            scope = {"index_ids": [selection["selection"]["index_id"]]}
        self.scope = scope_value(**scope) if scope else {}
        context = ssl.create_default_context()
        # Exercise the strict checks even on Python releases where they are not the default.
        context.verify_flags |= ssl.VERIFY_X509_STRICT
        context.load_verify_locations(cadata=ca)  # Add the local issuer; preserve normal roots/hostname checks.
        self.connection = http.client.HTTPSConnection("127.0.0.1", port, context=context, timeout=timeout)
        self._certificate_sha256 = endpoint["certificate_sha256"]
        self.connection.connect()  # Verify TLS and the native leaf before sending any request bytes.
        if hashlib.sha256(self.connection.sock.getpeercert(binary_form=True)).hexdigest() != endpoint["certificate_sha256"]:
            self.connection.close()
            raise ValueError("HTTP fixture peer differs from native control")
        self.session = None
        self.initialized = False
        self.protocol = "2025-11-25"
        self.pending = {}
        self.unsolicited = []

    def _request(self, method, value=None, path="/mcp"):
        encoded = None if value is None else json.dumps(value, separators=(",", ":")).encode()
        if encoded is not None and len(encoded) > 65_536:
            raise ValueError("HTTP fixture request exceeds native frame bound")
        # U110: never Authorization; the gateway would strip it anyway.
        headers = {"Accept": "application/json, text/event-stream",
                   "Content-Type": "application/json",
                   "X-AIRK-Response-Mode": "full"}
        protocol = request_protocol(value, self.initialized, self.protocol)
        if path == "/mcp":
            headers.update(standard_headers(value, protocol))
        if protocol is not None:
            headers["MCP-Protocol-Version"] = protocol
        if self.session is not None and path == "/mcp":
            headers["Mcp-Session-Id"] = self.session
        if self.connection.sock is None:
            self.connection.connect()
        if hashlib.sha256(self.connection.sock.getpeercert(binary_form=True)).hexdigest() != self._certificate_sha256:
            self.connection.close()
            raise ValueError("HTTP fixture peer changed before the request")
        self.connection.request(method, path, encoded, headers)
        response = self.connection.getresponse()
        data = response.read(MAX_RESPONSE_BYTES + 1)
        response_headers = response.getheaders()
        if len(data) > MAX_RESPONSE_BYTES:
            # Consume no more bytes; the connection is not safe for another exchange.
            self.connection.close()
            raise self.response_error(response.status, data, response_headers, method, value, path,
                                      failure="response_limit", truncated=True)
        if not 200 <= response.status < 300:
            if len(response_headers) > 100 or sum(len(name) + len(item) for name, item in response_headers) > 65_536:
                raise ValueError("HTTP fixture refusal headers exceed observation bound")
            raise self.response_error(response.status, data, response_headers, method, value, path)
        session = response.getheader("Mcp-Session-Id")
        if session is not None:
            if path != "/mcp":
                raise ValueError("HTTP fixture JSON API unexpectedly created an MCP session")
            if self.session not in (None, session) or not re.fullmatch(r"[A-Za-z0-9_-]{1,256}", session):
                raise ValueError("HTTP fixture session identity changed")
        try:
            messages = response_messages(data, response.getheader("Content-Type"))
        except (ValueError, UnicodeError) as error:
            raise ValueError("HTTP fixture response decode failed: "
                             f"{method} {path}, status={response.status}, "
                             f"content_type={response.getheader('Content-Type')!r}, "
                             f"bytes={len(data)}, sha256={hashlib.sha256(data).hexdigest()}") from error
        if path == "/mcp" and isinstance(value, dict) and "id" in value:
            matching = []
            for message in messages:
                if (isinstance(message, dict) and "id" in message
                        and type(message["id"]) is type(value["id"]) and message["id"] == value["id"]):
                    matching.append(message)
                else:
                    if len(self.unsolicited) >= MAX_RESPONSE_MESSAGES:
                        raise ValueError("HTTP fixture unsolicited message bound exceeded")
                    self.unsolicited.append(message)
            if len(matching) > 1:
                raise ValueError("HTTP fixture received duplicate matching JSON-RPC replies")
            if matching and "error" in matching[0]:
                raise self.response_error(response.status, data, response_headers, method, value, path,
                                          failure="jsonrpc_error")
            if session is not None:
                self.session = session
            return matching[0] if matching else None
        if len(messages) > 1:
            raise ValueError("HTTP fixture single-result endpoint returned multiple messages")
        if session is not None:
            self.session = session
        return messages[0] if messages else None

    def api_catalog(self):
        return self._request("GET", path="/api/v1/tools")

    def api_read(self, tool, arguments, *, scoped=True):
        """scoped=False sends the arguments unchanged (for example, to observe scope_required)."""
        if not re.fullmatch(r"[A-Za-z0-9_]{1,64}", tool):
            raise ValueError("HTTP fixture API tool name is invalid")
        if scoped:
            arguments = scoped_arguments(tool, arguments, self.scope)
        return self._request("POST", arguments, "/api/v1/tools/" + tool)

    def with_scope(self, value):
        """The JSON-RPC request with this client's scope on a tools/call that names none."""
        params = value.get("params") if isinstance(value, dict) and value.get("method") == "tools/call" else None
        if not self.scope or not isinstance(params, dict) or not isinstance(params.get("arguments", {}), dict):
            return value
        arguments = params.get("arguments", {})
        scoped = scoped_arguments(params.get("name"), arguments, self.scope)
        if scoped is arguments:
            return value
        return {**value, "params": {**params, "arguments": scoped}}

    def send(self, value, *, scoped=True):
        """scoped=False sends the request unchanged (for example, to observe scope_required)."""
        response = self._request("POST", self.with_scope(value) if scoped else value)
        if "id" in value:
            if not isinstance(response, dict) or response.get("id") != value["id"] or "error" in response:
                raise ValueError("HTTP fixture received a mismatched or failed JSON-RPC reply")
            self.pending[value["id"]] = response
            if value.get("method") == "initialize":
                protocol = response.get("result", {}).get("protocolVersion")
                if not isinstance(protocol, str) or not re.fullmatch(r"[!-~]{1,64}", protocol):
                    raise ValueError("HTTP fixture negotiated protocol is invalid")
                self.protocol = protocol
                self.initialized = True
        elif response is not None:
            raise ValueError("HTTP fixture notification unexpectedly returned a body")

    def receive(self, identifier, _deadline=None):
        if identifier not in self.pending:
            raise ValueError("HTTP fixture matching reply is missing")
        return self.pending.pop(identifier)

    def close(self, delete=True):
        try:
            if delete and self.session is not None:
                self._request("DELETE")
        finally:
            self.connection.close()
