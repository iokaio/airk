# SPDX-License-Identifier: Apache-2.0
"""Prepare this checkout's Codex connection from one verified local build.

Preserve existing collection/storage and tool policies. No agent execution,
publication, private-state export, or modification of user-level configuration.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import sys
import threading
import time
import tomllib
import uuid

LIMIT = 1_048_576


def require(condition, message):
    if not condition:
        raise ValueError(message)


def regular(path):
    """Reject redirected configuration paths and multiply linked files."""
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


def binding(args):
    require(isinstance(args, list) and len(args) == 7 and args[-1] == 'mcp',
            'Expected a direct AIRK stdio launch with root, data directory and collection')
    values = {}
    for flag, value in zip(args[0:6:2], args[1:6:2]):
        require(flag in ('--root', '--data-dir', '--collection') and flag not in values
                and isinstance(value, str) and value, 'Ambiguous AIRK launch arguments')
        values[flag] = value
    require(Path(values['--root']).is_absolute() and Path(values['--data-dir']).is_absolute(),
            'AIRK root and storage must be absolute paths')
    return values


def replace_scalar(source, table, key, value):
    """Edit only simple generated tables; semantic comparison guards the result."""
    header = re.compile(r'^\[' + re.escape(table) + r'\][ \t]*(?:#[^\r\n]*)?\r?$', re.M)
    headers = list(header.finditer(source))
    require(len(headers) <= 1, 'Ambiguous Codex table')
    newline = '\r\n' if '\r\n' in source else '\n'
    assignment = f'{key} = {json.dumps(value)}{newline}'
    if not headers:
        return source + (newline if source and not source.endswith('\n') else '') + f'[{table}]{newline}' + assignment
    start = source.find('\n', headers[0].start())
    if start == -1:
        return source + newline + assignment
    start += 1
    following = re.search(r'^\[', source[start:], re.M)
    end = start + following.start() if following else len(source)
    body = source[start:end]
    pattern = re.compile(r'^([ \t]*' + re.escape(key) + r'[ \t]*=[ \t]*)(?:"(?:[^"\\\r\n]|\\.)*"|\x27[^\x27\r\n]*\x27)([ \t]*(?:#[^\r\n]*)?)(\r?\n|$)', re.M)
    matches = list(pattern.finditer(body))
    require(len(matches) <= 1, 'Ambiguous Codex setting')
    if matches:
        match = matches[0]
        body = body[:match.start()] + match[1] + json.dumps(value) + match[2] + match[3] + body[match.end():]
    else:
        body = assignment + body
    return source[:start] + body + source[end:]


def prepare_config(before, root, helper, data_dir=None, new_data_dir=None):
    source = (before or b'').decode('utf-8')
    parsed = tomllib.loads(source)
    servers = parsed.get('mcp_servers', {})
    require(isinstance(servers, dict), 'Invalid MCP server table')
    candidates = []
    for name, server in servers.items():
        if name.startswith('airk-') or name == 'airk':
            require(isinstance(server, dict), 'Invalid AIRK configuration')
            scope = binding(server.get('args'))
            if Path(scope['--root']).resolve() == root:
                candidates.append((name, server))
    require(len(candidates) <= 1, 'Multiple AIRK servers target this workspace; resolve duplicates first')
    expected = copy.deepcopy(parsed)
    if candidates:
        name, server = candidates[0]
        require(re.fullmatch(r'[A-Za-z0-9_-]+', name), 'Unsupported AIRK server name')
        require('url' not in server and server.get('enabled', True) is True,
                'AIRK is disabled or uses a different transport; existing policy was preserved')
        require(Path(server.get('command', '')).name.lower() in ('airk', 'airk.exe'),
                'Custom AIRK launch wrapper was preserved; configure it manually')
        scope = binding(server['args'])
        require(data_dir is None or Path(scope['--data-dir']).resolve() == data_dir.resolve(),
                'Requested storage differs from the existing collection; storage was preserved')
        env = server.get('env', {})
        require(isinstance(env, dict) and all(isinstance(v, str) for v in env.values()), 'Invalid AIRK environment')
        source = replace_scalar(source, f'mcp_servers.{name}', 'command', str(helper))
        source = replace_scalar(source, f'mcp_servers.{name}.env', 'AIRK_CLIENT', 'codex')
        expected['mcp_servers'][name]['command'] = str(helper)
        expected['mcp_servers'][name].setdefault('env', {})['AIRK_CLIENT'] = 'codex'
    else:
        data_dir = data_dir or new_data_dir
        require(data_dir is not None, 'New configuration requires --data-dir matching the VS Code collection')
        require(data_dir.is_absolute(), 'New storage directory must be absolute')
        identity = str(root).lower() if os.name == 'nt' else str(root)
        collection = 'project-' + hashlib.sha256(identity.encode()).hexdigest()[:24]
        name = 'airk-' + collection
        require(name not in servers, 'AIRK server name belongs to another root')
        server = {'command': str(helper), 'args': ['--root', str(root), '--data-dir', str(data_dir),
                                                  '--collection', collection, 'mcp'],
                  'env': {'AIRK_CLIENT': 'codex'}}
        newline = '\r\n' if '\r\n' in source else '\n'
        source += (newline if source and not source.endswith('\n') else '')
        source += f'[mcp_servers.{name}]{newline}command = {json.dumps(str(helper))}{newline}'
        source += f'args = {json.dumps(server["args"])}{newline}[mcp_servers.{name}.env]{newline}AIRK_CLIENT = "codex"{newline}'
        expected.setdefault('mcp_servers', {})[name] = server
    require(tomllib.loads(source) == expected,
            'Unsupported TOML layout; unrelated settings were preserved. Use a standard AIRK table')
    server = expected['mcp_servers'][name]
    required = {'airk_status', 'airk_search', 'airk_read_file'}
    require(not required.intersection(server.get('disabled_tools', []))
            and ('enabled_tools' not in server or required.issubset(server['enabled_tools'])),
            'Existing tool policy disables local smoke-test tools; policy was preserved')
    return source.encode('utf-8'), name, server


def write_config(path, before, after):
    require(read_config(path) == before, 'Configuration changed during setup; nothing was replaced')
    require(before is None or path.stat().st_mode & 0o200, 'Configuration is read-only; nothing was replaced')
    path.parent.mkdir(parents=True, exist_ok=True)
    regular(path)
    temporary = path.with_name(path.name + '.' + uuid.uuid4().hex + '.tmp')
    try:
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'wb') as stream:
            stream.write(after)
            stream.flush()
            os.fsync(stream.fileno())
        if before is not None:
            os.chmod(temporary, path.stat().st_mode & 0o777)
        require(read_config(path) == before, 'Configuration changed during setup; nothing was replaced')
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def process_options():
    return {'creationflags': subprocess.CREATE_NO_WINDOW} if os.name == 'nt' else {}


def run_setup_step(label, command, *, timeout, wait_for_writer=False, **options):
    """Capture diagnostics explicitly: hidden Windows children lack a console."""
    deadline = time.monotonic() + timeout
    next_notice = 0
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise RuntimeError(f'{label} timed out after {timeout} seconds waiting for the collection writer. '
                               'The existing writer was left running; retry setup after it finishes.')
        try:
            return run_setup_attempt(label, command, timeout=remaining, **options)
        except subprocess.CalledProcessError as error:
            stderr = error.stderr or ''
            lines = [line.strip() for line in stderr.splitlines()]
            # This exact native error is raised before the writer lock is acquired
            # and before indexing mutates the collection. Never retry other failures.
            busy = ('Error: index writer locked' in lines
                    and 'lock acquisition failed because the operation would block' in lines)
            if wait_for_writer and busy:
                now = time.monotonic()
                if now >= deadline:
                    continue
                if now >= next_notice:
                    print(f'{label}: another AIRK process is writing this collection; '
                          f'waiting and retrying ({max(1, round(deadline - now))} seconds remaining)...', flush=True)
                    next_notice = now + 30
                time.sleep(min(5, deadline - now))
                continue
            details = '\n'.join(value.strip() for value in (error.stdout, error.stderr) if value and value.strip())
            raise RuntimeError(f'{label} failed (exit {error.returncode}):\n'
                               f'{details[-16384:] or "The helper returned no diagnostic output."}') from None


def run_setup_attempt(label, command, **options):
    try:
        return subprocess.run(command, check=True, capture_output=True, text=True,
                              encoding='utf-8', errors='replace', **options).stdout
    except subprocess.TimeoutExpired as error:
        # TimeoutExpired retains bytes even when text=True.
        details = error.stderr or error.stdout or b''
        if isinstance(details, bytes):
            details = details.decode('utf-8', errors='replace')
        raise RuntimeError(f'{label} timed out after {error.timeout} seconds.\n{details[-16384:]}') from None


def prepare_vectors(server, root):
    """Honor collection opt-out; install missing assets through native quota checks."""
    command = [server['command'], *server['args'][:-1]]
    env = {**os.environ, **server.get('env', {}), 'AIRK_CLIENT': 'cli'}
    options = dict(cwd=root, env=env, **process_options())
    status = json.loads(subprocess.check_output(
        [*command, 'collections', 'vectors', 'show'], text=True, timeout=60, **options))
    if not status['enabled']:
        print('Vector search is opted out for this collection; preserving lexical search.', flush=True)
        return
    local = Path(binding(server['args'])['--data-dir']) / 'models' / 'local'
    if local.exists() or local.is_symlink():
        # Never overwrite custom or incomplete models. Indexing validates all assets.
        regular(local / 'model.json')
        require((local / 'model.json').is_file(), 'Existing local model is incomplete; repair it before retrying')
        return
    cache = root / 'target' / 'local-vector-model'
    print('Preparing the default local vector model (about 91 MB on first setup)...', flush=True)
    run_setup_step('Model download and validation',
                   [sys.executable, str(Path(__file__).with_name('provision-local-model.py')),
                    '--destination', str(cache), '--download'], timeout=600, **options)
    run_setup_step('Native model installation', [*command, 'collections', 'vectors', 'install', str(cache)],
                   timeout=300, wait_for_writer=True, **options)


def smoke(server, version, root):
    """Check the actual configured launch through MCP, including cited live bytes."""
    scope = binding(server['args'])
    env = {**os.environ, **server.get('env', {}), 'AIRK_USAGE_MODE': 'disabled'}
    child = subprocess.Popen([server['command'], *server['args']], cwd=root, env=env,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                             **process_options())
    responses = queue.Queue()

    def receive():
        try:
            total = 0
            while True:
                line = child.stdout.readline(LIMIT + 1)
                total += len(line)
                require(total <= 2 * LIMIT and len(line) <= LIMIT, 'MCP response exceeded budget')
                require(line, 'MCP helper exited before verification finished')
                responses.put(json.loads(line))
        except Exception as error:
            responses.put(error)

    reader = threading.Thread(target=receive, daemon=True)
    reader.start()
    deadline = time.monotonic() + 45

    def send(message):
        child.stdin.write(json.dumps(message).encode() + b'\n')
        child.stdin.flush()

    def request(number, method, params):
        send({'jsonrpc': '2.0', 'id': number, 'method': method, 'params': params})
        while True:
            remaining = deadline - time.monotonic()
            require(remaining > 0, 'MCP verification timed out')
            result = responses.get(timeout=remaining)
            if isinstance(result, Exception):
                raise result
            if result.get('id') == number:
                require('error' not in result, f'MCP {method} failed')
                return result['result']

    def call(number, name, arguments):
        result = request(number, 'tools/call', {'name': name, 'arguments': arguments})
        require(not result.get('isError'), f'MCP {name} failed; check helper/index compatibility')
        return json.loads(result['content'][0]['text'])

    try:
        init = request(1, 'initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                        'clientInfo': {'name': 'airk-local-build-check', 'version': '1'}})
        require(init.get('serverInfo', {}).get('version') == version, 'MCP helper version differs from the installed candidate')
        send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
        names = {tool['name'] for tool in request(2, 'tools/list', {})['tools']}
        require({'airk_status', 'airk_search', 'airk_read_file'} <= names, 'Required MCP tools are missing')
        status = call(3, 'airk_status', {})
        require(status.get('collection') == scope['--collection'] and status.get('files', 0) > 0,
                'MCP did not open the expected indexed collection')
        token = status['publication']['token']
        source = (root / 'README.md').read_bytes()
        end_line = min(8, len(source.splitlines()))
        require(end_line > 0, 'README is empty; no source evidence to verify')
        result = call(4, 'airk_read_file', {'path': 'README.md', 'start_line': 1, 'end_line': end_line,
                                         'revision': token, 'max_bytes': 4096})
        hits = result.get('hits', [])
        digest = hashlib.sha256(source).hexdigest()
        require(hits and all(hit['citation']['path'] == 'README.md'
                            and hit['citation']['content_hash'] == digest for hit in hits),
                'MCP README evidence does not match the current source; index and retry')
        words = re.findall(r'[A-Za-z]{5,}', hits[0]['text'])
        require(words, 'README has no suitable smoke-test search term')
        found = call(5, 'airk_search', {'mode': 'lexical', 'query': words[0], 'path_prefix': 'README.md',
                                      'limit': 1, 'max_bytes': 2048})
        require(found.get('collection') == scope['--collection'] and any(
                    hit.get('citation', {}).get('path') == 'README.md'
                    and hit['citation'].get('content_hash') == digest for hit in found.get('hits', [])),
                'MCP lexical retrieval did not return the known README term')
        enabled = status.get('vectors_enabled')
        require(isinstance(enabled, bool), 'MCP helper did not report the vector policy')
        if enabled:
            require(status.get('local_model_ready') and status.get('indexed_vectors', 0) > 0
                    and 'diskann' in status.get('vector_backend', '').lower(),
                    'Vector search is enabled but the model/DiskANN index is not ready; rerun setup without --skip-index')
        for number, mode in ((6, None), (7, 'vector')):
            if mode == 'vector' and not enabled:
                continue
            arguments = {'query': hits[0]['text'][:512] if enabled else words[0],
                         'path_prefix': 'README.md', 'limit': 1, 'max_bytes': 4096}
            if mode:
                arguments['mode'] = mode
            found = call(number, 'airk_search', arguments)
            require(found.get('publication', {}).get('token') == token and any(
                hit.get('citation', {}).get('path') == 'README.md'
                and hit['citation'].get('content_hash') == digest for hit in found.get('hits', [])),
                f'MCP {mode or "default"} retrieval did not return revision-matched README evidence')
        return {'helper_version': version, 'collection': scope['--collection'], 'generation': status['generation'],
                'tool_count': len(names), 'status': 'passed', 'read': 'passed', 'search': 'passed',
                'default_search': 'passed', 'vector_search': 'passed' if enabled else 'opted_out',
                'indexed_vectors': status.get('indexed_vectors', 0),
                'agent_session': 'restart_required', 'probe_recording': 'disabled'}
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.terminate()
            child.wait(timeout=5)
        reader.join(timeout=1)
        child.stdout.close()


def setup(build_record, root, data_dir=None, skip_index=False, new_data_dir=None):
    root = root.resolve()
    run = build_record.resolve()
    require(run.is_relative_to(root / 'artifacts' / 'local'), 'Build record must be inside artifacts/local')
    (run / 'local-agent-setup.json').unlink(missing_ok=True)
    build = json.loads((run / 'result.json').read_bytes())
    installed = json.loads((run / 'local-install.json').read_bytes())
    native = json.loads((run / 'native' / 'publish' / 'manifest.json').read_bytes())
    helper = run / 'native' / 'stage' / 'airk.exe'
    regular(helper)
    version = native['version']
    require(build['schema'] == 'airk-local-build/1' and installed['schema'] == 'airk-local-install/1'
            and native['schema'] == 'airk-native-candidate/1' and native['target'] == 'x86_64-pc-windows-msvc'
            and installed['version'] == version and installed['vsix'] == build['vsix'], 'Local build/install identity differs')
    require(hashlib.sha256(helper.read_bytes()).hexdigest() == native['payload']['airk.exe'], 'Candidate helper digest differs')
    require(subprocess.check_output([str(helper), '--version'], text=True, **process_options()).strip() == f'airk {version}',
            'Candidate helper version differs')
    config = root / '.codex' / 'config.toml'
    before = read_config(config)
    after, name, server = prepare_config(before, root, helper, data_dir, new_data_dir)
    index_measurements = None
    if not skip_index:
        prepare_vectors(server, root)
        print('Indexing the existing workspace collection with the verified candidate...', flush=True)
        index_started = time.monotonic()
        index_report = json.loads(run_setup_step('Workspace indexing', [server['command'], *server['args'][:-1], 'index'], cwd=root,
                       env={**os.environ, **server.get('env', {}), 'AIRK_CLIENT': 'cli'},
                       timeout=3600, wait_for_writer=True, **process_options()))
        index_measurements = indexing_measurements(index_report, time.monotonic() - index_started)
    smoke(server, version, root)
    if before != after:
        write_config(config, before, after)
    saved = tomllib.loads(read_config(config).decode())['mcp_servers'][name]
    require(saved == server, 'Saved AIRK configuration differs from the tested launch')
    result = smoke(saved, version, root)
    require(read_config(config) == after, 'Configuration changed during verification; no success receipt was written')
    receipt = {'schema': 'airk-local-agent-setup/1', 'client': 'codex', 'config': str(config),
               'helper': str(helper), 'indexed': not skip_index, 'verification': result}
    if index_measurements is not None:
        receipt['index_measurements'] = index_measurements
    (run / 'local-agent-setup.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    print(f'Codex configured and MCP retrieval verified: AIRK {version}, collection {result["collection"]}.')
    print('Restart existing Codex sessions to load the updated configuration. Keep this build directory while it is configured.')
    return receipt


def indexing_measurements(report, elapsed_seconds):
    """Only aggregate counters/timings, never source paths or skipped-file diagnostics."""
    fields = ('generation', 'indexed', 'reused', 'removed', 'source_bytes', 'scan_extract_us', 'publish_us',
              'embedding_cache_hits', 'embedding_cache_misses', 'embedding_inferences', 'embedding_no_vector',
              'embedding_inputs', 'embedding_blank_inputs', 'embedding_duplicate_inputs', 'embedding_actual_forwards',
              'embedding_model_load_us', 'embedding_cache_lookup_us', 'embedding_cache_store_us',
              'embedding_inference_us', 'vector_graph_insert_us', 'embedding_workers', 'embedding_batches')
    measured = {name: report[name] for name in fields if name in report}
    require(all(type(value) is int and value >= 0 for value in measured.values()), 'Invalid indexing measurement')
    return {'wall_ms': round(elapsed_seconds * 1000, 3), **measured}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-record', type=Path, required=True)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--data-dir', type=Path, help='Required for first setup; must match existing storage when provided')
    parser.add_argument('--new-data-dir', type=Path, help='Fallback storage for first setup only; existing bindings win')
    parser.add_argument('--skip-index', action='store_true', help='Verify an already current index; stale evidence fails')
    args = parser.parse_args()
    try:
        setup(args.build_record, args.root, args.data_dir, args.skip_index, args.new_data_dir)
    except (RuntimeError, ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f'Local agent setup failed: {error}\n')


if __name__ == '__main__':
    main()
