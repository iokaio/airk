# SPDX-License-Identifier: Apache-2.0
"""Qualify occupied-port relocation on an explicitly approved isolated install."""
import argparse
import copy
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import socket
import ssl
import sys
import threading
import time
from urllib.parse import urlsplit
import uuid

PREFERRED_PORT = 57421


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Unavailable(RuntimeError):
    pass


class ForeignListener:
    """An owned foreign occupant; record contacts, never process HTTP credentials."""
    def __init__(self, port):
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        if os.name == 'nt':
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        else:
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            self.socket.bind(('127.0.0.1', port))
            self.socket.listen(4)
        except OSError as error:
            self.socket.close()
            if error.errno == errno.EADDRINUSE or getattr(error, 'winerror', None) == 10048:
                raise Unavailable(f'Cannot own requested collision port {port}; existing occupant untouched') from error
            raise
        self.port = self.socket.getsockname()[1]
        self.socket.settimeout(.1)
        self.closed = threading.Event()
        self.lock = threading.Lock()
        self.contacts = []
        self.error = None
        self.thread = threading.Thread(target=self._serve, name='airk-owned-port-occupant', daemon=True)
        self.thread.start()

    def _serve(self):
        try:
            while not self.closed.is_set():
                try:
                    with self.lock:
                        stream, _ = self.socket.accept()
                        if len(self.contacts) >= 16:
                            stream.close()
                            self.error = 'owned listener contact bound exceeded'
                            return
                        row = {'bytes': 0, 'tls_client_hello': False, 'application_data': False,
                               'plaintext_authorization': False, 'complete': False}
                        self.contacts.append(row)
                except socket.timeout:
                    self.closed.wait(.005)
                    continue
                with stream:
                    stream.settimeout(2)
                    data = b''
                    try:
                        initial = stream.recv(65536)
                        data += initial
                        # Deliberately invalid TLS, not a replacement CA or an
                        # imitation AIRK server. A validating client must stop.
                        stream.sendall(b'HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n')
                        while len(data) < 65536:
                            block = stream.recv(65536 - len(data))
                            if not block:
                                break
                            data += block
                    except (ConnectionError, socket.timeout):
                        pass
                    records, offset = [], 0
                    while len(data) - offset >= 5:
                        kind, size = data[offset], int.from_bytes(data[offset + 3:offset + 5], 'big')
                        records.append(kind)
                        offset += 5 + size
                    with self.lock:
                        row.update(bytes=len(data), tls_client_hello=len(data) >= 6 and data[0] == 22 and data[5] == 1,
                                   application_data=23 in records, plaintext_authorization=b'authorization:' in data.lower(),
                                   complete=True)
        except OSError as error:
            if not self.closed.is_set():
                self.error = str(error)

    def observations(self):
        with self.lock:
            if self.error:
                raise RuntimeError('Owned listener observation failed: ' + self.error)
            return copy.deepcopy(self.contacts)

    def require_uncontacted(self):
        if self.observations():
            raise AssertionError('Native relocation contacted its owned foreign port occupant')

    def close(self):
        self.closed.set()
        self.socket.close()
        self.thread.join(timeout=3)
        if self.thread.is_alive():
            raise RuntimeError('Owned foreign listener did not settle')


def immutable_scope(selection):
    value = copy.deepcopy(selection)
    if value.get('kind') != 'scoped':
        raise AssertionError('Port qualification requires an exact reviewed native scope')
    value['selection'].pop('expected_catalog_revision')
    return value


def validate_relocation(before, after, old_health, new_health):
    a, b = before['endpoint'], after['endpoint']
    if before['connection_id'] != after['connection_id'] or immutable_scope(before['selection']) != immutable_scope(after['selection']):
        raise AssertionError('Explicit reconnect changed credential identity or immutable authority')
    if after['selection']['selection']['expected_catalog_revision'] < before['selection']['selection']['expected_catalog_revision']:
        raise AssertionError('Explicit reconnect regressed the reviewed catalog clock')
    for field in ('installation', 'user', 'namespace', 'ca_sha256', 'executable_sha256'):
        if a[field] != b[field]:
            raise AssertionError('Relocation changed endpoint identity: ' + field)
    if a['url'] == b['url'] or b['generation'] <= a['generation']:
        raise AssertionError('Occupied saved port did not advance the actual endpoint generation and URL')
    if old_health['instance'] == new_health['instance'] or a['service_instance'] != old_health['instance'] or b['service_instance'] != new_health['instance']:
        raise AssertionError('Restart failed to replace the exact service incarnation')
    for field in ('runtime_sha256', 'executable_sha256'):
        if old_health[field] != new_health[field]:
            raise AssertionError('Restart replaced the approved runtime payload')


def stale_probe(transport, connection, control, occupant):
    occupant.require_uncontacted()
    try:
        client = transport.McpClient(connection, control, timeout=3)
    except ssl.SSLError:
        # McpClient connects and validates TLS before storing/sending bearer
        # bytes. Constructor refusal proves no HTTP request could be issued.
        pass
    else:
        client.close(delete=False)
        raise AssertionError('Stale URL unexpectedly established validating TLS')
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        rows = occupant.observations()
        if rows and rows[-1]['complete']:
            break
        time.sleep(.01)
    rows = occupant.observations()
    if len(rows) != 1 or not rows[0]['complete'] or not rows[0]['tls_client_hello'] or rows[0]['application_data'] or rows[0]['plaintext_authorization']:
        raise AssertionError('Stale-URL TLS-only refusal was not completely observed')
    return {'tls_refused_before_client_construction': True, 'occupant_observations': rows,
            'scope': 'invalid TLS occupant; not an untrusted-CA or certificate-rotation campaign'}


def read_marker(transport, connection, control):
    client = transport.McpClient(connection, control)
    arguments = {'path':'sentinel.txt','start_line':1,'end_line':1,'response_mode':'full'}
    try:
        client.send({'jsonrpc':'2.0','id':1,'method':'initialize','params':{
            'protocolVersion':'2025-11-25','capabilities':{},
            'clientInfo':{'name':'airk-owned-port-relocation','version':'1'}}})
        initialized = client.receive(1)
        if initialized['result']['serverInfo']['name'] != 'airk':
            raise AssertionError('Relocation client negotiated another MCP server')
        client.send({'jsonrpc':'2.0','method':'notifications/initialized'})
        client.send({'jsonrpc':'2.0','id':2,'method':'tools/call',
                     'params':{'name':'read_file','arguments':arguments}})
        results = {'initialize': initialized, 'mcp': client.receive(2), 'api': client.api_read('read_file', arguments)}
        for method in ('mcp','api'):
            envelope = results[method]['result'] if method == 'mcp' else results[method]
            if envelope.get('isError') is True or 'Port relocation preserves this fictional authority.' not in json.dumps(envelope):
                raise AssertionError('Reconnected ' + method + ' scope did not return its retained source')
        return results
    finally:
        client.close()


def register_project(fixture, owner):
    # Native reviewed setup may create its one storage leaf (data/name), but
    # requires an existing pinned parent. Never ask it to create missing parents.
    source = owner / 'source'; source.mkdir()
    data = owner / 'data'; data.mkdir()
    (source / 'sentinel.txt').write_text('Port relocation preserves this fictional authority.\n', encoding='utf-8')
    fixture.register('port-relocation', source, data)
    fixture.job('port-relocation', 'index')


def qualify(runtime, approved, report):
    runtime = runtime.resolve(strict=True)
    if report.resolve().is_relative_to(runtime):
        raise ValueError('Qualification report must remain outside the approved runtime')
    actual = hashlib.sha256((runtime / 'airk-install.json').read_bytes()).hexdigest()
    if not re.fullmatch('[0-9a-f]{64}', approved) or approved != actual:
        raise ValueError('Explicit approved installer digest differs')
    if json.loads((runtime / 'airk-runtime.json').read_text())['schema'] != 2:
        raise ValueError('Port relocation requires a modern single-image runtime')
    report.parent.mkdir(parents=True, exist_ok=True)
    with report.open('x', encoding='utf-8') as stream:
        proof = {'schema': 'airk.http-port-relocation/1', 'status': 'running', 'approved_install_sha256': approved,
                 'platform': sys.platform, 'principals': 1, 'production_native_changed': False,
                 'architecture': platform.machine(), 'os_release': platform.release(),
                 'qualification_script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                 'runtime_build': json.loads((runtime / 'airk-build.json').read_text()),
                 'limitations': ['One current OS principal; no two-user or cross-platform qualification.',
                                 'Private fixture CA trust only; no user trust-store or installed editor changes.',
                                 'Not a signed-distribution, minimum-OS, CA-rotation or real coding-client campaign.']}
        def retain():
            stream.seek(0); json.dump(proof, stream, indent=2); stream.write('\n'); stream.truncate(); stream.flush()
        retain()
        preferred = saved = fixture = None
        try:
            preferred = ForeignListener(PREFERRED_PORT)
            clients, transport = load('check-service-clients'), load('http_fixture')
            with clients.owned_client_fixture() as temporary:
                owner = Path(temporary)
                proof['fixture_root'] = str(owner); retain()
                fixture = clients.Fixture(runtime, owner)
                try:
                    proof['phase'] = 'isolated_managed_activation'; retain()
                    fixture.start()
                    register_project(fixture, owner)
                    identifier = uuid.uuid4().hex
                    before = fixture.enroll_http({'kind': 'scoped', 'selection': fixture.scopes['port-relocation']}, identifier)
                    old_health = fixture.rpc('health')['observation']
                    transport.validate_native_identity(before, old_health)
                    old_port = urlsplit(before['endpoint']['url']).port
                    if old_port == PREFERRED_PORT:
                        raise AssertionError('Service claimed the occupied preferred port')
                    old_token = transport.private_bytes(before['credential_file'], fixture.control, 103)
                    proof['phase'] = 'preferred_collision_scoped_mcp_and_api'; retain()
                    proof['before_result'] = read_marker(transport, before, fixture.control)
                    preferred.require_uncontacted()
                    proof['before_connection'] = before
                    proof['phase'] = 'exact_instance_stop'; retain()
                    fixture.close()  # Exact instance+runtime, durable completed Stop receipt.
                    proof['stop_receipt'] = fixture.managed_stop; retain()
                    saved = ForeignListener(old_port)
                    fixture.managed_stop = None; fixture.managed_health = None
                    proof['phase'] = 'saved_collision_explicit_start'; retain()
                    restarted = json.loads(fixture.run('runtime','start','--install-dir',fixture.install).stdout)
                    fixture.managed_health = fixture.rpc('health')['observation']
                    if restarted['instance'] != fixture.managed_health['instance']:
                        raise AssertionError('Explicit Start receipt differs from the reconnected native instance')
                    after = json.loads(fixture.run('runtime','http-connection-info','--install-dir',fixture.install,
                                                  '--connection-id',identifier).stdout)
                    # The command returns the connection DTO, without new enrollment.
                    transport.validate_native_identity(after, fixture.managed_health)
                    validate_relocation(before, after, old_health, fixture.managed_health)
                    if transport.private_bytes(after['credential_file'], fixture.control, 103) != old_token:
                        raise AssertionError('Relocation changed the enrolled bearer')
                    preferred.require_uncontacted(); saved.require_uncontacted()
                    proof.update(restart=restarted, after_connection=after, token_preserved=True,
                                 preferred_native_contacts=0, saved_native_contacts=0)
                    proof['phase'] = 'explicit_client_reconnect'; retain()
                    proof['after_result'] = read_marker(transport, after, fixture.control)
                    preferred.require_uncontacted(); saved.require_uncontacted()
                    proof['phase'] = 'stale_url_tls_refusal'; retain()
                    proof['stale_probe'] = stale_probe(transport, before, fixture.control, saved)
                    proof['status'] = 'passed'
                finally:
                    fixture.close()
                    proof['final_stop_receipt'] = fixture.managed_stop
                preferred.require_uncontacted()
                if len(saved.observations()) != 1:
                    raise AssertionError('Unexpected foreign-port contact after the intentional TLS probe')
                proof['phase'] = 'completed'
        except BaseException as error:
            proof['status'] = 'unavailable' if isinstance(error, Unavailable) else 'failed'
            proof['error_type'] = type(error).__name__
            retain()
            raise
        finally:
            try:
                if saved: saved.close()
                if preferred: preferred.close()
            except BaseException:
                proof['status'] = 'failed'
                proof['cleanup'] = 'owned listener settlement unavailable'
                raise
            finally:
                retain()
        return proof


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--approved-sha256', required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = qualify(args.runtime, args.approved_sha256, args.report)
    except Unavailable as error:
        print(str(error), file=sys.stderr)
        return 2
    print(json.dumps({'status': result['status'], 'report': str(args.report)}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
