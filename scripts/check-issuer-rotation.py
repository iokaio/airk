# SPDX-License-Identifier: Apache-2.0
"""Qualify installed issuer rotation with private client trust, never OS trust mutation."""
import argparse
import hashlib
import http.client
import importlib.util
import json
from pathlib import Path
import socket
import ssl
import time
import uuid


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


clients = load("check-service-clients")
transport = load("http_fixture")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def same_scope(before, after):
    a, b = json.loads(json.dumps(before)), json.loads(json.dumps(after))
    assert a["kind"] == b["kind"] == "scoped"
    assert b["selection"].pop("expected_catalog_revision") >= a["selection"].pop("expected_catalog_revision")
    assert a == b, "Issuer replacement changed reviewed authority"


def additive_context(*certificates):
    context = ssl.create_default_context()
    context.verify_flags |= ssl.VERIFY_X509_STRICT
    original = {digest(cert) for cert in context.get_ca_certs(binary_form=True)}
    for certificate in certificates:
        context.load_verify_locations(cadata=certificate.decode("ascii"))
    assert context.check_hostname and context.verify_mode == ssl.CERT_REQUIRED
    assert original <= {digest(cert) for cert in context.get_ca_certs(binary_form=True)}, "Existing trust roots were lost"
    return context


def close_overlap_before_wait(network, expires):
    """Acknowledge DELETE while live; an idle TCP connection is not a session lease."""
    assert type(expires) is int and expires > 0, "Invalid native overlap deadline"
    network.close()  # A refused/unknown DELETE remains a campaign failure.
    wait_limit = time.monotonic() + 70
    while int(time.time() * 1000) < expires:
        assert time.monotonic() < wait_limit, "Issuer overlap clock failed to reach bounded deadline"
        time.sleep(min(1, max(0.01, (expires - time.time() * 1000) / 1000)))


def rotation(fixture, action, receipt=None, **flags):
    assert action in ("status", "prepare", "commit", "finalize", "abort"), "OS trust mutation is outside this campaign"
    args = ["runtime", "http-issuer-rotation", "--install-dir", fixture.install, "--action", action]
    if receipt is not None:
        args += ["--operation-id", receipt["rotation"]["operation_id"], "--expected-receipt-sha256", receipt["receipt_sha256"]]
    for key, value in flags.items():
        args += ["--" + key.replace("_", "-"), str(value)]
    value = json.loads(fixture.run(*args).stdout)
    assert value["schema"] == 1
    return value


def prepare(fixture):
    status = rotation(fixture, "status")
    return rotation(fixture, "prepare", operation_id=uuid.uuid4().hex,
                    expected_ca_sha256=status["current_endpoint"]["ca_sha256"],
                    expected_descriptor_sha256=status["current_descriptor_sha256"], overlap_seconds=60)


def rpc(network, identifier, method, params):
    network.send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
    reply = network.receive(identifier)
    assert "error" not in reply, reply
    return reply["result"]


def handshake(network):
    rpc(network, 1, "initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "airk-owned-issuer-check", "version": "1"}})
    network.send({"jsonrpc": "2.0", "method": "notifications/initialized"})


def search(network):
    deadline = time.monotonic() + 60
    identifier = 10
    while True:
        reply = rpc(network, identifier, "tools/call", {"name": "airk_search", "arguments": {
            "query": "cobalt", "mode": "lexical", "response_mode": "full"}})
        delay = clients.read_retry_delay(reply)
        if delay is None:
            assert not reply.get("isError"), reply
            body = clients.tool_value(reply)
            assert body["hits"][0]["citation"]["path"] == "fixture.txt"
            return body
        assert time.monotonic() + delay < deadline, "Explicit native read retry budget exhausted"
        time.sleep(delay)
        identifier += 1


def stale_issuer_refused(endpoint, old_ca):
    port = int(endpoint["url"].split(":")[-1].split("/")[0])
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=10) as raw:
            with additive_context(old_ca).wrap_socket(raw, server_hostname="127.0.0.1"):
                raise AssertionError("Old-only issuer unexpectedly validated the replacement")
    except ssl.SSLCertVerificationError:
        return True  # No Authorization header or HTTP request was sent.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="Fresh owned evidence directory; retained even on failure")
    args = parser.parse_args()
    owner = args.output.absolute()
    owner.mkdir(parents=True, exist_ok=False)
    report = {"schema": 1, "passed": False, "os_user_trust_mutated": False,
        "not_exercised": ["native trust-new OS enrollment", "owned OS root removal", "actual external Codex/Claude/VS Code host reconnect"],
        "receipts": {}, "source": str(Path(__file__).resolve()), "source_sha256": digest(Path(__file__).read_bytes())}
    fixture = clients.Fixture(args.runtime_dir, owner, command_timeout=300, job_timeout=180)
    original_network = replacement = None
    primary = None
    try:
        assert fixture.single_image, "This acceptance campaign requires an approved schema-2 runtime"
        fixture.start()
        original_health = fixture.managed_health.copy()
        root, data = owner / "sources", owner / "data"
        root.mkdir(); data.mkdir()
        source = root / "fixture.txt"
        source.write_text("cobalt harbor original source remains unchanged\n", encoding="utf-8")
        source_sha = digest(source.read_bytes())
        fixture.register("issuer-fixture", root, data)
        fixture.configure("issuer-fixture", {"operation": "vectors", "enabled": False})
        fixture.job("issuer-fixture", "index")
        publication = fixture.published_status("issuer-fixture")["publication"]
        connection_id = uuid.uuid4().hex
        before = fixture.enroll_http({"kind": "scoped", "selection": fixture.scopes["issuer-fixture"]}, connection_id)
        transport.validate_native_identity(before, fixture.managed_health)
        original_network = transport.McpClient(before, fixture.control)
        handshake(original_network)
        report["before_read"] = search(original_network)
        credential_sha = digest(transport.private_bytes(before["credential_file"], fixture.control, 103))
        assert not (fixture.control / "http-1/user-trust-1.json").exists(), "Fresh fixture unexpectedly owns OS trust"

        prepared = prepare(fixture)
        report["receipts"]["prepared"] = prepared
        record = prepared["rotation"]
        old_ca = transport.private_bytes(record["old_ca_certificate_file"], fixture.control, 8192)
        new_ca = transport.private_bytes(record["new_ca_certificate_file"], fixture.control, 8192)
        overlap = additive_context(old_ca, new_ca)
        assert digest(ssl.PEM_cert_to_DER_cert(old_ca.decode())) == record["old_ca_sha256"]
        assert digest(ssl.PEM_cert_to_DER_cert(new_ca.decode())) == record["new_ca_sha256"]
        print("Prepared two exact issuers; committing checkpointed replacement with private client trust.", flush=True)
        committed = rotation(fixture, "commit", prepared)
        report["receipts"]["committed"] = committed
        assert committed["rotation"]["phase"] == "activated"
        fixture.managed_health = fixture.rpc("health")["observation"]
        assert fixture.managed_health["instance"] != original_health["instance"]
        for field in ("runtime_sha256", "executable_sha256"):
            assert fixture.managed_health[field] == original_health[field]
        after = json.loads(fixture.run("runtime", "http-connection-info", "--install-dir", fixture.install,
                                      "--connection-id", connection_id).stdout)
        same_scope(before["selection"], after["selection"])
        assert after["connection_id"] == connection_id
        assert digest(transport.private_bytes(after["credential_file"], fixture.control, 103)) == credential_sha
        assert after["endpoint"]["ca_sha256"] == record["new_ca_sha256"]
        transport.validate_native_identity(after, fixture.managed_health)
        report["old_only_tls_refused"] = stale_issuer_refused(after["endpoint"], old_ca)
        try:
            rpc(original_network, 99, "tools/list", {})
        except (OSError, http.client.HTTPException, ValueError) as error:
            report["old_session_unusable"] = type(error).__name__
        else:
            raise AssertionError("Predecessor HTTP session survived issuer replacement")
        replacement = transport.McpClient(after, fixture.control)
        replacement.connection.close()
        port = int(after["endpoint"]["url"].split(":")[-1].split("/")[0])
        replacement.connection = http.client.HTTPSConnection("127.0.0.1", port, context=overlap, timeout=60)
        handshake(replacement)
        report["after_read"] = search(replacement)
        assert fixture.published_status("issuer-fixture")["publication"] == publication
        assert digest(source.read_bytes()) == source_sha

        expires = committed["rotation"]["overlap_expires_unix_ms"]
        close_overlap_before_wait(replacement, expires)
        replacement = None
        report["overlap_session_delete_acknowledged_before_wait"] = True
        finalized = rotation(fixture, "finalize", committed)
        report["receipts"]["finalized"] = finalized
        assert finalized["rotation"]["phase"] == "finalized"
        for role in ("old", "current-old"):
            stage = Path(record["old_ca_certificate_file"]).parent.parent / role
            assert not (stage / "tls-identity.json").exists()
            assert (stage / "retired-private-key.json").is_file()
        assert not (fixture.control / "http-1/user-trust-1.json").exists()
        # Drop the overlap client's context and reconnect using only the new
        # local issuer plus unchanged system roots. No OS trust was installed.
        replacement = transport.McpClient(after, fixture.control)
        handshake(replacement)
        report["post_finalize_read"] = search(replacement)

        active_sha = digest((fixture.control / "http-1/tls-identity.json").read_bytes())
        cancel = prepare(fixture)
        aborted = rotation(fixture, "abort", cancel)
        report["receipts"]["aborted"] = aborted
        assert aborted["rotation"]["phase"] == "aborted"
        assert digest((fixture.control / "http-1/tls-identity.json").read_bytes()) == active_sha
        assert not (Path(cancel["rotation"]["new_ca_certificate_file"]).parent / "tls-identity.json").exists()
        report["post_abort_read"] = search(replacement)
        report["credential_bytes_unchanged"] = True
        report["scope_and_publication_unchanged"] = True
        report["passed"] = True
    except BaseException as error:
        primary = error
        report["failure"] = {"type": type(error).__name__, "message": str(error)[:8192]}
    finally:
        for network in (replacement, original_network):
            if network is not None:
                network.close(delete=False)
        try:
            if fixture.managed_started:
                current = fixture.rpc("health")["observation"]
                if current["executable_sha256"] == digest(fixture.binary.read_bytes()):
                    fixture.managed_health = current  # Owned exact image/control, including an interrupted reply.
            fixture.close()
            report["cleanup"] = "native_stop_complete"
        except BaseException as error:
            report["passed"] = False
            report["cleanup"] = {"state": "retained_outcome_unknown", "type": type(error).__name__, "message": str(error)[:8192]}
            if primary is None:
                primary = error
        (owner / "issuer-rotation-report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if primary is not None:
        raise primary
    print(f"Issuer rotation native/TLS acceptance passed; OS trust and external hosts not qualified. Evidence: {owner}")


if __name__ == "__main__":
    main()
