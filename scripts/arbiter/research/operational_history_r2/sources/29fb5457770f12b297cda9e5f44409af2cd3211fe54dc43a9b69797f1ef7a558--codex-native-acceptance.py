# SPDX-License-Identifier: Apache-2.0
"""Isolated actual Codex MCP acceptance; never sends a model turn."""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import queue
import ssl
import subprocess
import sys
import threading
import time
import tempfile
from urllib.parse import urlsplit
import uuid

ROOT = Path(__file__).resolve().parents[4]
ALLOWED = {'initialize', 'thread/start', 'mcpServerStatus/list', 'mcpServer/tool/call'}


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


clients, transport = load('check-service-clients'), load('http_fixture')


def generate_certificates(directory):
    # New fictional issuers/leaves only. Issuer private keys are never persisted.
    from cryptography import x509
    from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    from ipaddress import ip_address
    directory.mkdir()
    now = datetime.now(timezone.utc)
    for label in ('a', 'b'):
        ca_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'Owned fictional Codex acceptance issuer ' + label)])
        ca = (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(ca_key.public_key())
              .serial_number(x509.random_serial_number()).not_valid_before(now-timedelta(days=1))
              .not_valid_after(now+timedelta(days=2)).add_extension(x509.BasicConstraints(ca=True,path_length=0),critical=True)
              .sign(ca_key,hashes.SHA256()))
        key = rsa.generate_private_key(public_exponent=65537,key_size=2048)
        leaf = (x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,'127.0.0.1')]))
                .issuer_name(name).public_key(key.public_key()).serial_number(x509.random_serial_number())
                .not_valid_before(now-timedelta(days=1)).not_valid_after(now+timedelta(days=1))
                .add_extension(x509.BasicConstraints(ca=False,path_length=None),critical=True)
                .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ip_address('127.0.0.1'))]),critical=False)
                .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]),critical=False).sign(ca_key,hashes.SHA256()))
        (directory/('ca-'+label+'.pem')).write_bytes(ca.public_bytes(serialization.Encoding.PEM))
        (directory/('leaf-'+label+'.pem')).write_bytes(leaf.public_bytes(serialization.Encoding.PEM))
        (directory/('key-'+label+'.pem')).write_bytes(key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.PKCS8,serialization.NoEncryption()))


class FictionalTlsServer:
    def __init__(self, certificates, label):
        self.traffic = []
        outer = self
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = 'HTTP/1.1'
            def log_message(self, *_):
                pass
            def do_POST(self):
                size = int(self.headers.get('Content-Length', '0'))
                assert 0 <= size <= 65536
                body = json.loads(self.rfile.read(size))
                outer.traffic.append({'method': body.get('method'), 'authorization_present': bool(self.headers.get('Authorization'))})
                if body.get('id') is None:
                    self.send_response(202); self.send_header('Content-Length', '0'); self.end_headers(); return
                method = body.get('method')
                result = ({'protocolVersion': body['params']['protocolVersion'], 'capabilities': {'tools': {}},
                           'serverInfo': {'name': 'fictional-retained-trust-root', 'version': '1'}} if method == 'initialize'
                          else {'tools': []} if method == 'tools/list'
                          else {'resources': []} if method == 'resources/list'
                          else {'resourceTemplates': []} if method == 'resources/templates/list' else {})
                encoded = json.dumps({'jsonrpc': '2.0', 'id': body['id'], 'result': result}).encode()
                self.send_response(200); self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(encoded))); self.end_headers(); self.wfile.write(encoded)
            def do_GET(self):
                self.send_response(405); self.send_header('Content-Length', '0'); self.end_headers()
            do_DELETE = do_GET
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.server.daemon_threads = True
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(certificates / ('leaf-' + label + '.pem'), certificates / ('key-' + label + '.pem'))
        self.server.socket = context.wrap_socket(self.server.socket, server_side=True)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = 'https://127.0.0.1:' + str(self.server.server_port) + '/mcp'
    def close(self):
        self.server.shutdown(); self.server.server_close(); self.thread.join(timeout=5)
        assert not self.thread.is_alive()


class Codex:
    def __init__(self, binary, home, cwd, bundle, token, evidence):
        self.events, self.replies, self.requests, self.identifier = queue.Queue(), {}, [], 0
        self.secret = token
        env = {key: value for key, value in os.environ.items()
               if not any(word in key.upper() for word in ('TOKEN', 'SECRET', 'API_KEY', 'AUTHORIZATION'))
               and not key.upper().startswith(('CODEX_', 'VSCODE_'))}
        env.update(CODEX_HOME=str(home), CODEX_CA_CERTIFICATE=str(bundle), AIRK_FIXTURE_TOKEN=token,
                   OPENAI_BASE_URL='http://127.0.0.1:9')
        self.stderr = evidence.open('xb')
        self.child = subprocess.Popen([str(binary), 'app-server', '--stdio'], cwd=cwd, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding='utf-8', creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
        def stdout():
            for line in self.child.stdout:
                if len(line) > 2097152:
                    self.events.put(RuntimeError('App-server response bound exceeded')); break
                try: self.events.put(json.loads(line))
                except ValueError: self.events.put(RuntimeError('App-server emitted non-JSON stdout'))
        def stderr():
            total = 0
            for line in self.child.stderr:
                data = line.replace(token, '[REDACTED_FIXTURE_CREDENTIAL]').encode()
                if total + len(data) <= 65536:
                    self.stderr.write(data); total += len(data)
        self.readers = [threading.Thread(target=stdout, daemon=True), threading.Thread(target=stderr, daemon=True)]
        for thread in self.readers: thread.start()
        try:
            self.rpc('initialize', {'clientInfo': {'name': 'airk-owned-native-client-acceptance', 'version': '1'},
                                    'capabilities': {'experimentalApi': True}})
            self.child.stdin.write(json.dumps({'method': 'initialized', 'params': {}}) + '\n'); self.child.stdin.flush()
        except BaseException:
            self.close()
            raise
    def rpc(self, method, params, *, deadline=None):
        assert method in ALLOWED, 'No model-turn or unrelated app-server request is allowed'
        deadline = min(deadline if deadline is not None else float('inf'), time.monotonic() + 90)
        clients.remaining(deadline, 90)
        self.identifier += 1; identifier = self.identifier
        self.requests.append({'method': method, 'params': params})
        self.child.stdin.write(json.dumps({'id': identifier, 'method': method, 'params': params}) + '\n')
        self.child.stdin.flush()
        while identifier not in self.replies:
            value = self.events.get(timeout=max(.01, deadline-time.monotonic()))
            if isinstance(value, BaseException): raise value
            if 'id' in value:
                assert 'method' not in value, 'Unexpected server request requires explicit review'
                self.replies[value['id']] = value
            assert time.monotonic() < deadline
        reply = self.replies.pop(identifier)
        assert 'error' not in reply, {'method': method, 'error': reply.get('error')}
        return reply['result']
    def close(self):
        self.child.stdin.close()
        try: self.child.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.child.terminate(); self.child.wait(timeout=10)
        for thread in self.readers: thread.join(timeout=5)
        assert all(not thread.is_alive() for thread in self.readers)
        self.stderr.close()
        return {'exit_code': self.child.returncode, 'closed': True, 'model_turn_requests': 0,
                'methods': [row['method'] for row in self.requests]}


def write_config(home, url, preserved_url=None):
    home.mkdir()
    text = '[analytics]\nenabled = false\n[mcp_servers.airk]\nurl = ' + json.dumps(url)
    text += '\nbearer_token_env_var = "AIRK_FIXTURE_TOKEN"\nstartup_timeout_sec = 20\n'
    if preserved_url:
        text += '\n[mcp_servers.preserved_root]\nurl = ' + json.dumps(preserved_url) + '\nstartup_timeout_sec = 20\n'
    path = home / 'config.toml'
    with path.open('x', encoding='utf-8') as output: output.write(text)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def actual_calls(app, thread_id, fixture, original, crossed, report):
    inventory = app.rpc('mcpServerStatus/list', {'threadId': thread_id, 'serverName': 'airk', 'detail': 'full'})
    server = next(row for row in inventory['data'] if row['name'] == 'airk')
    assert server.get('serverInfo', {}).get('name') == 'airk' and server.get('toolsError') is None, server
    assert server['serverInfo']['version'] == '1.8.14', server
    actual = urlsplit(report['expected_endpoint'])
    assert server['httpOrigin'] == actual.scheme + '://' + actual.netloc, 'Effective actual client endpoint changed'
    assert {'status', 'read_file', 'search'} <= set(server['tools']), server
    report['inventory'] = inventory
    def call(tool, args, expected_error=False):
        cohort = report.setdefault('raw_read_cohorts', {}).setdefault(tool, [])
        def read(_deadline):
            value = app.rpc('mcpServer/tool/call', {'threadId': thread_id, 'server': 'airk', 'tool': tool, 'arguments': args}, deadline=_deadline)
            cohort.append(value)
            return value
        if expected_error:
            # Deliberate negative; dispatch exactly once and retain refusal.
            result = read(time.monotonic() + 90)
        else:
            result, _ = clients.read_envelope(read, time.monotonic() + 90)
        assert bool(result.get('isError')) == expected_error, result
        return result
    status = call('status', {'response_mode': 'full'})
    status_body = clients.tool_value(status)
    assert status_body['publication']['token'] == original['token'], status_body
    read = call('read_file', {'path': 'fixture.txt', 'start_line': 1, 'end_line': 1, 'response_mode': 'full'})
    body = clients.tool_value(read)
    assert body['hits'] and body['hits'][0]['citation']['path'] == 'fixture.txt', body
    assert 'cobalt-only-original' in body['hits'][0]['text'], body
    assert body['publication']['token'] == original['token'], body
    denied = call('read_file', {'path': 'outside.txt', 'project_ids': crossed['project_ids'],
                              'index_ids': [crossed['index_id']], 'response_mode': 'full'}, expected_error=True)
    report.update(status=status, read_file=read, out_of_scope=denied)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--runtime-dir', type=Path, required=True)
    parser.add_argument('--codex', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='Fresh directory under the existing private fixture base')
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--client-cwd-base', type=Path, required=True)
    args = parser.parse_args()
    root = Path(os.environ['AIRK_PRIVATE_CENSUS_BASE']).resolve(strict=True)
    owner = args.output.resolve()
    assert owner.is_relative_to(root) and owner != root
    transport.contained_private_path(owner.parent, root)
    owner.mkdir(exist_ok=False)
    report = {'schema': 1, 'passed': False, 'model_turns': 0, 'os_trust_mutations': False,
              'client_version': subprocess.check_output([args.codex, '--version'], text=True).strip(),
              'runtime_manifest_sha256': hashlib.sha256((args.runtime_dir/'airk-install.json').read_bytes()).hexdigest(),
              'actual_client': 'Codex app-server direct MCP calls', 'scenarios': {}}
    fixture = clients.Fixture(args.runtime_dir, owner, command_timeout=300)
    certificates = owner / 'fictional-certificates'
    cwd_base = args.client_cwd_base.resolve(strict=True)
    assert cwd_base.is_dir() and not cwd_base.is_relative_to(ROOT), 'Actual Codex cwd must be outside checkout discovery'
    cwd_owner = tempfile.TemporaryDirectory(prefix='airk-codex-native-', dir=cwd_base)
    cwd = Path(cwd_owner.name).resolve(strict=True)
    assert cwd.is_relative_to(cwd_base) and cwd != cwd_base and not cwd.is_relative_to(ROOT)
    trusted = wrong = app = None
    try:
        # Generate new fictional keys under this owned private fixture; historical
        # TLS evidence and its owner-only files are never opened or changed.
        generate_certificates(certificates)
        report['fictional_certificates_generated_in_owned_fixture'] = True
        fixture.start()
        data = owner/'data'; data.mkdir()
        originals = {}
        for name, filename, text in [('allowed','fixture.txt','cobalt-only-original source identity preserved\n'),
                                     ('outside','outside.txt','foreign-only-secret-fixture\n')]:
            source = owner/name; source.mkdir(); file = source/filename; file.write_text(text)
            originals[file] = hashlib.sha256(file.read_bytes()).hexdigest()
            fixture.register(name, source, data)
            fixture.configure(name, {'operation':'vectors','enabled':False})
            fixture.job(name, 'index')
        original = fixture.published_status('allowed')['publication']
        connection = fixture.enroll_http({'kind':'scoped','selection':fixture.scopes['allowed']},uuid.uuid4().hex,'codex')
        transport.validate_native_identity(connection, fixture.managed_health)
        token = transport.private_bytes(connection['credential_file'], fixture.control, 103).decode()
        native_ca = transport.private_bytes(connection['ca_certificate_file'], fixture.control, 32768)
        bundle = owner/'additive-ca.pem'
        bundle.write_bytes((certificates/'ca-a.pem').read_bytes()+native_ca)
        trusted, wrong = FictionalTlsServer(certificates,'a'), FictionalTlsServer(certificates,'b')
        home = owner/'codex-home'; config_hash = write_config(home,connection['endpoint']['url'],trusted.url)
        report['credential_identity'] = {'connection_id':connection['connection_id'],'selection':connection['selection'],
            'generation':connection['endpoint']['generation'],'config_sha256':config_hash}
        app = Codex(args.codex,home,cwd,bundle,token,owner/'codex-first.stderr')
        started = app.rpc('thread/start',{'cwd':str(cwd),'approvalPolicy':'never','sandbox':'read-only','ephemeral':True})
        thread_id = started['thread']['id']
        first = report['scenarios'].setdefault('first',{'expected_endpoint':connection['endpoint']['url']})
        actual_calls(app,thread_id,fixture,original,fixture.scopes['outside'],first)
        preserved = app.rpc('mcpServerStatus/list',{'threadId':thread_id,'serverName':'preserved_root'})
        assert preserved['data'][0]['serverInfo']['name']=='fictional-retained-trust-root',preserved
        assert trusted.traffic and all(not row['authorization_present'] for row in trusted.traffic)
        report['preserved_existing_root'] = True
        report['scenarios']['first']['close'] = app.close(); app = None
        app = Codex(args.codex,home,cwd,bundle,token,owner/'codex-reconnect.stderr')
        reconnected = app.rpc('thread/start',{'cwd':str(cwd),'approvalPolicy':'never','sandbox':'read-only','ephemeral':True})
        thread_id = reconnected['thread']['id']
        reconnected = report['scenarios'].setdefault('reconnect',{'expected_endpoint':connection['endpoint']['url']})
        actual_calls(app,thread_id,fixture,original,fixture.scopes['outside'],reconnected)
        assert hashlib.sha256((home/'config.toml').read_bytes()).hexdigest()==config_hash
        report['scenarios']['reconnect']['close'] = app.close(); app = None
        negative_home = owner/'wrong-issuer-home'; write_config(negative_home,wrong.url)
        app = Codex(args.codex,negative_home,cwd,bundle,token,owner/'codex-wrong-issuer.stderr')
        refused = app.rpc('mcpServerStatus/list',{'serverName':'airk'})
        assert len(refused['data'])==1 and refused['data'][0]['serverInfo'] is None and refused['data'][0]['toolsError'],refused
        assert not wrong.traffic,'Wrong issuer received HTTP before TLS validation'
        report['scenarios']['wrong_issuer']={'http_requests':0,'authorization_headers':0,'discovery_refused':True,'close':app.close()};app=None
        assert all(hashlib.sha256(path.read_bytes()).hexdigest()==sha for path,sha in originals.items())
        report['source_bytes_preserved']=True
        report['passed']=True
    finally:
        if app is not None: report['unfinished_client_close']=app.close()
        for server in (trusted,wrong):
            if server is not None: server.close()
        try:
            fixture.close()
            report['native_stop_complete']=fixture.managed_stop is not None and fixture.managed_stop['record']['phase']=='complete'
        finally:
            assert cwd.resolve(strict=True).is_relative_to(cwd_base) and cwd != cwd_base
            cwd_owner.cleanup()
            report['owned_external_client_cwd_removed'] = True
            report['passed']=report['passed'] and report.get('native_stop_complete',False)
            args.report.write_text(json.dumps(report,indent=2)+'\n')
    assert report['passed']
    print(json.dumps({'passed':True,'client_version':report['client_version'],'model_turns':0,'native_stop_complete':True}))


if __name__ == '__main__':
    main()
