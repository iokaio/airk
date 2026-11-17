# SPDX-License-Identifier: Apache-2.0
"""Exact installed Claude 2.1.118 control-channel MCP acceptance; no model turn.

Prepared only. Execution must use a freshly source-sealed runtime. The installed
CLI's mcp_call projection omits MCP isError; retain this limitation and the raw
control response instead of manufacturing a flag or claiming envelope parity.
"""
import argparse
import hashlib
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

ROOT = Path(__file__).resolve().parents[4]
EXPECTED_CLAUDE_SHA256 = '5a05b03b880de26bb78fb9824bacf08e99dcf05c02e3c66546cccaf2f2930b2e'
ALLOWED = {'initialize', 'mcp_status', 'mcp_call', 'mcp_reconnect'}


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


shared = load(Path(__file__).with_name('codex-native-acceptance.py'), 'airk_actual_client_shared')
clients, transport = shared.clients, shared.transport


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def redacted(value, token):
    return json.loads(json.dumps(value).replace(token, '[REDACTED_FIXTURE_CREDENTIAL]'))


class Claude:
    """Only reviewed control requests; never writes a user or model message."""
    def __init__(self, binary, home, cwd, bundle, token, config, stderr_file):
        self.events, self.identifier, self.requests = queue.Queue(), 0, []
        self.secret, self.closed = token, False
        home.mkdir(exist_ok=True)
        self.stderr = stderr_file.open('xb')
        self.journal_path = stderr_file.with_name(stderr_file.name + '.control.jsonl')
        self.journal = self.journal_path.open('xb')
        self.journal_lock, self.journal_bytes, self.journal_events = threading.Lock(), 0, 0
        env = {key: value for key, value in os.environ.items()
               if not any(word in key.upper() for word in ('TOKEN', 'SECRET', 'API_KEY', 'AUTHORIZATION'))
               and not key.upper().startswith(('CLAUDE', 'ANTHROPIC', 'VSCODE_', 'CODEX_'))}
        # Only the owned child sees these homes/credentials. Bare mode avoids
        # hooks, keychain, plugin sync, memory and CLAUDE.md discovery. No user
        # prompt is ever sent, and accidental model traffic has no API identity.
        env.update(HOME=str(home), USERPROFILE=str(home), CLAUDE_CONFIG_DIR=str(home / '.claude'),
                   APPDATA=str(home / 'AppData/Roaming'), LOCALAPPDATA=str(home / 'AppData/Local'),
                   XDG_CONFIG_HOME=str(home / '.config'), NODE_EXTRA_CA_CERTS=str(bundle),
                   AIRK_CLAUDE_FIXTURE_TOKEN=token, ANTHROPIC_BASE_URL='http://127.0.0.1:9',
                   CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1')
        args = [str(binary), '--bare', '--print', '--input-format', 'stream-json',
                '--output-format', 'stream-json', '--verbose', '--no-session-persistence',
                '--strict-mcp-config', '--mcp-config', str(config), '--setting-sources', '',
                '--settings', '{"disableAllHooks":true}', '--tools', '',
                '--disable-slash-commands', '--permission-mode', 'dontAsk']
        self.child = subprocess.Popen(args, cwd=cwd, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8',
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)

        def stdout():
            total = 0
            try:
                for line in self.child.stdout:
                    total += len(line)
                    if len(line) > 2097152 or total > 16777216:
                        raise RuntimeError('Claude control output bound exceeded')
                    value = json.loads(line)
                    # Retain every bounded raw response before any contract or
                    # workload assertion can fail; expanded headers are redacted.
                    self.record('received', value)
                    self.events.put(value)
            except BaseException as error:
                self.events.put(error)
            finally:
                self.events.put(EOFError('Claude control output closed'))

        def stderr():
            total = 0
            for line in self.child.stderr:
                data = line.replace(token, '[REDACTED_FIXTURE_CREDENTIAL]').encode()
                if total + len(data) <= 65536:
                    self.stderr.write(data)
                    total += len(data)

        self.readers = [threading.Thread(target=stdout, daemon=True), threading.Thread(target=stderr, daemon=True)]
        for reader in self.readers:
            reader.start()
        try:
            self.rpc('initialize', {})
        except BaseException:
            self.close()
            raise

    def rpc(self, subtype, arguments, *, allow_error=False, deadline=None):
        assert subtype in ALLOWED, 'No model/query/context-usage operation is allowed'
        until = min(deadline if deadline is not None else float('inf'), time.monotonic() + 90)
        clients.remaining(until, 90)
        self.identifier += 1
        identifier = str(self.identifier)
        request = {'type': 'control_request', 'request_id': identifier,
                   'request': {'subtype': subtype, **arguments}}
        self.requests.append(redacted(request, self.secret))
        self.record('submitted', request)
        self.child.stdin.write(json.dumps(request) + '\n')
        self.child.stdin.flush()
        while True:
            value = self.events.get(timeout=clients.remaining(until, 90))
            if isinstance(value, BaseException):
                raise value
            assert value.get('type') not in ('control_request', 'assistant', 'user'), 'Unexpected tool/auth/model request'
            if value.get('type') == 'control_response':
                reply = value['response']
                assert reply['request_id'] == identifier, 'Unexpected control response identity'
                retained = redacted(reply, self.secret)
                if not allow_error:
                    assert reply.get('subtype') == 'success', retained
                return retained

    def record(self, direction, message):
        value = {'direction': direction, 'message': redacted(message, self.secret)}
        data = (json.dumps(value) + '\n').encode()
        with self.journal_lock:
            assert self.journal_bytes + len(data) <= 16777216, 'Redacted control journal byte bound exceeded'
            self.journal.write(data)
            self.journal.flush()
            self.journal_bytes += len(data)
            self.journal_events += 1

    def inventory(self, *, connected=True):
        until = time.monotonic() + 30
        observations = []
        while True:
            response = self.rpc('mcp_status', {}, deadline=until)
            observations.append(response)
            servers = response.get('response', {}).get('mcpServers', [])
            selected = [server for server in servers if server['name'] == 'airk']
            if len(selected) == 1 and selected[0].get('status') in ('connected', 'failed', 'needs-auth'):
                assert (selected[0]['status'] == 'connected') is connected, observations
                return selected[0], observations
            assert time.monotonic() + .1 < until, observations
            time.sleep(.1)

    def close(self):
        if self.closed:
            return {'closed': True, 'already_closed': True}
        self.closed = True
        self.child.stdin.close()
        forced = False
        try:
            self.child.wait(timeout=15)
        except subprocess.TimeoutExpired:
            forced = True
            self.child.terminate()
            self.child.wait(timeout=10)
        for reader in self.readers:
            reader.join(timeout=5)
        self.stderr.close()
        self.journal.close()
        assert all(not reader.is_alive() for reader in self.readers), 'Owned Claude pipe reader survived exit'
        return {'closed': True, 'forced': forced, 'exit_code': self.child.returncode,
                'raw_control_journal': str(self.journal_path), 'raw_control_journal_sha256': digest(self.journal_path),
                'raw_control_events': self.journal_events,
                'model_turn_requests': 0, 'control_subtypes': [row['request']['subtype'] for row in self.requests]}


def write_config(path, url):
    value = {'mcpServers': {'airk': {'type': 'http', 'url': url,
             'headers': {'Authorization': 'Bearer ${AIRK_CLAUDE_FIXTURE_TOKEN}',
                         'X-AIRK-Response-Mode': 'full'}}}}
    with path.open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2)
    return digest(path)


def body_of(response):
    assert response.get('subtype') == 'success', response
    value = response['response']
    content = value['content']
    if isinstance(content, str):
        body = json.loads(content)
    else:
        assert isinstance(content, list) and len(content) == 1 and content[0].get('type') == 'text', value
        body = json.loads(content[0]['text'])
    assert isinstance(body, dict), value
    return body


def actual_calls(app, original, crossed):
    server, observations = app.inventory()
    assert server.get('serverInfo', {}).get('name') == 'airk', server
    assert server.get('serverInfo', {}).get('version') == '1.8.14', server
    names = [tool['name'] for tool in server.get('tools', [])]
    assert {'status', 'read_file', 'search'} <= set(names), server
    result = {'inventory_observations': observations, 'calls': [],
              'projection_limit': 'Installed mcp_call omits isError; exact raw control replies retained, no manufactured native flag.'}
    def call(tool, arguments):
        response = app.rpc('mcp_call', {'tool': 'mcp__airk__' + tool,
                                      'arguments': {'response_mode': 'full', **arguments}}, allow_error=True)
        result['calls'].append({'tool': tool, 'arguments': arguments, 'response': response})
        return response
    status = body_of(call('status', {}))
    assert status['publication']['token'] == original['token'], status
    read = body_of(call('read_file', {'path': 'fixture.txt', 'start_line': 1, 'end_line': 1}))
    assert read['publication']['token'] == original['token'], read
    assert read['hits'][0]['citation']['path'] == 'fixture.txt' and 'cobalt-only-original' in read['hits'][0]['text'], read
    denied = call('read_file', {'path': 'outside.txt', 'project_ids': crossed['project_ids'],
                              'index_ids': [crossed['index_id']]})
    if denied.get('subtype') == 'success':
        refused = body_of(denied)
        assert refused.get('error') or refused.get('status') in ('error', 'failed', 'refused'), refused
        assert not refused.get('hits'), refused
    else:
        assert denied.get('subtype') == 'error' and denied.get('error'), denied
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-dir', type=Path, required=True)
    parser.add_argument('--claude', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--client-cwd-base', type=Path, required=True)
    args = parser.parse_args()
    assert digest(args.claude) == EXPECTED_CLAUDE_SHA256, 'Installed reviewed Claude image changed'
    version = subprocess.check_output([args.claude, '--version'], text=True).strip()
    assert version == '2.1.118 (Claude Code)', version
    builder = shared.load('build-service-runtime')
    build = json.loads((args.runtime_dir / 'airk-build.json').read_text())
    assert builder.source_identity() == build['source'], 'Freeze/current production inputs differ from sealed runtime'
    root = Path(os.environ['AIRK_PRIVATE_CENSUS_BASE']).resolve(strict=True)
    owner = args.output.resolve()
    assert owner.is_relative_to(root) and owner != root
    transport.contained_private_path(owner.parent, root)
    owner.mkdir(exist_ok=False)
    cwd_base = args.client_cwd_base.resolve(strict=True)
    assert cwd_base.is_dir() and not cwd_base.is_relative_to(ROOT), 'Actual Claude cwd must exclude checkout discovery'
    cwd_owner = tempfile.TemporaryDirectory(prefix='airk-claude-native-', dir=cwd_base)
    cwd = Path(cwd_owner.name).resolve(strict=True)
    fixture = clients.Fixture(args.runtime_dir, owner, command_timeout=300)
    report = {'schema': 1, 'passed': False, 'client_version': version, 'client_sha256': digest(args.claude),
              'runtime_manifest_sha256': digest(args.runtime_dir / 'airk-install.json'),
              'model_turn_requests': 0, 'os_trust_mutations': False, 'scenarios': {},
              'actual_client': 'Exact installed Claude trusted control channel; initialize/mcp_status/mcp_call only'}
    app = trusted = wrong = None
    try:
        certificates = owner / 'fictional-certificates'
        shared.generate_certificates(certificates)
        fixture.start()
        data = owner / 'data'
        data.mkdir()
        originals = {}
        for name, filename, text in [('allowed', 'fixture.txt', 'cobalt-only-original source identity preserved\n'),
                                     ('outside', 'outside.txt', 'foreign-only-secret-fixture\n')]:
            source = owner / name
            source.mkdir()
            file = source / filename
            file.write_text(text)
            originals[file] = digest(file)
            fixture.register(name, source, data)
            fixture.configure(name, {'operation': 'vectors', 'enabled': False})
            fixture.job(name, 'index')
        original = fixture.published_status('allowed')['publication']
        connection = fixture.enroll_http({'kind': 'scoped', 'selection': fixture.scopes['allowed']},
                                        uuid.uuid4().hex, 'claude_code')
        transport.validate_native_identity(connection, fixture.managed_health)
        token = transport.private_bytes(connection['credential_file'], fixture.control, 103).decode()
        native_ca = transport.private_bytes(connection['ca_certificate_file'], fixture.control, 32768)
        inherited_path = os.environ.get('NODE_EXTRA_CA_CERTS')
        inherited = Path(inherited_path).read_bytes() if inherited_path else b''
        bundle = owner / 'additive-ca.pem'
        bundle.write_bytes(inherited + b'\n' + (certificates / 'ca-a.pem').read_bytes() + native_ca)
        config = owner / 'explicit-mcp-config.json'
        config_hash = write_config(config, connection['endpoint']['url'])
        report['credential_identity'] = {'connection_id': connection['connection_id'],
            'selection': connection['selection'], 'generation': connection['endpoint']['generation'],
            'config_sha256': config_hash, 'inherited_extra_ca_preserved': bool(inherited)}
        for phase in ('first', 'fresh_process_reconnect'):
            app = Claude(args.claude, owner / ('home-' + phase), cwd, bundle, token, config,
                         owner / ('claude-' + phase + '.stderr'))
            report['scenarios'][phase] = actual_calls(app, original, fixture.scopes['outside'])
            report['scenarios'][phase]['close'] = app.close()
            app = None
        trusted = shared.FictionalTlsServer(certificates, 'a')
        trusted_config = owner / 'preserved-root-config.json'
        write_config(trusted_config, trusted.url)
        app = Claude(args.claude, owner / 'home-preserved-root', cwd, bundle, token, trusted_config,
                     owner / 'claude-preserved-root.stderr')
        server, observations = app.inventory()
        assert server.get('serverInfo', {}).get('name') == 'fictional-retained-trust-root', server
        report['scenarios']['preserved_root'] = {'observations': observations, 'http_requests': len(trusted.traffic),
                                                'close': app.close()}
        app = None
        wrong = shared.FictionalTlsServer(certificates, 'b')
        wrong_config = owner / 'wrong-issuer-config.json'
        write_config(wrong_config, wrong.url)
        app = Claude(args.claude, owner / 'home-wrong-issuer', cwd, bundle, token, wrong_config,
                     owner / 'claude-wrong-issuer.stderr')
        server, observations = app.inventory(connected=False)
        assert not wrong.traffic, 'Wrong issuer received HTTP before TLS validation'
        report['scenarios']['wrong_issuer'] = {'observations': observations, 'http_requests': 0,
                                             'authorization_headers': 0, 'close': app.close()}
        app = None
        assert digest(config) == config_hash
        assert all(digest(file) == expected for file, expected in originals.items())
        report['source_bytes_preserved'] = True
        report['passed'] = True
    finally:
        cleanup_errors = []
        try:
            if app is not None:
                try:
                    report['unfinished_client_close'] = app.close()
                except BaseException as error:
                    cleanup_errors.append({'owner': 'claude', 'error': str(error)[:2048]})
            for server in (trusted, wrong):
                if server is not None:
                    try:
                        server.close()
                    except BaseException as error:
                        cleanup_errors.append({'owner': 'fictional_tls_server', 'error': str(error)[:2048]})
            try:
                fixture.close()
                report['native_stop_complete'] = fixture.managed_stop is not None and fixture.managed_stop['record']['phase'] == 'complete'
            except BaseException as error:
                cleanup_errors.append({'owner': 'native_fixture', 'error': str(error)[:2048]})
        finally:
            try:
                cwd_owner.cleanup()
                report['owned_external_client_cwd_removed'] = True
            except BaseException as error:
                cleanup_errors.append({'owner': 'external_client_cwd', 'error': str(error)[:2048]})
            report['cleanup_errors'] = cleanup_errors
            report['passed'] = report['passed'] and report.get('native_stop_complete', False) and not cleanup_errors
            with args.report.open('x', encoding='utf-8') as output:
                json.dump(redacted(report, token if 'token' in locals() else '[UNISSUED]'), output, indent=2)
                output.write('\n')
    assert report['passed']
    print(json.dumps({'passed': True, 'client_version': version, 'model_turn_requests': 0, 'native_stop_complete': True}))


if __name__ == '__main__':
    main()
