# SPDX-License-Identifier: Apache-2.0
"""Prepare this checkout's Codex connection from one verified local build.

Preserve existing collection/storage and tool policies. No agent execution,
publication, private-state export, or modification of user-level configuration.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
import platform
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


def local_module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def native_json(binary, arguments):
    """Metadata only: bounded results and fixed failures never print credentials."""
    result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                            text=True, encoding='utf-8', timeout=120, **process_options())
    require(result.returncode == 0, 'Verified native setup command refused')
    require(len(result.stdout.encode()) <= LIMIT and 'airk1.' not in result.stdout,
            'Native setup returned an invalid metadata response')
    return json.loads(result.stdout)


def immutable_selection(value):
    if isinstance(value, dict):
        return {key: immutable_selection(item) for key, item in value.items() if key != 'expected_catalog_revision'}
    if isinstance(value, list):
        return [immutable_selection(item) for item in value]
    return value


def verify_modern_runtime(runtime, approved):
    runtime = runtime.resolve(strict=True)
    require(re.fullmatch(r'[a-f0-9]{64}', approved or '') is not None, 'Explicit approved runtime SHA-256 required')
    manifest_path = runtime / 'airk-install.json'
    raw = read_config(manifest_path)
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
    require(set(pair) == {'schema', 'version', 'os', 'arch', 'airk_sha256'} and pair['schema'] == 2
            and pair['version'] == manifest['version'] and pair['os'] == manifest['os']
            and pair['arch'] == manifest['arch'], 'Modern single-image runtime required')
    require(re.fullmatch(r'\d+\.\d+\.\d+', pair['version']) is not None
            and tuple(map(int, pair['version'].split('.'))) >= (1, 8, 14), 'Modern runtime version required')
    host_os = {'win32': 'windows', 'darwin': 'macos', 'linux': 'linux'}.get(sys.platform)
    host_arch = {'amd64': 'x86_64', 'x86_64': 'x86_64', 'arm64': 'aarch64', 'aarch64': 'aarch64'}.get(platform.machine().lower())
    require(pair['os'] == host_os and pair['arch'] == host_arch, 'Selected runtime does not match this client host')
    suffix = '.exe' if pair['os'] == 'windows' else ''
    main = ('AIRK.app/Contents/MacOS/' if pair['os'] == 'macos' else '') + 'airk' + suffix
    require(manifest['files'].get(main) == pair['airk_sha256'], 'Modern main executable identity differs')
    forbidden = {'airk-embed'+suffix, 'airk-launcher'+suffix, 'airk-service'+suffix, 'airk-index'+suffix, 'airk-source'+suffix,
                 'onnxruntime.dll', 'libonnxruntime.so', 'libonnxruntime.dylib'}
    require(not any(Path(name).name in forbidden for name in manifest['files']), 'Modern payload contains an obsolete runtime role')
    return runtime / main, pair['version'], pair['airk_sha256']


def owned_server(before, root, *, candidate_only=False):
    parsed = tomllib.loads((before or b'').decode('utf-8'))
    servers = parsed.get('mcp_servers', {})
    require(isinstance(servers, dict), 'Invalid MCP server table')
    found = []
    for name, value in servers.items():
        require(name != 'airk_explorer', 'Historical split Explorer declaration requires complete native ownership and scope review; all configuration preserved and cleanup blocked')
        if name == 'airk' or name.startswith('airk-'):
            require(isinstance(value, dict) and re.fullmatch(r'[A-Za-z0-9_-]+', name), 'Invalid AIRK server table')
            if candidate_only:
                found.append((name, value))
                continue
            if 'url' in value:
                # An HTTP declaration has no filesystem scope. Its native saved
                # connection must prove the immutable authority before editing.
                found.append((name, value))
            else:
                scope = binding(value.get('args'))
                if Path(scope['--root']).resolve() == root:
                    require(Path(value.get('command', '')).name.lower() in ('airk', 'airk.exe'), 'Custom AIRK wrapper preserved')
                    found.append((name, value))
    require(len(found) <= 1, 'Multiple AIRK servers require explicit review')
    return found[0] if found else ('airk', None)


def launcher_observation(binary, install, approved):
    value = native_json(binary, ['runtime', 'inspect-legacy-launchers', '--install-dir', install, '--expected-active', approved])
    require(type(value.get('schema')) is int and value['schema'] == 1 and value.get('active') == approved, 'Legacy launcher inspection changed the selected runtime')
    for role in ('launcher', 'universal_launcher'):
        record = value.get(role)
        require(record is None or isinstance(record, dict) and set(record) == {'path', 'sha256'}
                and isinstance(record['path'], str) and Path(record['path']).is_absolute()
                and re.fullmatch(r'[a-f0-9]{64}', record['sha256'] or ''), 'Invalid native launcher lineage')
    images = value.get('legacy_runtime_images', [])
    require(isinstance(images,list), 'Invalid native legacy runtime image census')
    for record in images:
        require(isinstance(record,dict) and set(record) == {'path','sha256','runtime_sha256'}
                and isinstance(record['path'],str) and Path(record['path']).is_absolute()
                and isinstance(record['sha256'],str) and re.fullmatch(r'[a-f0-9]{64}',record['sha256'])
                and isinstance(record['runtime_sha256'],str) and re.fullmatch(r'[a-f0-9]{64}',record['runtime_sha256'])
                and record['runtime_sha256'] != approved, 'Invalid native approved predecessor image')
    return value


def verified_legacy_entry(binary, install, approved, previous):
    command = previous.get('command')
    require(isinstance(command, str) and Path(command).is_absolute(), 'Legacy command requires an exact native owned image')
    image = Path(command)
    regular(image)
    require(image.is_file(), 'Original image is missing; native ownership review required')
    observed = launcher_observation(binary, install, approved)
    candidates = [observed[role] for role in ('launcher','universal_launcher') if observed[role] is not None]
    candidates += observed.get('legacy_runtime_images',[])
    matching = [record for record in candidates if image.samefile(Path(record['path']))]
    require(matching and len({record['sha256'] for record in matching}) == 1,
            'Original command is not a native-owned stable/universal launcher or approved predecessor runtime image; configuration preserved')
    with image.open('rb') as stream:
        require(hashlib.file_digest(stream, 'sha256').hexdigest() == matching[0]['sha256'], 'Original launcher changed after native inspection')
    args = previous.get('args')
    require(isinstance(args, list) and 1 <= len(args) <= 32 and all(isinstance(item, str) and item and not any(ord(char) < 32 for char in item) for item in args)
            and len(json.dumps(args).encode()) <= 16384 and args[-1] == 'mcp', 'Unsupported original launcher arguments; configuration preserved')
    if args[:2] == ['runtime', 'launch']:
        require(len(args) >= 6 and args[2] == '--install-dir' and args[4] == '--'
                and Path(args[3]).is_absolute() and Path(args[3]).samefile(install), 'Original launcher names a different installation')
        args = args[5:]
    require((len(args)-1) % 2 == 0, 'Unsupported original MCP selector')
    fields = {}
    for flag, argument in zip(args[:-1:2], args[1:-1:2]):
        require(flag in ('--root', '--data-dir', '--collection', '--service-project', '--service-install-dir', '--catalog-session', '--combined-session')
                and flag not in fields, 'Unsupported or duplicate original MCP scope; configuration preserved')
        fields[flag] = argument
    if '--service-install-dir' in fields:
        require(Path(fields['--service-install-dir']).is_absolute() and Path(fields['--service-install-dir']).samefile(install), 'Original MCP installation differs')
    if '--service-project' in fields:
        require(re.fullmatch(r'[a-f0-9]{64}',fields['--service-project']) is not None, 'Invalid original native Project binding')
    sessions = [flag for flag in ('--catalog-session', '--combined-session') if flag in fields]
    require(len(sessions) <= 1 and (not sessions or not {'--root','--data-dir','--collection','--service-project'}.intersection(fields)), 'Mixed original scope requires manual review')
    return fields, observed


def validate_legacy_registration(binary, install, approved, control, inspection, fields, registration, root):
    require(Path(registration['root']).samefile(root), 'Original registered source differs; configuration preserved')
    if '--root' in fields:
        require(Path(fields['--root']).is_absolute() and Path(fields['--root']).samefile(root), 'Original configured source differs')
    for flag, field in (('--data-dir','data_root'), ('--collection','collection'), ('--service-project','binding')):
        if flag in fields:
            require(Path(fields[flag]).samefile(Path(registration[field])) if flag == '--data-dir' else fields[flag] == registration[field], 'Original storage, collection or Project differs')
    if '--service-project' not in fields:
        require('--root' in fields, 'Original root-only selector is missing')
        arguments = ['runtime','source-scope','--install-dir',install,'--expected-control',control,
                     '--expected-runtime',approved,'--source-root',root]
        if '--collection' in fields:
            arguments += ['--index-collection',fields['--collection']]
        observed = native_json(binary, arguments)
        require(observed.get('runtime_sha256') == approved and Path(observed['install_dir']).samefile(install)
                and Path(observed['control_dir']).samefile(control), 'Original native source scope crossed installations')
        scope = observed['scope']
        require(scope.get('schema') == 1 and scope.get('namespace') == inspection['registry_namespace']
                and Path(scope['root']).samefile(root) and scope.get('collection') == fields.get('--collection'), 'Original native source scope differs')
        require(scope.get('outcome') != 'ambiguous', 'ambiguous_root_scope_requires_original_union: preserve the complete original multi-index scope; this helper cannot select one member')
        require(scope.get('outcome') == 'registered' and scope.get('registration') == registration, 'Original source scope no longer resolves the exact supplied registration')


def retire_after_config(binary, install, approved, config, after, requested):
    require(read_config(config) == after, 'Configuration changed before legacy launcher cleanup; cleanup not requested')
    result = None
    try:
        observed = launcher_observation(binary, install, approved)
        remaining = any(observed[role] is not None for role in ('launcher','universal_launcher'))
        if remaining and not requested:
            return {'status':'pending_review','complete':False,'observation':observed,
                    'reason':'This scoped development helper cannot prove other client configurations migrated. Use --retire-obsolete-executables only after checking all known configurations.'}
        if not requested:
            return {'status':'complete','complete':True,'observation':observed,'native_retirement_requested':False}
        require(read_config(config) == after, 'Configuration changed before legacy launcher cleanup; cleanup not requested')
        result = native_json(binary, ['runtime','retire-legacy-launchers','--install-dir',install,'--expected-active',approved])
        require(type(result.get('schema')) is int and result['schema'] == 1 and result.get('active') == approved and result.get('legacy_launchers_retired') is True
                and result.get('launcher') is None and result.get('universal_launcher') is None
                and result.get('processes',{}).get('scope') == 'verified_launcher_images'
                and type(result['processes'].get('remaining')) is int and result['processes']['remaining'] == 0
                and isinstance(result['processes'].get('observations'),list) and len(result['processes']['observations']) <= 2, 'Native launcher retirement outcome is not complete')
        require(read_config(config) == after, 'Configuration changed during native launcher retirement; migration outcome requires review')
        return {'status':'complete','complete':True,'native_retirement_requested':True,'receipt':result}
    except (RuntimeError,ValueError,OSError,KeyError,TypeError,subprocess.SubprocessError) as error:
        return {'status':'blocked_or_unknown','complete':False,'native_retirement_requested':requested,'error':str(error),
                **({'receipt':result} if result is not None else {})}


def retire_superseded_after_config(binary, install, approved, config, after, launcher_cleanup, requested):
    if not launcher_cleanup['complete']:
        return {'status':'blocked_by_launcher_cleanup','complete':False,'native_retirement_requested':False}
    if not requested:
        return {'status':'pending_review','complete':False,'native_retirement_requested':False,
                'reason':'This scoped helper has not requested native retirement of all superseded payloads after reviewing the other client declarations.'}
    result = None
    try:
        require(read_config(config) == after, 'Configuration changed before superseded runtime retirement; cleanup not requested')
        result = native_json(binary,['runtime','retire-previous','--install-dir',install,'--expected-active',approved])
        retired = result.get('retired_runtimes')
        require(result.get('active') == approved and result.get('complete') is True and isinstance(retired,list)
                and all(isinstance(identity,str) and re.fullmatch(r'[a-f0-9]{64}',identity) and identity != approved for identity in retired)
                and len(set(retired)) == len(retired)
                and result.get('selected_service_restarted') is False
                and result.get('processes',{}).get('scope') == 'verified_superseded_runtime_images'
                and type(result['processes'].get('remaining')) is int and result['processes']['remaining'] == 0
                and isinstance(result['processes'].get('observations'),list)
                and len(result['processes']['observations']) <= 20,
                'Native superseded runtime retirement is not complete')
        previous = result.get('retired_previous')
        require(previous is None or previous in retired, 'Native previous-runtime receipt differs from the complete retired roster')
        require(read_config(config) == after, 'Configuration changed during superseded runtime retirement; migration outcome requires review')
        return {'status':'complete','complete':True,'native_retirement_requested':True,'receipt':result}
    except (RuntimeError,ValueError,OSError,KeyError,TypeError,subprocess.SubprocessError) as error:
        return {'status':'blocked_or_unknown','complete':False,'native_retirement_requested':True,'error':str(error),
                **({'receipt':result} if result is not None else {})}


def remove_generated_field(source, table, key):
    """Remove one simple generated scalar/array, retaining its inline comment."""
    headers = list(re.finditer(r'^\['+re.escape(table)+r'\][ \t]*(?:#[^\r\n]*)?\r?$', source, re.M))
    require(len(headers) <= 1, 'Ambiguous AIRK table')
    if not headers:
        return source
    start = source.find('\n', headers[0].start())
    if start < 0:
        return source
    start += 1
    following = re.search(r'^\[', source[start:], re.M)
    end = start + following.start() if following else len(source)
    body = source[start:end]
    pattern = re.compile(r'^[ \t]*'+re.escape(key)+r'[ \t]*=[ \t]*(?:"(?:[^"\\\r\n]|\\.)*"|\x27[^\x27\r\n]*\x27|\[[^\r\n]*\])([ \t]*(?:#[^\r\n]*)?)(\r?\n|$)', re.M)
    matches = list(pattern.finditer(body))
    require(len(matches) <= 1, 'Ambiguous AIRK field')
    if not matches:
        return source
    match = matches[0]
    comment = match[1].lstrip()
    body = body[:match.start()] + (comment+match[2] if comment else '') + body[match.end():]
    return source[:start] + body + source[end:]


def prepare_http_config(before, root, canonical, known_tools=None, recognized_entry=None):
    """Use the native serializer; change only a proven owned server's transport."""
    require('airk1.' not in canonical, 'Native configuration contains credential bytes')
    native = tomllib.loads(canonical)
    require(set(native) == {'mcp_servers'} and set(native['mcp_servers']) == {'airk'}, 'Unsupported native configuration shape')
    target = native['mcp_servers']['airk']
    require(set(target) == {'url', 'bearer_token_env_var', 'http_headers'}
            and re.fullmatch(r'AIRK_MCP_TOKEN_[A-F0-9]{32}', target['bearer_token_env_var'])
            and target['http_headers'] == {'X-AIRK-Response-Mode': 'terse'}, 'Unsupported native Codex HTTP format')
    source = (before or b'').decode('utf-8')
    original = tomllib.loads(source)
    expected = copy.deepcopy(original)
    if recognized_entry is None:
        name, previous = owned_server(before, root)
    else:
        name, previous = recognized_entry
        require(original.get('mcp_servers',{}).get(name) == previous, 'Original reviewed configuration changed; preserved')
    require(previous is not None or name not in original.get('mcp_servers', {}), 'AIRK name belongs to another root')
    previous = previous or {}
    env = previous.get('env', {})
    require(isinstance(env, dict) and set(env) <= {'AIRK_CLIENT', 'AIRK_MCP_RESPONSE_MODE'}, 'Former server environment needs review; preserved')
    mode = env.get('AIRK_MCP_RESPONSE_MODE', previous.get('http_headers', {}).get('X-AIRK-Response-Mode', 'terse'))
    require(mode in ('full', 'terse'), 'Unknown former response mode preserved')
    require(set(previous.get('http_headers', {})) <= {'X-AIRK-Response-Mode'}, 'Custom HTTP headers preserved; review required')
    for field in ('command', 'args'):
        source = remove_generated_field(source, 'mcp_servers.'+name, field)
    for field in env:
        source = remove_generated_field(source, 'mcp_servers.'+name+'.env', field)
    # Empty former env table is harmless TOML, but its resulting semantic entry
    # is explicitly removed below by deleting only that exact empty header.
    source = re.sub(r'^\[mcp_servers\.'+re.escape(name)+r'\.env\][ \t]*(#[^\r\n]*)?(\r?\n|$)', lambda m: (m[1]+m[2]) if m[1] else '', source, flags=re.M)
    for field in ('url', 'bearer_token_env_var'):
        source = replace_scalar(source, 'mcp_servers.'+name, field, target[field])
    source = replace_scalar(source, 'mcp_servers.'+name+'.http_headers', 'X-AIRK-Response-Mode', mode)
    updated = {key: value for key, value in previous.items() if key not in ('command', 'args', 'env', 'http_headers')}
    updated.update(url=target['url'], bearer_token_env_var=target['bearer_token_env_var'], http_headers={'X-AIRK-Response-Mode': mode})
    if known_tools is not None:
        for field in ('enabled_tools', 'disabled_tools'):
            if field not in updated:
                continue
            values = updated[field]
            require(isinstance(values, list) and all(isinstance(value, str) for value in values), 'Invalid existing tool policy preserved')
            normalized = []
            for value in values:
                if value.startswith('airk_'):
                    require(value[5:] in known_tools, 'Unknown former tool alias needs review; preserved')
                    normalized.append(value[5:])
                else:
                    normalized.append(value)
            if normalized != values:
                source = remove_generated_field(source, 'mcp_servers.'+name, field)
                # JSON arrays are valid TOML arrays. The complete semantic check
                # below refuses multiline/custom layouts rather than discarding them.
                marker = '[mcp_servers.'+name+']'
                lines = source.splitlines(keepends=True)
                positions = [index for index, line in enumerate(lines) if re.fullmatch(re.escape(marker)+r'[ \t]*(?:#[^\r\n]*)?(?:\r?\n)?', line)]
                require(len(positions) == 1, 'Unsupported tool policy table')
                newline = '\r\n' if '\r\n' in source else '\n'
                lines.insert(positions[0]+1, field+' = '+json.dumps(normalized)+newline)
                source = ''.join(lines)
                updated[field] = normalized
    expected.setdefault('mcp_servers', {})[name] = updated
    require(tomllib.loads(source) == expected, 'Unsupported TOML layout; original configuration preserved')
    required = {'status', 'search', 'read_file'}
    normalize = lambda names: {value.removeprefix('airk_') for value in names}
    require(not required.intersection(normalize(updated.get('disabled_tools', [])))
            and ('enabled_tools' not in updated or required <= normalize(updated['enabled_tools'])),
            'Existing tool policy excludes verification tools; preserved')
    return source.encode('utf-8'), name, updated


def https_smoke(connection, control, version, root, collection=None):
    """Generic real TLS/MCP verification; this does not execute the Codex client."""
    http = local_module('local_setup_http', 'http_fixture.py')
    client = http.McpClient(connection, control)
    sequence = 0
    def request(method, params):
        nonlocal sequence
        sequence += 1
        client.send({'jsonrpc': '2.0', 'id': sequence, 'method': method, 'params': params})
        reply = client.receive(sequence)
        require('error' not in reply, 'HTTPS MCP protocol refused verification')
        return reply['result']
    def call(name, arguments):
        result = request('tools/call', {'name': name, 'arguments': arguments})
        require(result.get('isError') is not True, 'HTTPS evidence read refused; index/scope remains unchanged')
        text = next((block['text'] for block in result.get('content', []) if block.get('type') == 'text'), None)
        require(isinstance(text, str), 'Missing complete HTTPS evidence result')
        return json.loads(text)
    try:
        initialized = request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                             'clientInfo': {'name': 'airk-local-build-check', 'version': '1'}})
        require(initialized.get('serverInfo', {}).get('version') == version, 'HTTPS service version differs from the selected build')
        client.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
        tools = request('tools/list', {})['tools']
        require({'status', 'search', 'read_file'} <= {tool['name'] for tool in tools}, 'Required HTTPS tools missing')
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
                and hit['citation']['content_hash'] == digest for hit in read['hits']), 'HTTPS README differs from saved local evidence')
        words = re.findall(r'[A-Za-z]{5,}', read['hits'][0]['text'])
        require(words, 'README evidence has no suitable lexical term')
        search = call('search', {'query': words[0], 'mode': 'lexical', 'path_prefix': 'README.md',
                                 'limit': 1, 'max_bytes': 2048, 'verification': 'require', 'response_mode': 'full'})
        require(any(hit.get('citation', {}).get('path') == 'README.md' and hit['citation'].get('content_hash') == digest
                    for hit in search.get('hits', [])), 'HTTPS search did not retain the verified source identity')
        require((root / 'README.md').read_bytes() == source, 'Source changed during HTTPS verification')
        return {'transport': 'https', 'service_version': version, 'collection': status.get('collection'),
                'publication': status['publication'], 'tool_count': len(tools), 'status': 'passed',
                'tool_catalog': tools,
                'read': 'passed', 'search': 'passed', 'source_sha256': digest,
                'codex_client_execution': 'not_run', 'codex_trust_and_environment': 'not_configured_by_probe'}
    finally:
        client.close()


def setup_http(runtime, approved, install, connection_id, root, *, selection_file=None,
               registration_file=None, data_dir=None, report=None, retire_legacy_launchers=False):
    """Opt-in modern mode: no indexing, registration, grants, or global settings."""
    root = root.resolve(strict=True)
    require(root != Path.home().resolve(), 'User-level Codex configuration must not be modified')
    require(re.fullmatch(r'[a-f0-9]{32}', connection_id or '') is not None, 'Explicit stable connection ID required')
    require(not (selection_file and registration_file), 'Choose one reviewed selection or original legacy registration')
    if report is not None:
        report = report.resolve()
        require(report.is_relative_to(root / 'artifacts' / 'local'), 'Setup evidence must remain under checkout artifacts/local')
        regular(report)
    binary, version, execution = verify_modern_runtime(runtime, approved)
    require(install.is_absolute(), 'Explicit absolute managed installation required')
    install = install.resolve(strict=True)
    config = root / '.codex' / 'config.toml'
    before = read_config(config)
    # This locates a candidate table only. Native lineage and immutable scope
    # checks below authorize migration before any configuration is changed.
    name, previous = owned_server(before, root, candidate_only=True)
    inspection = native_json(binary, ['runtime', 'inspect', '--install-dir', install])
    require(inspection.get('payload_verified') is True and inspection.get('approval', {}).get('manifest_sha256') == approved,
            'Installed service differs from the explicitly approved build')
    installed_binary = Path(inspection['executable'])
    regular(installed_binary)
    require(installed_binary.resolve().is_relative_to(install / 'runtimes' / approved)
            and hashlib.sha256(installed_binary.read_bytes()).hexdigest() == execution, 'Installed main image differs')
    control = Path(inspection['control_dir'])
    require(control.is_absolute() and control.resolve(strict=True) == control and not control.is_symlink(), 'Redirected service control directory refused')
    http = local_module('local_setup_http_metadata', 'http_fixture.py')
    selected = None
    if selection_file:
        selected = json.loads(http.private_bytes(selection_file, selection_file.parent, 8192))
        require(set(selected) == {'kind', 'selection'} and selected['kind'] in ('scoped', 'combined'), 'Use native original-receipt migration for imported legacy authority')
    old_connection = None
    lineage = None
    if previous and 'url' in previous:
        require(data_dir is None, 'Existing HTTP storage is native-owned; do not supply a replacement data directory')
        match = re.fullmatch(r'AIRK_MCP_TOKEN_([A-F0-9]{32})', previous.get('bearer_token_env_var', ''))
        require(match is not None and match[1].lower() == connection_id, 'Existing HTTP credential identity differs; preserved')
        old_connection = native_json(binary, ['runtime', 'http-connection-info', '--install-dir', install, '--connection-id', connection_id])
        require(previous['url'] == old_connection['endpoint']['url'], 'Existing URL is not the authenticated native connection')
        require(selected is None or immutable_selection(old_connection['selection']) == immutable_selection(selected), 'Reviewed selection changes existing authority; preserved')
        require(registration_file is None, 'An existing HTTP connection must not be reimported')
    elif previous:
        fields, lineage = verified_legacy_entry(binary, install, approved, previous)
        session = next((flag for flag in ('--catalog-session','--combined-session') if flag in fields),None)
        if session:
            kind = 'scoped' if session == '--catalog-session' else 'combined'
            require(selected is not None and selected['kind'] == kind and registration_file is None and data_dir is None,
                    'Original frozen catalog/combined scope requires its complete matching selection file')
            require(immutable_selection(selected['selection']) == immutable_selection(json.loads(fields[session])),
                    'Original frozen catalog or multi-index authority differs; configuration preserved')
            old_connection = native_json(binary, ['runtime','http-connection','--install-dir',install,
                '--selection-file',selection_file,'--connection-id',connection_id,'--client','codex'])
        else:
            require(registration_file is not None and selected is None, 'Original legacy registration and configuration receipt required')
            registration = json.loads(http.private_bytes(registration_file, registration_file.parent, 8192))
            validate_legacy_registration(binary,install,approved,control,inspection,fields,registration,root)
            require(data_dir is None or data_dir.samefile(Path(registration['data_root'])), 'Requested storage differs; preserved')
            old_connection = native_json(binary, ['runtime', 'http-legacy-connection', '--install-dir', install,
                '--registration-file', registration_file, '--configuration-receipt', hashlib.sha256(before).hexdigest(),
                '--connection-id', connection_id, '--client', 'codex'])
            require(old_connection.get('status') != 'review_required', 'Native original legacy authority requires review; configuration preserved')
            require(old_connection.get('selection', {}).get('kind') == 'imported_legacy'
                    and old_connection['selection']['selection']['registration'] == registration
                    and old_connection['selection']['selection']['configuration_receipt'] == hashlib.sha256(before).hexdigest(),
                    'Native migration changed the original configuration binding')
    else:
        require(selected is not None and registration_file is None and data_dir is None,
                'New HTTP configuration requires an explicit native reviewed selection, not guessed storage')
        old_connection = native_json(binary, ['runtime', 'http-connection', '--install-dir', install,
            '--selection-file', selection_file, '--connection-id', connection_id, '--client', 'codex'])
    require(read_config(config) == before, 'Configuration changed during enrollment; preserved')
    bundle = native_json(binary, ['--service-install-dir', install, 'setup', '--client', 'codex', '--existing-connection-id', connection_id])
    connection = bundle['connection']
    require(connection['connection_id'] == connection_id and old_connection['connection_id'] == connection_id
            and immutable_selection(connection['selection']) == immutable_selection(old_connection['selection'])
            and (selected is None or immutable_selection(connection['selection']) == immutable_selection(selected)), 'Native connection authority changed')
    require(connection['endpoint']['executable_sha256'] == execution
            and connection['endpoint']['namespace'] == inspection['registry_namespace'], 'Native endpoint crossed the approved installation')
    require(Path(connection['ca_certificate_file']) == control / 'http-1' / 'ca.pem'
            and Path(connection['credential_file']) == control / 'http-1' / ('credential-'+connection_id+'.token'), 'Native private references crossed the control root')
    environment = bundle['environment']
    require(environment['token_source_file'] == connection['credential_file']
            and environment['additional_ca_file'] == connection['ca_certificate_file']
            and environment['token_variable'] == 'AIRK_MCP_TOKEN_'+connection_id.upper()
            and environment['ca_variable'] == 'CODEX_CA_CERTIFICATE' and environment['preserve_existing_trust'] is True,
            'Native client environment references differ')
    recognized = (name,previous) if previous is not None else None
    after, name, server = prepare_http_config(before, root, bundle['configuration'],recognized_entry=recognized)
    require(server['url'] == connection['endpoint']['url'] and server['bearer_token_env_var'] == environment['token_variable'], 'Canonical setup configuration crossed its connection')
    registration = connection['selection']['selection'].get('registration')
    collection = registration['collection'] if registration else None
    verification = https_smoke(connection, control, version, root, collection)
    after, name, server = prepare_http_config(before, root, bundle['configuration'],
        known_tools={tool['name'] for tool in verification['tool_catalog']},recognized_entry=recognized)
    if before != after:
        write_config(config, before, after)
    require(read_config(config) == after, 'Configuration changed during verification; no success receipt written')
    cleanup = retire_after_config(binary,install,approved,config,after,retire_legacy_launchers)
    superseded = retire_superseded_after_config(binary,install,approved,config,after,cleanup,retire_legacy_launchers)
    receipt = {'schema': 'airk-local-agent-setup/2', 'client': 'codex', 'config': str(config),
               'selected_runtime_sha256': approved, 'connection': connection, 'environment': environment,
               'configuration_preimage_sha256': hashlib.sha256(before or b'').hexdigest(),
               'indexed': False, 'verification': verification, 'client_environment_loaded': False,
               'status':'configuration_saved' if cleanup['complete'] and superseded['complete'] else 'configuration_saved_cleanup_pending',
               'legacy_launcher_lineage':lineage,'legacy_launcher_cleanup':cleanup,'superseded_runtime_cleanup':superseded,
               'limitations': ['Generic HTTPS smoke passed; the actual Codex client was not executed.',
                               'Load protected token and additive CA references in the actual client host before reconnecting.',
                               'Native existing-connection rendering may ensure selected service readiness; it does not reenroll or issue a grant.']}
    if report is not None:
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps(receipt, indent=2)+'\n', encoding='utf-8')
    return receipt


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
        name = 'airk'
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
    required = {'status', 'search', 'read_file'}
    normalize = lambda names: {name.removeprefix('airk_') for name in names}
    require(not required.intersection(normalize(server.get('disabled_tools', [])))
            and ('enabled_tools' not in server or required.issubset(normalize(server['enabled_tools']))),
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
    print('Installing the embedded local vector model offline...', flush=True)
    run_setup_step('Native bundled model installation', [*command, 'collections', 'vectors', 'install-bundled'],
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
        name = name.removeprefix('airk_') if name.removeprefix('airk_') in names else name
        result = request(number, 'tools/call', {'name': name, 'arguments': arguments})
        require(not result.get('isError'), f'MCP {name} failed; check helper/index compatibility')
        return json.loads(result['content'][0]['text'])

    try:
        init = request(1, 'initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                        'clientInfo': {'name': 'airk-local-build-check', 'version': '1'}})
        require(init.get('serverInfo', {}).get('version') == version, 'MCP helper version differs from the installed candidate')
        send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
        names = {tool['name'] for tool in request(2, 'tools/list', {})['tools']}
        require({'status', 'search', 'read_file'} <= {name.removeprefix('airk_') for name in names}, 'Required MCP tools are missing')
        status = call(3, 'airk_status', {})
        require(status.get('index_state') == 'available' and status.get('collection') == scope['--collection']
                and type(status.get('files')) is int and status['files'] > 0,
                'MCP did not provide the expected publication snapshot; refresh status before verifying indexed evidence')
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
    require(tuple(map(int, version.split('.'))) < (1, 8, 14),
            'Modern builds require explicit --runtime-dir/--approved-sha256/--install-dir HTTP setup')
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
              'extraction_workers_requested',
              'extraction_worker_limit',
              'extraction_workers_spawned',
              'extraction_parallel_jobs',
              'extraction_serial_jobs',
              'extraction_work_us',
              'extraction_wait_us',
              'extraction_source_peak_bytes',
              'extraction_coordinator_source_peak_bytes',
              'extraction_result_bound_bytes',
              'embedding_cache_hits', 'embedding_cache_misses', 'embedding_inferences', 'embedding_no_vector',
              'embedding_inputs', 'embedding_blank_inputs', 'embedding_duplicate_inputs', 'embedding_actual_forwards',
              'embedding_tokenized_inputs', 'embedding_input_tokens', 'embedding_embedded_tokens',
              'embedding_padding_tokens', 'embedding_truncated_inputs', 'embedding_plan_compilations',
              'embedding_plan_compile_us', 'embedding_plan_cache_peak_bytes',
              'vector_eligible_artifacts', 'vector_excluded_artifacts', 'vector_reused_inputs', 'vector_graph_nodes',
              'vector_registry_rows', 'vector_registry_bytes',
              'embedding_cache_file_opens',
              'embedding_cache_read_bytes',
              'embedding_cache_written_bytes',
              'embedding_cache_record_verification_bytes',
              'embedding_cache_index_verification_bytes',
              'embedding_cache_full_pack_verification_bytes',
              'embedding_cache_syncs',
              'embedding_cache_index_hits',
              'embedding_cache_index_peak_bytes',
              'embedding_cache_packs_written',
              'embedding_cache_packs_reused',
              'embedding_cache_compacted_records',
              'embedding_cache_finalize_us',
              'vector_units', 'vector_body_bytes', 'vector_planning_us',
              'embedding_model_load_us', 'embedding_cache_lookup_us', 'embedding_cache_store_us',
              'embedding_inference_us', 'vector_graph_insert_us', 'embedding_workers', 'embedding_batches')
    measured = {name: report[name] for name in fields if name in report}
    require(all(type(value) is int and value >= 0 for value in measured.values()), 'Invalid indexing measurement')
    return {'wall_ms': round(elapsed_seconds * 1000, 3), **measured}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--build-record', type=Path, help='Historical pre-1.8.14 standalone setup only')
    mode.add_argument('--runtime-dir', type=Path, help='Explicit verified modern single-image distribution')
    parser.add_argument('--approved-sha256', help='Externally approved exact airk-install.json SHA-256')
    parser.add_argument('--install-dir', type=Path, help='Already owned managed installation of that exact distribution')
    parser.add_argument('--connection-id', help='Stable native connection ID; reuse the same ID on uncertain setup outcomes')
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument('--selection-file', type=Path, help='Owner-private native-reviewed scoped/combined HttpSelection JSON')
    selection.add_argument('--registration-file', type=Path, help='Owner-private original imported legacy registration JSON')
    parser.add_argument('--report', type=Path, help='Optional protected-reference evidence under checkout artifacts/local')
    parser.add_argument('--retire-obsolete-executables','--retire-legacy-launchers',dest='retire_legacy_launchers', action='store_true',
                        help='Scoped development helper only: after all known client configs migrated, request native launcher then superseded-runtime retirement')
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--data-dir', type=Path, help='Required for first setup; must match existing storage when provided')
    parser.add_argument('--new-data-dir', type=Path, help='Fallback storage for first setup only; existing bindings win')
    parser.add_argument('--skip-index', action='store_true', help='Verify an already current index; stale evidence fails')
    args = parser.parse_args()
    try:
        if args.runtime_dir:
            require(args.approved_sha256 and args.install_dir and args.connection_id, 'Modern mode requires explicit runtime approval, installation and stable connection ID')
            require(args.new_data_dir is None, 'Modern mode preserves native storage; --new-data-dir is historical only')
            receipt = setup_http(args.runtime_dir, args.approved_sha256, args.install_dir, args.connection_id,
                                 args.root, selection_file=args.selection_file, registration_file=args.registration_file,
                                 data_dir=args.data_dir, report=args.report, retire_legacy_launchers=args.retire_legacy_launchers)
            print(json.dumps(receipt, indent=2))
            if not receipt['legacy_launcher_cleanup']['complete'] or not receipt.get('superseded_runtime_cleanup',{}).get('complete'):
                parser.exit(1, 'HTTPS configuration saved; obsolete executable cleanup remains pending or blocked. See the retained cleanup results.\n')
        else:
            require(not any((args.approved_sha256, args.install_dir, args.connection_id, args.selection_file, args.registration_file, args.report, args.retire_legacy_launchers)),
                    'HTTP options require --runtime-dir')
            setup(args.build_record, args.root, args.data_dir, args.skip_index, args.new_data_dir)
    except (RuntimeError, ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, f'Local agent setup failed: {error}\n')


if __name__ == '__main__':
    main()
