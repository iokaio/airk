# SPDX-License-Identifier: Apache-2.0
"""A bounded actual HTTPS MCP test client, never a deployed proxy or config writer."""
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


def validate_native_identity(connection, health):
    endpoint = connection["endpoint"]
    if (endpoint["service_instance"] != health["instance"] or endpoint["user"] != health["user"]
            or endpoint["executable_sha256"] != health["executable_sha256"]
            or endpoint["namespace"] != health["catalog_clock"]["namespace"]):
        raise ValueError("HTTP fixture endpoint differs from the authenticated native service")


def validate_enrollment(connection, control, selection, connection_id, *, reference_resolver=None):
    """Check bare native CLI metadata without discarding immutable authority."""
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
    """Submit once via the installer. run follows Fixture.run's argument API.

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


class HttpRefused(ValueError):
    def __init__(self, status, body):
        super().__init__("HTTP fixture request refused with status " + str(status))
        self.status, self.body = status, body


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


class McpClient:
    def __init__(self, connection, control_root, timeout=60):
        endpoint = connection["endpoint"]
        matched = re.fullmatch(r"https://127\.0\.0\.1:([1-9][0-9]{0,4})/mcp", endpoint["url"])
        if (connection.get("schema") != 1 or endpoint.get("schema") != 1 or not matched
                or not 0 < int(matched[1]) <= 65535 or endpoint.get("state") != "ready"
                or connection["selection"]["selection"]["namespace"] != endpoint["namespace"]):
            raise ValueError("HTTP fixture endpoint identity is invalid or unavailable")
        ca = private_bytes(connection["ca_certificate_file"], control_root, 8192).decode("ascii")
        if ca.count("-----BEGIN CERTIFICATE-----") != 1 or ca.count("-----END CERTIFICATE-----") != 1:
            raise ValueError("HTTP fixture needs exactly one reviewed issuer")
        if hashlib.sha256(ssl.PEM_cert_to_DER_cert(ca)).hexdigest() != endpoint["ca_sha256"]:
            raise ValueError("HTTP fixture issuer differs from native control")
        token = private_bytes(connection["credential_file"], control_root, 103).decode("ascii")
        if not re.fullmatch(r"airk1\." + re.escape(connection["connection_id"]) + r"\.[0-9a-f]{64}", token):
            raise ValueError("HTTP fixture credential identity is invalid")
        context = ssl.create_default_context()
        # Exercise the strict checks even on Python releases where they are not the default.
        context.verify_flags |= ssl.VERIFY_X509_STRICT
        context.load_verify_locations(cadata=ca)  # Add the local issuer; preserve normal roots/hostname checks.
        self.connection = http.client.HTTPSConnection("127.0.0.1", int(matched[1]), context=context, timeout=timeout)
        self._certificate_sha256 = endpoint["certificate_sha256"]
        self.connection.connect()  # Verify TLS and the native leaf before sending any bearer bytes.
        if hashlib.sha256(self.connection.sock.getpeercert(binary_form=True)).hexdigest() != endpoint["certificate_sha256"]:
            self.connection.close()
            raise ValueError("HTTP fixture peer differs from native control")
        self._token = token
        self.session = None
        self.initialized = False
        self.protocol = "2025-11-25"
        self.pending = {}
        self.unsolicited = []

    def _request(self, method, value=None, path="/mcp"):
        encoded = None if value is None else json.dumps(value, separators=(",", ":")).encode()
        if encoded is not None and len(encoded) > 65_536:
            raise ValueError("HTTP fixture request exceeds native frame bound")
        headers = {"Authorization": "Bearer " + self._token, "Accept": "application/json, text/event-stream",
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
            raise ValueError("HTTP fixture peer changed before authentication")
        self.connection.request(method, path, encoded, headers)
        response = self.connection.getresponse()
        data = response.read(1_048_577)
        if len(data) > 1_048_576:
            raise ValueError("HTTP fixture response exceeds native output bound")
        if not 200 <= response.status < 300:
            try:
                body = json.loads(data) if data else None
            except ValueError:
                body = data.decode("utf-8", errors="replace")
            raise HttpRefused(response.status, body)
        session = response.getheader("Mcp-Session-Id")
        if session is not None:
            if path != "/mcp":
                raise ValueError("HTTP fixture JSON API unexpectedly created an MCP session")
            if self.session not in (None, session) or not re.fullmatch(r"[A-Za-z0-9_-]{1,256}", session):
                raise ValueError("HTTP fixture session identity changed")
            self.session = session
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
            return matching[0] if matching else None
        if len(messages) > 1:
            raise ValueError("HTTP fixture single-result endpoint returned multiple messages")
        return messages[0] if messages else None

    def api_catalog(self):
        return self._request("GET", path="/api/v1/tools")

    def api_read(self, tool, arguments):
        if not re.fullmatch(r"[A-Za-z0-9_]{1,64}", tool):
            raise ValueError("HTTP fixture API tool name is invalid")
        return self._request("POST", arguments, "/api/v1/tools/" + tool)

    def send(self, value):
        response = self._request("POST", value)
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
            self._token = ""
