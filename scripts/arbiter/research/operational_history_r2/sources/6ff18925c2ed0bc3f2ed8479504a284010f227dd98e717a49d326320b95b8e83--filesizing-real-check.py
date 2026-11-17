# SPDX-License-Identifier: Apache-2.0
"""Local-only qualification; output and indexed private sources must not be committed."""
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

BASE = Path(__file__).resolve().parent.parent
OUT = Path(os.environ.get('AIRK_REAL_OUTPUT', str(BASE / 'artifacts' / 'filesizing-real-repositories')))
STATE = OUT / 'state'
BINARY = BASE / 'target' / 'release' / 'airk.exe'
ROOTS = [BASE, Path('D:/code/Github/bookbuilder')]
ENV = dict(os.environ, AIRK_CLIENT='vscode')
SUMMARY = {}

def save(name, value):
    (OUT / (name + '.json')).write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')

def spec(root):
    identity = str(root.resolve())
    collection = 'project-' + hashlib.sha256(identity.lower().encode()).hexdigest()[:24]
    return [str(BINARY), '--root', identity, '--data-dir', str(STATE), '--collection', collection], collection

def native(root, name, args, timeout=300):
    start = time.perf_counter()
    result = subprocess.run(spec(root)[0] + args, cwd=root, env=ENV, capture_output=True, text=True, encoding='utf-8', timeout=timeout)
    elapsed = time.perf_counter() - start
    save(name + '-timing', {'seconds': elapsed, 'exit_code': result.returncode})
    (OUT / (name + '.stderr.txt')).write_text(result.stderr, encoding='utf-8')
    (OUT / (name + '.stdout.json')).write_text(result.stdout, encoding='utf-8')
    if result.returncode:
        raise RuntimeError(f'{name} failed after {elapsed:.3f}s: {result.stderr[-2000:]}')
    return json.loads(result.stdout), elapsed

class Mcp:
    def __init__(self, root):
        self.root = root
        self.args, self.collection = spec(root)
        self.stderr = (OUT / (root.name + '-mcp.stderr.txt')).open('w', encoding='utf-8')
        self.process = subprocess.Popen(self.args + ['mcp'], cwd=root, env=ENV, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True, encoding='utf-8')
        self.responses = queue.Queue()
        threading.Thread(target=lambda: [self.responses.put(line) for line in self.process.stdout], daemon=True).start()
        self.next_id = 0
        self.transcript = []
        self.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'airk-real-repository-check', 'version': '1.1.0'}})
        self.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
        tools = self.request('tools/list', {})['result']['tools']
        assert len(tools) == 16

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + '\n')
        self.process.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        self.send({'jsonrpc': '2.0', 'id': self.next_id, 'method': method, 'params': params})
        while True:
            message = json.loads(self.responses.get(timeout=60))
            if message.get('id') == self.next_id:
                self.transcript.append({'method': method, 'params': params, 'response': message})
                return message

    def call(self, tool, args, rejected=False, empty=False):
        start = time.perf_counter()
        response = self.request('tools/call', {'name': tool, 'arguments': args})
        elapsed = time.perf_counter() - start
        failed = 'error' in response or response.get('result', {}).get('isError', False)
        if rejected:
            assert failed, f'{tool} should reject {args}'
            return None, elapsed
        if empty and failed:
            return None, elapsed
        assert not failed, response
        evidence = json.loads(response['result']['content'][0]['text'])
        collection = evidence.get('collection') or evidence.get('read', {}).get('publication', {}).get('collection')
        assert collection == self.collection, evidence
        if empty:
            assert not evidence.get('hits') and evidence['read']['status'] != 'content', evidence
        def validate(value):
            if isinstance(value, dict):
                if 'collection' in value:
                    assert value['collection'] == self.collection
                if 'paths' in value:
                    for relative in value['paths']:
                        path = self.root / relative
                        assert path.resolve().is_relative_to(self.root.resolve()) and path.is_file(), relative
                if 'path' in value and isinstance(value['path'], str):
                    path = self.root / value['path']
                    assert path.resolve().is_relative_to(self.root.resolve()), value['path']
                    assert path.is_file(), value['path']
                    if 'content_hash' in value:
                        assert hashlib.sha256(path.read_bytes()).hexdigest() == value['content_hash'], value['path']
                for child in value.values():
                    validate(child)
            elif isinstance(value, list):
                for child in value:
                    validate(child)
        validate(evidence)
        return evidence, elapsed

    def close(self):
        save(self.root.name + '-mcp-transcript-private', self.transcript)
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=10)
        self.stderr.close()

def index():
    assert not STATE.exists(), 'Fresh state required for honest cold-index timing'
    OUT.mkdir(parents=True, exist_ok=True)
    save('binary', {'version': subprocess.check_output([str(BINARY), '--version'], text=True).strip(),
        'sha256': hashlib.sha256(BINARY.read_bytes()).hexdigest(), 'profile': 'release'})
    for root in ROOTS:
        print(f'{root.name}: starting fresh scheduled index', flush=True)
        report, seconds = native(root, root.name + '-cold-index', ['index', '--scheduled', '--automatic'], timeout=3600)
        SUMMARY[root.name] = {'collection': spec(root)[1], 'index_seconds': seconds, 'index': report}
        save('summary-private', SUMMARY)
        print(json.dumps({'repository': root.name, 'index_seconds': round(seconds, 3), 'indexed': report.get('indexed'), 'source_bytes': report.get('source_bytes'), 'skipped': len(report.get('skipped', []))}), flush=True)

def source_inventory(root):
    names = subprocess.check_output(['git', '-c', 'safe.directory=' + root.as_posix(), '-C', str(root),
        'ls-files', '-z', '--cached', '--others', '--exclude-standard']).decode('utf-8').split('\0')
    hashes = {}
    for name in sorted(set(names)):
        if not name or any(part in ('temp', '.airk', 'artifacts') for part in Path(name).parts):
            continue
        path = root / name
        if path.is_file() and not path.is_symlink():
            hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes

def verify_continuity(root):
    directory = STATE / 'collections' / spec(root)[1]
    inventories = list((directory / 'generations').glob('*/inventory.json'))
    assert len(inventories) == 1, 'Expected one fresh published generation'
    inventory = json.loads(inventories[0].read_text(encoding='utf-8'))
    assert inventory['collection'] == spec(root)[1]
    count = 0
    for file in inventory['files'].values():
        payload_path = directory / 'objects' / 'artifacts' / file['state']['payload_digest']
        payload = json.loads(payload_path.read_text(encoding='utf-8'))
        records = file.get('continuity', [])
        assert [r['descriptor'] for r in records] == payload.get('continuity', []), 'Continuity descriptors were omitted or changed'
        count += len(records)
    return {'records': count, 'inventory_bytes': inventories[0].stat().st_size,
        'all_extracted_descriptors_preserved': True,
        'logical_collection_bytes': sum(p.stat().st_size for p in directory.rglob('*') if p.is_file())}

def retrieval():
    clients = [Mcp(root) for root in ROOTS]
    terms = ['automatic_scan_limits', 'PipelinePromptTokenRules']
    paths = ['crates/airk-kernel/src/limits.rs', 'pro/src/WritingForgePro.Core/Validation/PipelinePromptTokenRules.cs']
    try:
        for i, client in enumerate(clients):
            other = 1 - i
            checks = []
            for tool, args, field, positive in [
                ('airk_status', {}, None, True),
                ('airk_search', {'query': terms[i], 'limit': 10}, 'hits', True),
                ('airk_search', {'query': terms[other], 'limit': 10}, 'hits', False),
                ('airk_search', {'query': 'return', 'limit': 20}, 'hits', True),
                ('airk_symbols', {'name': terms[i]}, 'facts', True),
                ('airk_symbols', {'name': terms[other]}, 'facts', False),
                ('airk_list_files', {'prefix': paths[i], 'limit': 10}, 'paths', True),
                ('airk_list_files', {'prefix': paths[other], 'limit': 10}, 'paths', False),
                ('airk_read_file', {'path': paths[i], 'start_line': 1, 'end_line': 12}, 'hits', True),
            ]:
                evidence, seconds = client.call(tool, args)
                if field:
                    assert bool(evidence[field]) == positive, (tool, args, evidence)
                checks.append({'tool': tool, 'arguments': args, 'seconds': seconds, 'count': len(evidence[field]) if field else None, 'passed': True})
            _, seconds = client.call('airk_read_file', {'path': paths[other], 'start_line': 1, 'end_line': 12}, empty=True)
            checks.append({'tool': 'airk_read_file', 'seconds': seconds, 'foreign_path_no_evidence': True, 'passed': True})
            for tool, args in [
                ('airk_read_file', {'path': '../' + ROOTS[other].name + '/' + paths[other], 'start_line': 1, 'end_line': 12}),
                ('airk_search', {'query': terms[other], 'collection': clients[other].collection}),
            ]:
                _, seconds = client.call(tool, args, rejected=True)
                checks.append({'tool': tool, 'arguments': args, 'seconds': seconds, 'rejected': True, 'passed': True})
            SUMMARY.setdefault(client.root.name, {})['mcp_checks'] = checks
            print(f'{client.root.name}: {len(checks)} MCP checks passed', flush=True)
    finally:
        for client in clients:
            client.close()
        save('summary-private', SUMMARY)

if __name__ == '__main__':
    import sys
    if len(sys.argv) == 1:
        before = {root.name: source_inventory(root) for root in ROOTS}
        index()
    else:
        SUMMARY.update(json.loads((OUT / 'summary-private.json').read_text(encoding='utf-8')))
    retrieval()
    for root in ROOTS:
        limits, _ = native(root, root.name + '-limits', ['collections', 'limits', 'describe'])
        coverage, seconds = native(root, root.name + '-coverage', ['coverage', '--refresh'])
        SUMMARY[root.name]['limits'] = limits
        SUMMARY[root.name]['coverage'] = coverage
        SUMMARY[root.name]['coverage_seconds'] = seconds
        SUMMARY[root.name]['continuity'] = verify_continuity(root)
        save('summary-private', SUMMARY)
    if len(sys.argv) == 1:
        for root in ROOTS:
            assert source_inventory(root) == before[root.name], f'{root.name}: source files changed during qualification'
            SUMMARY[root.name]['unchanged_source_files'] = len(before[root.name])
        save('summary-private', SUMMARY)
    print('Both real-repository indexing and MCP isolation checks passed.', flush=True)
