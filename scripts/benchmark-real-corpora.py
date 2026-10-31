# SPDX-License-Identifier: Apache-2.0
"""Read-only real-corpus campaigns against an approved, isolated AIRK runtime."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
import platform
from pathlib import Path, PurePosixPath
import queue
import re
import subprocess
import tempfile
import threading
import time
import uuid


def repository():
    for parent in Path(__file__).resolve().parents:
        if (parent / 'scripts/check-service-clients.py').is_file():
            return parent
    raise RuntimeError('repository_missing')


def clients():
    spec = importlib.util.spec_from_file_location('airk_corpus_clients', repository() / 'scripts/check-service-clients.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class CampaignError(Exception):
    """Only fixed local codes may reach the public report."""


def require(condition, code):
    if not condition:
        raise CampaignError(code)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def inside(path, root):
    return path == root or root in path.parents


def physical(path, exists=True):
    path = Path(os.path.abspath(path))
    for part in (path, *path.parents):
        try:
            info = part.lstat()
        except FileNotFoundError:
            continue
        require(not part.is_symlink() and not getattr(info, 'st_file_attributes', 0) & 0x400,
                'redirected_path')
    return path.resolve(strict=exists)


def isolate(roots, targets):
    roots = [physical(root) for root in roots]
    require(len(roots) == 2 and all(root.is_dir() for root in roots), 'two_corpora_required')
    require(not inside(roots[0], roots[1]) and not inside(roots[1], roots[0]), 'overlapping_corpora')
    for target in targets:
        resolved = physical(target, exists=False)
        require(not any(inside(resolved, root) or inside(root, resolved) for root in roots), 'state_source_overlap')
    return roots


def git(root, *args):
    result = subprocess.run(['git', '--no-optional-locks', '-c', f'safe.directory={root.as_posix()}', '-C', str(root), *args],
                            capture_output=True, check=False)
    require(result.returncode == 0, 'source_identity_failed')
    return result.stdout


def source_identity(root, max_files=200000, max_bytes=100 << 30):
    """No source writes; Git ignores apply, AIRK may exclude additional inputs.

    This deliberately conservative superset invalidates comparisons on any change.
    Names and dirty records feed an opaque hash and are never returned.
    """
    head = git(root, 'rev-parse', 'HEAD').strip().decode('ascii')
    status = git(root, 'status', '--porcelain=v2', '-z', '--untracked-files=all')
    names = sorted(set(git(root, 'ls-files', '-z', '--cached', '--others', '--exclude-standard').split(b'\0')) - {b''})
    require(len(names) <= max_files, 'source_identity_file_budget')
    value = hashlib.sha256(head.encode() + b'\0' + status)
    total = 0
    for name in names:
        relative = os.fsdecode(name)
        parts = Path(relative).parts
        require(not Path(relative).is_absolute() and '..' not in parts, 'source_identity_path')
        path = root / relative
        value.update(name + b'\0')
        if not path.exists():
            value.update(b'missing\0')
            continue
        path = physical(path)
        require(inside(path, root) and path.is_file(), 'source_identity_nonfile')
        before = path.stat()
        total += before.st_size
        require(total <= max_bytes, 'source_identity_byte_budget')
        value.update(bytes.fromhex(digest(path)))
        after = path.stat()
        require((before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns), 'source_changed')
    require(status == git(root, 'status', '--porcelain=v2', '-z', '--untracked-files=all'), 'source_changed')
    return {'commit': head, 'identity_sha256': value.hexdigest(), 'files': len(names), 'bytes': total, 'dirty': bool(status)}


def unchanged(root, expected):
    require(source_identity(root) == expected, 'source_changed')


def verify_runtime(runtime, approved):
    """Approved digest must come from the reviewed lightweight builder run.

    Sealing alone cannot prove Cargo feature selection. The approval is external.
    """
    runtime = physical(runtime)
    require(re.fullmatch('[0-9a-f]{64}', approved) is not None, 'runtime_approval_invalid')
    require(digest(runtime / 'airk-install.json') == approved, 'runtime_approval_mismatch')
    install = json.loads((runtime / 'airk-install.json').read_bytes())
    pair = json.loads((runtime / 'airk-runtime.json').read_bytes())
    build = json.loads((runtime / 'airk-build.json').read_bytes())
    require(install.get('schema') == pair.get('schema') == build.get('schema') == 1, 'runtime_schema')
    require(build.get('runtime') == pair and install.get('version') == pair.get('version'), 'runtime_receipt_mismatch')
    require(build.get('profile') == 'release', 'release_runtime_required')
    files = install.get('files', {})
    require(4 <= len(files) <= 2048, 'runtime_inventory_invalid')
    for name, expected in files.items():
        relative = PurePosixPath(name)
        require(not relative.is_absolute() and '..' not in relative.parts and '\\' not in name, 'runtime_inventory_path')
        path = physical(runtime / name)
        require(inside(path, runtime) and path.is_file() and digest(path) == expected, 'runtime_inventory_mismatch')
    actual = {p.relative_to(runtime).as_posix() for p in runtime.rglob('*') if p.is_file()}
    require(actual == set(files) | {'airk-install.json'}, 'runtime_inventory_extra')
    stem = 'AIRK.app/Contents/MacOS/' if pair['os'] == 'macos' else ''
    suffix = '.exe' if pair['os'] == 'windows' else ''
    for name, key in [('airk', 'airk_sha256'), ('airk-embed', 'embed_sha256')]:
        binary = runtime / f'{stem}{name}{suffix}'
        require(digest(binary) == pair.get(key), 'runtime_pair_mismatch')
        observed = subprocess.run([str(binary), '--version'], capture_output=True, timeout=30)
        require(observed.returncode == 0 and observed.stdout.decode().strip() == f'{name} {pair["version"]}', 'compiled_version_mismatch')
    return {'version': pair['version'], 'install_sha256': approved, 'main_sha256': pair['airk_sha256'],
            'helper_sha256': pair['embed_sha256'], 'profile': build['profile'],
            'lightweight_basis': 'externally_approved_no_default_features_builder', 'build_receipt_sha256': digest(runtime / 'airk-build.json')}


CODES = {'status_timeout', 'direct_query_admission', 'direct_query_setup', 'publication_busy',
         'index_status_unavailable', 'index_worker_unavailable', 'vector_search_unavailable', 'native_tool_error'}
STATES = {'available', 'unavailable', 'unknown'}
SEMANTIC = {'ready', 'partial', 'pending', 'failed', 'disabled'}
READ_STATES = {'history_unavailable', 'content', 'not_modified', 'move_candidate', 'removed_from_index', 'ambiguous', 'unknown'}


def tool_value(envelope):
    require(isinstance(envelope, dict) and isinstance(envelope.get('content'), list), 'invalid_tool_envelope')
    content = envelope['content']
    require(len(content) == 1 and isinstance(content[0], dict) and content[0].get('type') == 'text'
            and isinstance(content[0].get('text'), str), 'invalid_tool_envelope')
    try:
        value = json.loads(content[0]['text'])
    except ValueError:
        # Native CallToolResult::error intentionally permits plain text. This is
        # a complete failed tool result, not an uncertain transport exchange.
        require(envelope.get('isError') is True, 'invalid_tool_envelope')
        return {'error': {'code': 'native_tool_error'}}
    if not isinstance(value, dict):
        require(envelope.get('isError') is True, 'invalid_tool_envelope')
        return {'error': {'code': 'native_tool_error'}}
    return value


def validate_publication(publication, expected, require_collection=False):
    require(isinstance(publication, dict), 'invalid_publication')
    token = publication.get('token')
    match = re.fullmatch(r'airk-r1:([0-9a-f]{32}):(0|[1-9][0-9]*):([0-9a-f]{64})', token) if isinstance(token, str) else None
    require(match is not None and int(match[2]) <= (1 << 64) - 1, 'invalid_publication')
    if 'collection' in publication or require_collection:
        require(publication.get('collection') == expected['collection'], 'crossed_publication_scope')
    for key, wanted in [('incarnation', match[1]), ('generation', int(match[2])), ('revision', match[3])]:
        if key in publication:
            require(publication[key] == wanted, 'inconsistent_publication')
    return token


def validate_scope(value, expected, name, failed):
    for key, wanted in [('collection', expected['collection']), ('project', expected['binding'])]:
        if key in value:
            require(value[key] == wanted, 'crossed_response_scope')
    publication = value.get('publication')
    token = validate_publication(publication, expected) if publication is not None else None
    conditional = value.get('read') if name == 'read_file' else None
    if conditional is not None:
        require(isinstance(conditional, dict) and conditional.get('status') in READ_STATES, 'invalid_conditional_read')
        nested = conditional.get('publication')
        current = conditional.get('current_publication')
        require(nested is not None or current is not None, 'missing_publication_scope')
        nested_token = validate_publication(nested, expected, True) if nested is not None else None
        current_token = validate_publication(current, expected, True) if current is not None else None
        # The benchmark requests only current evidence, with no historical revision.
        require(conditional.get('historical') is False, 'unexpected_historical_read')
        require(nested_token is None or current_token is None or nested_token == current_token, 'inconsistent_publication')
        if token is not None:
            require(token == nested_token, 'inconsistent_publication')
        if conditional['status'] != 'content':
            # Typed no-content outcomes are scoped observations, not usable excerpts.
            require('hits' not in value, 'unexpected_conditional_payload')
            return
    success = not failed and (name not in ('status', 'diagnostics') or value.get('index_state') == 'available')
    if success:
        require(value.get('collection') == expected['collection'] and publication is not None, 'missing_response_scope')
        if name in ('status', 'diagnostics'):
            require(value.get('project') == expected['binding'], 'missing_response_scope')
        else:
            require(publication.get('collection') == expected['collection'], 'missing_publication_scope')
    elif name in ('status', 'diagnostics') and not failed:
        require(value.get('project') == expected['binding'] and publication is None, 'invalid_unavailable_scope')


def safe_observation(envelope, expected=None, name=None):
    """Validate identity before reducing to a fixed, content-free schema."""
    value = tool_value(envelope)
    if expected is not None:
        validate_scope(value, expected, name, envelope.get('isError') is True)
    error = value.get('index_error') or value.get('error') or {}
    code = error.get('code') if isinstance(error, dict) else None
    if code is None and value.get('status') == 'retry':
        code = value.get('code')
    semantic = value.get('semantic_readiness') or {}
    if not isinstance(semantic, dict):
        semantic = {}
    counts = {key: value.get(key) if type(value.get(key)) is int and value[key] >= 0 else None
              for key in ('files', 'chunks', 'indexed_vectors')}
    return {'index_state': value.get('index_state') if value.get('index_state') in STATES else 'other',
            'code': code if code in CODES else ('other' if code else None),
            'is_error': envelope.get('isError') is True,
            'semantic_state': semantic.get('state') if semantic.get('state') in SEMANTIC else 'other',
            'selected_artifacts': semantic.get('selected_artifacts') if type(semantic.get('selected_artifacts')) is int else None,
            'completed_artifacts': semantic.get('completed_artifacts') if type(semantic.get('completed_artifacts')) is int else None,
            'read_status': value.get('read', {}).get('status') if isinstance(value.get('read'), dict) and value['read'].get('status') in READ_STATES else None,
            'hits': len(value['hits']) if isinstance(value.get('hits'), list) else None,
            'items': len(value['paths']) if isinstance(value.get('paths'), list) else None, **counts}


def failure_code(error):
    if isinstance(error, CampaignError):
        return str(error)
    if isinstance(error, (TimeoutError, subprocess.TimeoutExpired, queue.Empty)):
        return 'transport_timeout'
    if isinstance(error, (BrokenPipeError, OSError)):
        return 'transport_io_failed'
    return 'campaign_failed'


class ObservationFailure(CampaignError):
    def __init__(self, code, observation):
        super().__init__(code)
        self.observation = observation


class RequestNotSubmitted(CampaignError):
    """The campaign budget expired before an MCP request was sent."""


def remaining(deadline, ceiling):
    budget = min(ceiling, deadline - time.monotonic())
    require(budget > 0, 'campaign_deadline')
    return budget


class Session:
    def __init__(self, fixture, project, deadline=None):
        self.deadline = deadline if deadline is not None else time.monotonic() + fixture.command_timeout
        self.fixture, self.project, self.counter = fixture, project, 0
        self.responses = queue.Queue()
        self.child = None

    def __enter__(self):
        f = self.fixture
        self.started = time.perf_counter()
        self.child = subprocess.Popen([str(f.binary), *f.arguments(self.project), 'mcp'], cwd=f.runtime,
                                      env=f.environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.DEVNULL, text=True, encoding='utf-8',
                                      creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
        def pump():
            try:
                for line in self.child.stdout:
                    self.responses.put(json.loads(line))
            except Exception:
                self.responses.put(None)
            finally:
                self.responses.put(None)
        threading.Thread(target=pump, daemon=True).start()
        try:
            response = self.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                                     'clientInfo': {'name': 'airk-corpus-benchmark', 'version': '1'}})
            require(isinstance(response.get('serverInfo'), dict), 'initialize_invalid')
            self.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
            self.initialize_ms = round((time.perf_counter() - self.started) * 1000, 3)
            return self
        except BaseException as error:
            self.__exit__(type(error), None, None)
            raise

    def send(self, message):
        self.child.stdin.write(json.dumps(message) + '\n')
        self.child.stdin.flush()

    def request(self, method, params):
        deadline = min(self.deadline, time.monotonic() + self.fixture.command_timeout)
        try:
            remaining(deadline, self.fixture.command_timeout)
        except CampaignError:
            raise RequestNotSubmitted('campaign_deadline') from None
        self.counter += 1
        identifier = self.counter
        self.send({'jsonrpc': '2.0', 'id': identifier, 'method': method, 'params': params})
        while time.monotonic() < deadline:
            try:
                value = self.responses.get(timeout=max(.001, deadline - time.monotonic()))
            except queue.Empty:
                raise CampaignError('mcp_response_timeout') from None
            require(isinstance(value, dict), 'mcp_closed')
            if value.get('id') == identifier:
                require('error' not in value and isinstance(value.get('result'), dict), 'mcp_protocol_error')
                return value['result']
        raise CampaignError('mcp_response_timeout')

    def tool(self, name, arguments=None):
        before = time.perf_counter()
        try:
            envelope = self.request('tools/call', {'name': name, 'arguments': arguments or {'response_mode': 'full'}})
            observed = safe_observation(envelope, self.fixture.projects[self.project], name)
            return envelope, {'elapsed_ms': round((time.perf_counter() - before) * 1000, 3), **observed}
        except Exception as error:
            code = failure_code(error)
            failure = ({'budget_exhausted': True, 'submission': 'not_submitted'}
                       if isinstance(error, RequestNotSubmitted) else {'transport_or_protocol_failure': True})
            raise ObservationFailure(code, {'elapsed_ms': round((time.perf_counter() - before) * 1000, 3),
                'is_error': True, 'index_state': 'unknown', 'code': code, **failure}) from None

    def __exit__(self, exc_type, *_):
        if self.child:
            cleanup_failed = False
            try:
                try:
                    self.child.stdin.close()
                except (OSError, ValueError):
                    cleanup_failed = True
                try:
                    self.child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    cleanup_failed = True
            finally:
                try:
                    if self.child.poll() is None:
                        self.child.kill()
                        self.child.wait(timeout=5)
                except (OSError, subprocess.TimeoutExpired):
                    cleanup_failed = True
                try:
                    self.child.stdout.close()
                except OSError:
                    cleanup_failed = True
            if exc_type is None:
                require(not cleanup_failed and self.child.returncode == 0, 'mcp_cleanup_failed')


def close_owned_fixture(fixture, primary=None, detail=None):
    """Always reap our child, even when the graceful shutdown command fails."""
    failed = False
    stages = []
    reap = 'not_required'
    try:
        if fixture.child is not None and fixture.child.poll() is None:
            try:
                fixture.run('service', 'shutdown', '--runtime-dir', fixture.control, check=False, timeout=5)
            except Exception:
                failed = True
                stages.append('shutdown_request')
            try:
                fixture.child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                failed = True
                stages.append('graceful_wait')
    finally:
        try:
            if fixture.child is not None and fixture.child.poll() is None:
                reap = 'attempted'
                fixture.child.kill()
                fixture.child.wait(timeout=5)
                reap = 'succeeded'
        except (OSError, subprocess.TimeoutExpired):
            failed = True
            stages.append('owned_reap')
            reap = 'failed'
        finally:
            try:
                fixture.log.close()
            except OSError:
                failed = True
                stages.append('log_close')
            if detail is not None:
                detail.update({'failed_stages': stages, 'owned_reap': reap})
    if primary is None:
        require(not failed, 'service_cleanup_failed')
    return failed


def probe_identity(ordinal, name, arguments):
    mode = arguments.get('mode') if name == 'search' else None
    require(name in ('list_files', 'search', 'read_file') and (name != 'search' or mode in ('lexical', 'vector', 'hybrid')), 'probe_kind_invalid')
    query = arguments.get('query')
    return {'ordinal': ordinal, 'tool': name, 'kind': mode if name == 'search' else ('inventory' if name == 'list_files' else 'excerpt'),
            'mode': mode, 'query_sha256': hashlib.sha256(query.encode('utf-8')).hexdigest() if isinstance(query, str) else None}


def environment_facts():
    return {'os': platform.system(), 'os_release': platform.release(), 'machine': platform.machine(),
            'logical_cpus': os.cpu_count(), 'python': platform.python_version(),
            'storage_class': 'unmeasured', 'cache_eviction': 'not_performed',
            'antivirus_activity': 'unmeasured', 'effective_affinity': 'unmeasured'}


def timed_preparation(output, key, operation):
    started = time.perf_counter()
    try:
        return operation()
    finally:
        output[key] = round((time.perf_counter() - started) * 1000, 3)


def sample(fixture, project, probes, session_type=Session, deadline=None, output=None):
    started = time.perf_counter()
    deadline = deadline if deadline is not None else time.monotonic() + fixture.command_timeout
    results = output if output is not None else {}
    results.update({'initialize_ms': None, 'first_status': None, 'reads': [], 'aborted': False})
    try:
        with session_type(fixture, project, deadline=deadline) as session:
            results['initialize_ms'] = session.initialize_ms
            # No retry/diagnostics/list call may precede or replace this observation.
            try:
                _, first = session.tool('status')
            except ObservationFailure as error:
                results['first_status'] = error.observation
                raise
            first['completed_ms'] = round((time.perf_counter() - started) * 1000, 3)
            results['first_status'] = first
            for ordinal, (name, arguments) in enumerate(probes):
                identity = probe_identity(ordinal, name, arguments)
                try:
                    _, observed = session.tool(name, arguments)
                except ObservationFailure as error:
                    results['reads'].append({**identity, **error.observation})
                    raise
                results['reads'].append({**identity, 'completed_ms': round((time.perf_counter() - started) * 1000, 3), **observed})
            _, results['warm_status'] = session.tool('status')
    except Exception as error:
        results.update({'aborted': True, 'code': failure_code(error),
                        'elapsed_ms': round((time.perf_counter() - started) * 1000, 3)})
        if isinstance(error, RequestNotSubmitted) or isinstance(error, ObservationFailure) and error.observation.get('budget_exhausted'):
            results.update({'budget_exhausted': True, 'submission': 'not_submitted'})
    return results


def completed_sample(*args, **kwargs):
    result = sample(*args, **kwargs)
    require(not result['aborted'], result.get('code', 'sample_aborted'))
    return result


def readiness(fixture, project, deadline):
    while time.monotonic() < deadline:
        with Session(fixture, project, deadline=deadline) as session:
            envelope, observation = session.tool('status')
            if observation['index_state'] == 'available':
                return envelope, observation
        time.sleep(.1)
    raise CampaignError('publication_deadline')


def terminal_job(fixture, receipt, deadline):
    job = fixture.rpc({'job': {'id': receipt['job']['id']}}, deadline=deadline)['job']
    require(job['id'] == receipt['job']['id'] and job['project'] == receipt['job']['project'], 'crossed_job_receipt')
    return job


PROBE_OUTCOMES = ('usable_nonempty', 'success_empty', 'typed_no_content', 'native_error',
                  'transport_protocol_failure', 'budget_exhausted', 'other_unusable')


def probe_outcome(observation):
    if observation.get('budget_exhausted'):
        return 'budget_exhausted'
    if observation.get('transport_or_protocol_failure'):
        return 'transport_protocol_failure'
    if observation.get('is_error'):
        return 'native_error'
    if observation.get('read_status') not in (None, 'content'):
        return 'typed_no_content'
    count = observation.get('hits') if observation.get('tool') in ('search', 'read_file') else observation.get('items')
    return 'usable_nonempty' if type(count) is int and count > 0 else ('success_empty' if count == 0 else 'other_unusable')


class ProbeCounters:
    """Bounded per-probe counters and first/last events; never a polling log."""
    def __init__(self, probes, expected=1):
        self.report = {'expected_observations_per_probe': expected, 'probes': [
            {**probe_identity(n, name, arguments), 'observed': 0, 'not_observed': expected,
             **{outcome: 0 for outcome in PROBE_OUTCOMES}}
            for n, (name, arguments) in enumerate(probes)]}
        self.refresh()

    def observe(self, ordinal, observation, elapsed_ms=None):
        row = self.report['probes'][ordinal]
        event = {**observation}
        event.update({key: row[key] for key in ('ordinal', 'tool', 'kind', 'mode', 'query_sha256')})
        outcome = probe_outcome(event)
        row['observed'] += 1
        row[outcome] += 1
        row['not_observed'] = max(0, self.report['expected_observations_per_probe'] - row['observed'])
        row.setdefault('first_observation', event)
        row['last_observation'] = event
        if outcome in ('native_error', 'transport_protocol_failure', 'budget_exhausted'):
            row.setdefault('first_error', event)
        if outcome == 'usable_nonempty' and 'first_usable_ms' not in row:
            row['first_usable_ms'] = elapsed_ms
            row['first_usable_observation'] = event
        self.refresh()

    def refresh(self):
        rows = self.report['probes']
        self.report['all_probes_eventually_usable'] = all(row['usable_nonempty'] > 0 for row in rows)
        if any(row['transport_protocol_failure'] for row in rows):
            outcome = 'transport_protocol_failures_observed'
        elif any(row['native_error'] for row in rows):
            outcome = 'native_errors_observed'
        elif any(row['budget_exhausted'] for row in rows):
            outcome = 'budget_exhausted'
        elif any(row['not_observed'] for row in rows):
            outcome = 'incomplete_observations'
        elif any(row['success_empty'] or row['typed_no_content'] or row['other_unusable'] for row in rows):
            outcome = 'nonusable_observations'
        else:
            outcome = 'all_requested_observations_usable'
        self.report['observation_outcome'] = outcome

    def sample(self, sample, offset_ms=0):
        for observation in sample.get('reads', []):
            ordinal = observation['ordinal']
            elapsed = offset_ms + observation.get('completed_ms', observation.get('elapsed_ms', 0))
            self.observe(ordinal, observation, elapsed)


def status_counter(output, observation):
    for key in ('observed', 'available', 'unavailable', 'unknown', 'other', 'failed_or_unavailable'):
        output.setdefault(key, 0)
    output['observed'] = output.get('observed', 0) + 1
    state = observation.get('index_state')
    key = state if state in ('available', 'unavailable', 'unknown') else 'other'
    output[key] = output.get(key, 0) + 1
    if observation.get('is_error') or state != 'available':
        output['failed_or_unavailable'] = output.get('failed_or_unavailable', 0) + 1
        output.setdefault('first_failure', observation)
    output['last_observation'] = observation


def summarize_samples(probes, samples, requested):
    counter = ProbeCounters(probes, requested)
    for sample in samples:
        counter.sample(sample)
    return counter.report


def initial_index_campaign(fixture, project, probes, semantic, output=None):
    started = time.perf_counter()
    deadline = time.monotonic() + fixture.job_timeout
    result = output if output is not None else {}
    counter = ProbeCounters(probes)
    result.update({'during_index': {}, 'probe_summary': counter.report, 'status_observations': {}})
    receipt = enqueue_owned(fixture, project, 'index', deadline)
    sample_offset_ms = (time.perf_counter() - started) * 1000
    first = sample(fixture, project, probes, deadline=deadline, output=result['during_index'])
    counter.sample(first, sample_offset_ms)
    if first.get('first_status'):
        status_counter(result['status_observations'], first['first_status'])
    if first.get('warm_status'):
        status_counter(result['status_observations'], first['warm_status'])
    require(not first['aborted'], first.get('code', 'sample_aborted'))
    with Session(fixture, project, deadline=deadline) as session:
        while True:
            try:
                _, observation = session.tool('status')
            except ObservationFailure as error:
                status_counter(result['status_observations'], error.observation)
                raise
            status_counter(result['status_observations'], observation)
            job = terminal_job(fixture, receipt, deadline)
            result.update({'published': observation, 'job_state': job['state']})
            for ordinal, (name, arguments) in enumerate(probes):
                if not counter.report['probes'][ordinal]['usable_nonempty']:
                    try:
                        _, measured = session.tool(name, arguments)
                    except ObservationFailure as error:
                        counter.observe(ordinal, error.observation, (time.perf_counter() - started) * 1000)
                        raise
                    counter.observe(ordinal, measured, round((time.perf_counter() - started) * 1000, 3))
            require(job['state'] not in ('failed', 'canceled'), 'index_job_failed')
            require(observation['semantic_state'] != 'failed', 'semantic_not_ready')
            source_ready = job['state'] == 'succeeded' and observation['index_state'] == 'available'
            semantic_ready = not semantic or observation['semantic_state'] == 'ready'
            if source_ready and semantic_ready:
                break
            time.sleep(min(.2, remaining(deadline, .2)))
    result.update({'elapsed_ms': round((time.perf_counter() - started) * 1000, 3),
        'first_usable_ms': {f'{row["ordinal"]}:{row["kind"]}': row['first_usable_ms']
                            for row in counter.report['probes'] if 'first_usable_ms' in row},
        'missing_usable_probes': sum(not row['usable_nonempty'] for row in counter.report['probes'])})
    return result


def enqueue_owned(fixture, project, kind, deadline):
    epoch = fixture.rpc('request_epoch', deadline=deadline)
    return fixture.rpc({'enqueue': {'project': fixture.projects[project]['binding'], 'kind': kind,
        'caller': uuid.uuid4().hex, 'key': {'namespace': epoch['namespace'], 'epoch': epoch['epoch'],
                                         'nonce': uuid.uuid4().hex}}}, deadline=deadline)


def wait_owned(fixture, receipt, deadline):
    while True:
        job = terminal_job(fixture, receipt, deadline)
        if job['state'] in ('succeeded', 'failed', 'canceled'):
            return job
        remaining(deadline, 1)
        time.sleep(min(.1, remaining(deadline, .1)))


def cancellation_restart_campaign(fixture, project):
    deadline = time.monotonic() + fixture.job_timeout
    receipt = enqueue_owned(fixture, project, 'reconcile', deadline)
    fixture.rpc({'cancel': {'id': receipt['job']['id']}}, deadline=deadline)
    job = wait_owned(fixture, receipt, deadline)
    return {'state': job['state'], 'cancel_won': job['state'] == 'canceled'}


def timings(samples, requested=None):
    requested = len(samples) if requested is None else requested
    observed = [sample['first_status'] for sample in samples if sample.get('first_status') is not None]
    values = sorted(observation['elapsed_ms'] for observation in observed)
    result = {'requested_samples': requested, 'recorded_samples': len(samples),
              'first_status_observations': len(observed), 'first_status_missing': max(0, requested - len(observed)),
              'first_status_failures': sum(observation.get('is_error') or observation.get('index_state') != 'available' for observation in observed)}
    def percentile(p):
        return values[min(len(values) - 1, max(0, math.ceil(len(values) * p) - 1))]
    result.update({'first_status_p50_ms': percentile(.5) if values else None,
                   'first_status_p95_ms': percentile(.95) if values else None})
    return result


def independent_availability(fixture, project, probes, origin, deadline, counter, output):
    """Later sessions are separately counted observations, never hidden retries."""
    output.update({'basis': 'independent_sessions_after_exact_first_sample', 'maximum_sessions': 32,
                   'independent_sessions': 0, 'probe_summary': counter.report, 'status_observations': {}})
    try:
        while not counter.report['all_probes_eventually_usable'] and output['independent_sessions'] < 32:
            if time.monotonic() >= deadline:
                output['stop_reason'] = 'availability_deadline'
                break
            time.sleep(min(.25, remaining(deadline, .25)))
            if time.monotonic() >= deadline:
                output['stop_reason'] = 'availability_deadline'
                break
            output['independent_sessions'] += 1
            with Session(fixture, project, deadline=deadline) as session:
                try:
                    _, status = session.tool('status')
                except ObservationFailure as error:
                    status_counter(output['status_observations'], error.observation)
                    raise
                status_counter(output['status_observations'], status)
                for ordinal, (name, arguments) in enumerate(probes):
                    if not counter.report['probes'][ordinal]['usable_nonempty']:
                        try:
                            _, measured = session.tool(name, arguments)
                        except ObservationFailure as error:
                            counter.observe(ordinal, error.observation, round((time.perf_counter() - origin) * 1000, 3))
                            raise
                        counter.observe(ordinal, measured, round((time.perf_counter() - origin) * 1000, 3))
    except Exception as error:
        code = failure_code(error)
        output.update({'aborted': True, 'code': code,
                       'stop_reason': 'budget_exhausted' if code == 'campaign_deadline' else 'transport_or_protocol_failure'})
        raise
    output.setdefault('stop_reason', 'all_probes_observed_usable' if counter.report['all_probes_eventually_usable'] else 'session_limit')
    output['missing_usable_probes'] = sum(not row['usable_nonempty'] for row in counter.report['probes'])


def reopen_campaign(fixture, project, probes, samples, output=None):
    result = output if output is not None else {}
    result.update({'service_restarts': 1, 'requested_frontend_sessions': samples,
                   'sequence': 'one_healthy_restart_first_sample_independent_availability_remaining_frontends',
                   'sessions': [], 'availability': {}, 'completion_basis': 'workflow_only_not_probe_success',
                   'probe_summary': ProbeCounters(probes, samples).report, 'status_summary': timings([], samples)})
    origin = time.perf_counter()
    close_owned_fixture(fixture)
    fixture.log = (fixture.owner / 'service.stderr').open('ab')
    fixture.child = None
    fixture.start()
    result['shutdown_and_start_ms'] = round((time.perf_counter() - origin) * 1000, 3)
    counter = ProbeCounters(probes)
    result['availability']['probe_summary'] = counter.report
    first = {'started_since_restart_ms': round((time.perf_counter() - origin) * 1000, 3)}
    result['sessions'].append(first)
    sample(fixture, project, probes, output=first)
    counter.sample(first, first['started_since_restart_ms'])
    result['first_after_restart_sample_index'] = 0
    result['probe_summary'] = summarize_samples(probes, result['sessions'], samples)
    result['status_summary'] = timings(result['sessions'], samples)
    require(not first['aborted'], first.get('code', 'sample_aborted'))
    deadline = time.monotonic() + fixture.command_timeout
    independent_availability(fixture, project, probes, origin, deadline, counter, result['availability'])
    for _ in range(1, samples):
        row = {'started_since_restart_ms': round((time.perf_counter() - origin) * 1000, 3)}
        result['sessions'].append(row)
        sample(fixture, project, probes, output=row)
        result['probe_summary'] = summarize_samples(probes, result['sessions'], samples)
        result['status_summary'] = timings(result['sessions'], samples)
        require(not row['aborted'], row.get('code', 'sample_aborted'))
    result['first_sample_status_summary'] = timings(result['sessions'][:1], 1)
    result['later_sample_status_summary'] = timings(result['sessions'][1:], max(0, samples - 1))
    return result


def diagnostic_identity(fixture, project):
    # Explicitly after the initial campaign's first status/read observations.
    # This is not used to turn a failed first response into a success.
    deadline = time.monotonic() + fixture.command_timeout
    with Session(fixture, project, deadline=deadline) as session:
        envelope, observed = session.tool('diagnostics')
        value = tool_value(envelope)
    semantic = value.get('semantic_readiness') or {}
    result = {}
    for key in ('pipeline_digest', 'selection_digest'):
        digest_value = semantic.get(key)
        result[key] = digest_value if isinstance(digest_value, str) and re.fullmatch('[0-9a-f]{64}', digest_value) else None
    policy = semantic.get('default_search_policy')
    result['default_search_policy'] = policy if policy in ('partial_hybrid', 'lexical_while_pending', 'require_complete') else 'other'
    result['collected_after_initial_observation'] = True
    result['observation'] = observed
    return result


def run(args, result=None):
    corpus = [Path(args.airk), Path(args.bookbuilder)]
    roots = isolate(corpus, [args.runtime_dir, args.report, args.state_parent])
    state_parent = physical(args.state_parent)
    require(inside(state_parent, physical(tempfile.gettempdir())), 'temporary_state_required')
    identity = [source_identity(root) for root in roots]
    runtime = verify_runtime(args.runtime_dir, args.runtime_sha256)
    result = result if result is not None else {}
    result.update({'schema': 1, 'runtime': runtime, 'samples': args.samples, 'environment': environment_facts(),
              'completion_basis': 'workflow_only_not_probe_success',
              'cache_condition': 'fresh_process_not_cold_filesystem', 'service_route': 'owned_explicit_service',
              'policy': {'watch': 'disabled', 'index': 'native_default', 'vector_selection': 'native_default',
                         'semantic': 'bundled_default' if args.semantic else 'not_provisioned',
                         'concurrent_corpora': False},
              'limits': ['installed_launcher_not_measured', 'editor_not_measured', 'modified_source_incremental_not_measured',
                         'background_audit_interference_not_measured'] + ([] if args.excerpt_path else ['excerpt_not_measured']), 'corpora': []})
    with tempfile.TemporaryDirectory(prefix='airk-real-corpora-', dir=state_parent) as temporary:
        owner = physical(temporary)
        isolate(roots, [owner])
        fixture = clients().Fixture(args.runtime_dir, owner, command_timeout=args.command_timeout, job_timeout=args.job_timeout)
        primary = None
        try:
            timed_preparation(result, 'owned_service_start_ms', fixture.start)
            for alias, root, before in zip(('airk', 'bookbuilder'), roots, identity):
                data = owner / f'data-{alias}'
                data.mkdir()
                row = {'alias': alias, 'source': before, 'watch_enabled': False, 'semantic_requested': args.semantic,
                       'initial_index': {}, 'sessions': [], 'reopen': {}, 'before_index': {
                           'publication_observation': 'unmeasured_to_preserve_first_status',
                           'model_setup_can_publish_initial_head': args.semantic}}
                result['corpora'].append(row)
                preparation_started = time.perf_counter()
                timed_preparation(row['before_index'], 'registration_ms', lambda: fixture.register(alias, root, data))
                fixture.rpc({'configure_watch': {'project': fixture.projects[alias]['binding'], 'enabled': False}})
                watched = fixture.rpc({'watch': {'project': fixture.projects[alias]['binding']}})
                require(watched['observation']['enabled'] is False, 'watch_not_disabled')
                if args.semantic:
                    timed_preparation(row['before_index'], 'model_setup_ms', lambda: fixture.job(alias, 'setup-model'))
                else:
                    row['before_index']['model_setup_ms'] = None
                row['before_index']['total_ms'] = round((time.perf_counter() - preparation_started) * 1000, 3)
                probes = [('list_files', {'limit': 1, 'max_bytes': 4096, 'response_mode': 'full'}),
                          ('search', {'query': args.lexical_query, 'mode': 'lexical', 'limit': 1, 'max_bytes': 4096, 'response_mode': 'full'})]
                if args.excerpt_path:
                    require(not Path(args.excerpt_path).is_absolute() and '..' not in Path(args.excerpt_path).parts, 'excerpt_path_invalid')
                    probes.append(('read_file', {'path': args.excerpt_path, 'start_line': 1, 'end_line': 2, 'max_bytes': 4096, 'response_mode': 'full'}))
                if args.semantic:
                    probes += [('search', {'query': args.lexical_query, 'mode': mode, 'limit': 1, 'max_bytes': 4096, 'response_mode': 'full'}) for mode in ('vector', 'hybrid')]
                initial = initial_index_campaign(fixture, alias, probes, args.semantic, row['initial_index'])
                unchanged(root, before)
                configured_identity = diagnostic_identity(fixture, alias)
                sessions = row['sessions']
                for _ in range(args.samples):
                    measured = {}
                    sessions.append(measured)
                    sample(fixture, alias, probes, output=measured)
                    row['session_summary'] = timings(sessions, args.samples)
                    row['session_probe_summary'] = summarize_samples(probes, sessions, args.samples)
                    require(not measured['aborted'], measured.get('code', 'sample_aborted'))
                reopen = reopen_campaign(fixture, alias, probes, args.samples, row['reopen'])
                cancel = cancellation_restart_campaign(fixture, alias)
                reconcile_started = time.perf_counter()
                deadline = time.monotonic() + fixture.job_timeout
                reconcile = wait_owned(fixture, enqueue_owned(fixture, alias, 'reconcile', deadline), deadline)
                require(reconcile['state'] == 'succeeded', 'reconcile_failed')
                unchanged(root, before)
                row.update({'alias': alias, 'source': before, 'watch_enabled': False,
                    'semantic_requested': args.semantic, 'semantic_identity': configured_identity, 'initial_index': initial, 'sessions': sessions,
                    'reopen': reopen, 'session_summary': timings(sessions, args.samples),
                    'cancellation_after_healthy_reopen': cancel, 'unchanged_reconcile': {'state': reconcile['state'],
                        'elapsed_ms': round((time.perf_counter() - reconcile_started) * 1000, 3)}})
            for root, before in zip(roots, identity):
                unchanged(root, before)
        except BaseException as error:
            primary = error
            raise
        finally:
            cleanup = result.setdefault('cleanup_detail', {})
            try:
                result['cleanup_failed'] = close_owned_fixture(fixture, primary, cleanup)
            finally:
                if cleanup.get('failed_stages'):
                    result['cleanup_failed'] = True
    result['source_identity_preserved'] = True
    return result


def validate_output(args):
    # All checks precede mkdir/open, including when the campaign itself fails.
    isolate([args.airk, args.bookbuilder], [args.report, args.runtime_dir, args.state_parent])
    report = physical(args.report, exists=False)
    runtime = physical(args.runtime_dir, exists=False)
    require(not inside(report, runtime) and not inside(runtime, report), 'report_runtime_overlap')
    require(not report.exists(), 'report_already_exists')
    return report


def observation_outcome(report):
    summaries = []
    def collect(value):
        if isinstance(value, dict):
            budget = value.get('budget_exhausted') or value.get('aborted') and value.get('code') == 'campaign_deadline'
            if budget:
                summaries.append('budget_exhausted')
            elif value.get('transport_or_protocol_failure'):
                summaries.append('transport_protocol_failures_observed')
            elif value.get('is_error'):
                summaries.append('native_errors_observed')
            if value.get('failed_or_unavailable', 0) or value.get('first_status_failures', 0):
                summaries.append('status_failures_observed')
            if value.get('aborted') and not budget:
                summaries.append('transport_protocol_failures_observed')
            status = value.get('first_status')
            if status is not None and (status.get('is_error') or status.get('index_state') != 'available'):
                summaries.append('status_failures_observed')
            if 'probes' in value and 'observation_outcome' in value:
                summaries.append(value['observation_outcome'])
            for child in value.values(): collect(child)
        elif isinstance(value, list):
            for child in value: collect(child)
    collect(report.get('corpora', []))
    for outcome in ('transport_protocol_failures_observed', 'native_errors_observed', 'budget_exhausted', 'status_failures_observed', 'incomplete_observations', 'nonusable_observations'):
        if outcome in summaries: return outcome
    return 'all_requested_observations_usable' if summaries else 'not_observed'


def execute(args):
    report = {'schema': 1, 'completed': False}
    try:
        destination = validate_output(args)
    except Exception as error:
        # An unsafe output target receives no directory creation and no file.
        return {'schema': 1, 'completed': False, 'code': failure_code(error)}, False
    try:
        require(1 <= args.samples <= 1000 and args.command_timeout > 0 and args.job_timeout > 0, 'argument_budget')
        run(args, report)
        report['completed'] = True
    except Exception as error:
        report.update({'completed': False, 'code': failure_code(error)})
    report['observation_outcome'] = observation_outcome(report)
    # Revalidate before mkdir and use exclusive creation; an output race never
    # authorizes replacement and does not erase measurements already collected.
    try:
        validate_output(args)
        destination.parent.mkdir(parents=True, exist_ok=True)
        validate_output(args)
        with destination.open('x', encoding='utf-8') as stream:
            json.dump(report, stream, indent=2)
            stream.write('\n')
    except Exception as error:
        report.update({'completed': False, 'output_code': failure_code(error)})
        return report, False
    return report, True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-dir', type=Path, required=True)
    parser.add_argument('--runtime-sha256', required=True, help='Install digest approved from the reviewed lightweight builder run')
    parser.add_argument('--airk', type=Path, required=True, help='Read-only AIRK corpus root')
    parser.add_argument('--bookbuilder', type=Path, required=True, help='Read-only bookbuilder corpus root')
    parser.add_argument('--state-parent', type=Path, default=Path(tempfile.gettempdir()))
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--samples', type=int, default=1)
    parser.add_argument('--command-timeout', type=int, default=120)
    parser.add_argument('--job-timeout', type=int, default=3600)
    parser.add_argument('--lexical-query', default='configuration')
    parser.add_argument('--excerpt-path', help='Optional common collection-relative existing path; never written to the report')
    parser.add_argument('--semantic', action='store_true')
    args = parser.parse_args()
    try:
        report, _ = execute(args)
    except Exception as error:
        report = {'schema': 1, 'completed': False, 'code': failure_code(error)}
    print(json.dumps(report, indent=2))
    return 0 if report.get('completed') else 1


if __name__ == '__main__':
    raise SystemExit(main())
