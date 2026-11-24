# SPDX-License-Identifier: Apache-2.0
"""Connect this checkout's Codex project file to the running AIRK service.

Verify one explicitly approved runtime pair, read the service's endpoint and the
exact manual entry through the pair's `airkctl`, smoke-test the loopback HTTP endpoint
with an explicit per-call read scope and, only with --apply-config, ask
`airkctl agents connect` to write the project entry. The entry names the live
endpoint URL only; configuration edits belong to airkctl's safe writer, never
to this script. No index, default, enrollment, agent execution, publication,
private-state export or user-level configuration change.
"""
import argparse
import hashlib
import importlib.util
import json
import platform
from pathlib import Path
import re
import subprocess
import sys

LIMIT = 1_048_576
FIRST_OPEN_READ_VERSION = (1, 8, 206)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def local_module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def process_options():
    return {'creationflags': subprocess.CREATE_NO_WINDOW} if sys.platform == 'win32' else {}


def regular(path):
    """Reject redirected paths and multiply linked files."""
    for item in (path, *path.parents):
        if item.exists() or item.is_symlink():
            info = item.lstat()
            require(not item.is_symlink() and not getattr(info, 'st_file_attributes', 0) & 0x400,
                    f'Redirected path is not supported: {item}')
    if path.exists():
        require(path.is_file() and path.stat().st_nlink == 1, f'Expected regular file: {path}')


def read_config(path):
    regular(path)
    if not path.exists():
        return None
    with path.open('rb') as stream:
        value = stream.read(LIMIT + 1)
    require(len(value) <= LIMIT, 'Codex configuration exceeds 1 MiB')
    return value


def native_json(binary, arguments):
    """Read-only native metadata: bounded size, fixed failure text."""
    result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                            text=True, encoding='utf-8', timeout=120, **process_options())
    require(result.returncode == 0, 'Verified native command refused')
    require(len(result.stdout.encode()) <= LIMIT, 'Native command returned an invalid metadata response')
    return json.loads(result.stdout)


def native_outcome(binary, arguments):
    """Keep a native command's complete structured outcome, including uncertainty."""
    try:
        result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                                text=True, encoding='utf-8', timeout=125, **process_options())
    except subprocess.TimeoutExpired as error:
        def partial(value):
            raw = value if isinstance(value, bytes) else (value or '').encode('utf-8')
            return {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
        return {'exit_code': None, 'transport_outcome': 'uncertain', 'timeout_seconds': error.timeout,
                'partial_stdout': partial(error.stdout), 'partial_stderr': partial(error.stderr)}
    raw = result.stdout if result.returncode == 0 else result.stderr
    try:
        if len(raw.encode()) > LIMIT:
            raise ValueError('invalid native response')
        value = json.loads(raw)
        if not isinstance(value, dict):
            raise ValueError('invalid native response')
    except ValueError:
        # A completed process with unreadable output does not prove whether its
        # admitted edit committed. Keep a typed transport outcome; never retry.
        return {'exit_code': result.returncode, 'transport_outcome': 'uncertain',
                'response_failure': 'invalid_native_response', 'response_bytes': len(raw.encode()),
                'response_sha256': hashlib.sha256(raw.encode()).hexdigest()}
    return {'exit_code': result.returncode, 'result': value}


def verify_runtime(runtime, approved):
    """Verify the approved inventory and return the matched pair's identities.

    Every listed file must match its sealed digest and no unlisted file may
    exist. Only a current schema-3 airk/airkctl pair for this host is accepted.
    """
    runtime = runtime.resolve(strict=True)
    require(re.fullmatch(r'[a-f0-9]{64}', approved or '') is not None, 'Explicit approved runtime SHA-256 required')
    raw = read_config(runtime / 'airk-install.json')
    require(raw is not None and hashlib.sha256(raw).hexdigest() == approved, 'Runtime inventory approval differs')
    manifest = json.loads(raw)
    require(manifest.get('schema') == 1 and isinstance(manifest.get('files'), dict)
            and 1 <= len(manifest['files']) <= 2048, 'Invalid runtime inventory')
    for name, expected in manifest['files'].items():
        require(isinstance(name, str) and name and '\\' not in name and ':' not in name
                and all(part not in ('', '.', '..') for part in name.split('/'))
                and not name.startswith('/') and re.fullmatch(r'[a-f0-9]{64}', expected or ''), 'Unsafe runtime inventory')
        item = runtime / name
        regular(item)
        require(item.is_file() and item.resolve().is_relative_to(runtime), 'Runtime payload path differs')
        with item.open('rb') as stream:
            require(hashlib.file_digest(stream, 'sha256').hexdigest() == expected, 'Runtime payload seal differs')
    actual = {item.relative_to(runtime).as_posix() for item in runtime.rglob('*') if item.is_file()}
    require(actual == set(manifest['files']) | {'airk-install.json'}, 'Unlisted runtime files preserved; approval refused')
    pair = json.loads(read_config(runtime / 'airk-runtime.json'))
    require(pair.get('schema') == 3, 'A current matched airk/airkctl runtime pair is required')
    clients = local_module('local_setup_pair_contract', 'check-service-clients.py')
    require(clients.classify_runtime(runtime)['topology'] == 'split_pair', 'Invalid matched runtime pair')
    require(pair['version'] == manifest['version'] and pair['os'] == manifest['os']
            and pair['arch'] == manifest['arch'], 'Runtime metadata identity differs')
    require(re.fullmatch(r'\d+\.\d+\.\d+', pair['version']) is not None
            and tuple(map(int, pair['version'].split('.'))) >= FIRST_OPEN_READ_VERSION,
            'Setup requires a current matched airk/airkctl pair')
    host_os = {'win32': 'windows', 'darwin': 'macos', 'linux': 'linux'}.get(sys.platform)
    host_arch = {'amd64': 'x86_64', 'x86_64': 'x86_64', 'arm64': 'aarch64', 'aarch64': 'aarch64'}.get(platform.machine().lower())
    require(pair['os'] == host_os and pair['arch'] == host_arch, 'Selected runtime does not match this client host')
    suffix = '.exe' if pair['os'] == 'windows' else ''
    prefix = 'AIRK.app/Contents/MacOS/' if pair['os'] == 'macos' else ''
    require(manifest['files'].get(prefix + 'airk' + suffix) == pair['airk_sha256'], 'Main executable identity differs')
    require(manifest['files'].get(prefix + 'airkctl' + suffix) == pair['airkctl_sha256'], 'Matched airkctl executable identity differs')
    forbidden = {'airk-embed' + suffix, 'airk-launcher' + suffix, 'airk-service' + suffix, 'airk-index' + suffix,
                 'airk-source' + suffix, 'onnxruntime.dll', 'libonnxruntime.so', 'libonnxruntime.dylib'}
    require(not any(Path(name).name in forbidden for name in manifest['files']), 'Payload contains an obsolete runtime role')
    return {'cli': runtime / (prefix + 'airkctl' + suffix), 'cli_sha256': pair['airkctl_sha256'],
            'version': pair['version'], 'main_sha256': pair['airk_sha256']}


def http_smoke(connection, control, version, root, collection=None, *, scope=None):
    """Generic real loopback HTTP/MCP verification; this does not execute the Codex client."""
    http = local_module('local_setup_http', 'http_fixture.py')
    client = http.McpClient(connection, control, 125, scope=scope)
    sequence = 0
    def request(method, params):
        nonlocal sequence
        sequence += 1
        client.send({'jsonrpc': '2.0', 'id': sequence, 'method': method, 'params': params})
        reply = client.receive(sequence)
        require('error' not in reply, 'HTTP MCP protocol refused verification')
        return reply['result']
    def call(name, arguments):
        result = request('tools/call', {'name': name, 'arguments': arguments})
        require(result.get('isError') is not True, 'HTTP evidence read refused; index/scope remains unchanged')
        text = next((block['text'] for block in result.get('content', []) if block.get('type') == 'text'), None)
        require(isinstance(text, str), 'Missing complete HTTP evidence result')
        return json.loads(text)
    try:
        initialized = request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                             'clientInfo': {'name': 'airk-local-build-check', 'version': '1'}})
        require(initialized.get('serverInfo', {}).get('version') == version, 'HTTP service version differs from the selected build')
        client.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
        tools = request('tools/list', {})['tools']
        require({'status', 'search', 'read_file'} <= {tool['name'] for tool in tools}, 'Required HTTP tools missing')
        status = call('status', {'response_mode': 'full'})
        require(status.get('index_state') == 'available' and status.get('files', 0) > 0
                and (collection is None or status.get('collection') == collection), 'Expected saved publication unavailable')
        source = (root / 'README.md').read_bytes()
        digest = hashlib.sha256(source).hexdigest()
        end = min(8, len(source.splitlines()))
        require(end > 0, 'README evidence is empty')
        read = call('read_file', {'path': 'README.md', 'start_line': 1, 'end_line': end,
                                 'revision': status['publication']['token'], 'max_bytes': 4096,
                                 'verification': 'require', 'response_mode': 'full'})
        require(read.get('hits') and all(hit['citation']['path'] == 'README.md'
                and hit['citation']['content_hash'] == digest for hit in read['hits']), 'HTTP README differs from saved local evidence')
        words = re.findall(r'[A-Za-z]{5,}', read['hits'][0]['text'])
        require(words, 'README evidence has no suitable lexical term')
        search = call('search', {'query': words[0], 'mode': 'lexical', 'path_prefix': 'README.md',
                                 'limit': 1, 'max_bytes': 2048, 'verification': 'require', 'response_mode': 'full'})
        require(any(hit.get('citation', {}).get('path') == 'README.md' and hit['citation'].get('content_hash') == digest
                    for hit in search.get('hits', [])), 'HTTP search did not retain the verified source identity')
        require((root / 'README.md').read_bytes() == source, 'Source changed during HTTP verification')
        return {'transport': 'http', 'service_version': version, 'collection': status.get('collection'),
                'publication': status['publication'], 'tool_count': len(tools), 'status': 'passed',
                'tool_catalog': tools,
                'read': 'passed', 'search': 'passed', 'source_sha256': digest,
                'codex_client_execution': 'not_run', 'codex_environment': 'not_configured_by_probe'}
    finally:
        client.close()

def setup(runtime, approved, control, root, *, scope, apply_config=False, report=None):
    """Connect this checkout's Codex project file through `airkctl agents connect`.

    Explicit index/set scope or the existing workspace default is required; this
    creates no index, default or credential, and Python never edits configuration.
    """
    root = root.resolve(strict=True)
    require(control.is_absolute(), 'Explicit absolute control directory required')
    control = control.resolve(strict=True)
    if report is not None:
        report = report.resolve()
        require(report.is_relative_to(root / 'artifacts' / 'local'), 'Setup evidence must remain under checkout artifacts/local')
        regular(report)
        require(not report.exists(), 'Setup report already exists; nothing changed')
    require(root != Path.home().resolve(), 'User-level Codex configuration must not be modified')
    http = local_module('local_setup_open_http', 'http_fixture.py')
    scope = http.scope_value(**scope)
    require(scope, 'Explicit index, index-set, or existing workspace-default scope required')
    identity = verify_runtime(runtime, approved)
    cli = identity['cli']
    with cli.open('rb') as stream:
        require(hashlib.file_digest(stream, 'sha256').hexdigest() == identity['cli_sha256'], 'Matched airkctl identity differs')
    version = identity['version']
    prefix = ['--output', 'json', '--service-runtime-dir', control]
    endpoint_view = native_json(cli, [*prefix, 'agents', 'endpoint'])
    endpoint = {key: value for key, value in endpoint_view.items() if key != 'manual'}
    require(endpoint == http.read_endpoint(control) and endpoint['executable_sha256'] == identity['main_sha256'],
            'Native endpoint differs from the approved running image or persisted endpoint')
    manual = native_json(cli, [*prefix, 'agents', 'manual', '--client', 'codex', '--project', root])
    require(manual.get('client') == 'codex' and manual.get('scope') == 'project'
            and manual.get('format') == 'toml' and Path(manual.get('file', '')) == root / '.codex' / 'config.toml'
            and isinstance(manual.get('text'), str), 'Native manual entry target differs')
    config = root / '.codex' / 'config.toml'
    before = read_config(config)
    verification = http_smoke(endpoint, control, version, root, scope=scope)
    require(read_config(config) == before, 'Configuration changed during verification; no edit requested')
    outcome = None
    status = 'manual_configuration_ready'
    if apply_config:
        outcome = native_outcome(cli, [*prefix, 'agents', 'connect', '--client', 'codex', '--project', root])
        if outcome.get('transport_outcome') == 'uncertain':
            status = 'configuration_outcome_unknown'
        elif outcome['exit_code'] == 0:
            status = 'configuration_saved'
        else:
            reason = (outcome['result'].get('native') or {}).get('reason')
            status = {'automatic_policy': 'manual_configuration_required',
                      'conflict_retained': 'configuration_conflict_retained',
                      'outcome_unknown': 'configuration_outcome_unknown'}.get(reason, 'native_setup_failed')
    receipt = {'schema': 'airk-local-agent-setup/3', 'client': 'codex', 'config': str(config),
               'selected_runtime_sha256': approved, 'endpoint': endpoint, 'read_scope': scope,
               'manual': manual, 'native_edit': outcome, 'status': status, 'indexed': False,
               'configuration_preimage_sha256': hashlib.sha256(before or b'').hexdigest(),
               'client_environment_loaded': False, 'verification': verification,
               'limitations': ['The actual Codex client was not executed.',
                               'Manual text uses the current port; regenerate after a service port change.',
                               'Other client configurations are neither inspected nor changed.']}
    if report is not None:
        report.parent.mkdir(parents=True, exist_ok=True)
        with report.open('x', encoding='utf-8') as destination:
            destination.write(json.dumps(receipt, indent=2) + '\n')
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-dir', type=Path, required=True, help='Explicit verified runtime distribution (current matched airk/airkctl pair)')
    parser.add_argument('--approved-sha256', required=True, help='Externally approved exact airk-install.json SHA-256')
    parser.add_argument('--control-dir', type=Path, required=True, help='Existing control directory of the running service')
    parser.add_argument('--index', action='append', dest='index_ids', help='Exact Index id/name; repeat for a union')
    parser.add_argument('--index-set', action='append', dest='index_set_ids', help='Exact Index set id/name')
    parser.add_argument('--workspace-default', action='store_true', help='Use the root folder existing default instead of named indexes/sets; creates no default')
    parser.add_argument('--apply-config', action='store_true', help='Run airkctl agents connect for the project Codex file; otherwise print the manual entry without editing')
    parser.add_argument('--report', type=Path, help='Optional new evidence file under checkout artifacts/local')
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        require(not args.workspace_default or not (args.index_ids or args.index_set_ids),
                'Choose named indexes/sets or the existing workspace default')
        read_scope = ({'workspace': str(args.root.resolve())} if args.workspace_default else
                      {key: value for key, value in (('index_ids', args.index_ids), ('index_set_ids', args.index_set_ids)) if value})
        receipt = setup(args.runtime_dir, args.approved_sha256, args.control_dir, args.root,
                        scope=read_scope, apply_config=args.apply_config, report=args.report)
        print(json.dumps(receipt, indent=2))
        if receipt['status'] not in ('manual_configuration_ready', 'configuration_saved'):
            parser.exit(1, 'Native setup did not save configuration; use the retained typed outcome and manual entry.\n')
    except (RuntimeError, ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, f'Local agent setup failed: {error}\n')


if __name__ == '__main__':
    main()
