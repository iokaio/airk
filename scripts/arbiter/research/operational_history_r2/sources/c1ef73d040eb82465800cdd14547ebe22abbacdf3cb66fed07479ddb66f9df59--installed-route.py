# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
from types import SimpleNamespace

root = Path.cwd()
out = root / 'artifacts/performance-1.8.2-20261101'
spec = importlib.util.spec_from_file_location('bench', root / 'scripts/benchmark-real-corpora.py')
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
config = json.loads((root / '.mcp.json').read_bytes())['mcpServers']['airk']
expected = {'binding': '54c6ddfbd9f5ea27159c390186159e9628fc5951682a7015e13744e6965315c1',
            'collection': 'project-2cb86554e9ba4fd24218d0af'}
fixture = SimpleNamespace(command_timeout=120, projects={'airk': expected})

class InstalledSession(bench.Session):
    def __enter__(self):
        self.started = time.perf_counter()
        self.child = subprocess.Popen([config['command'], *config['args']], cwd=root,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, encoding='utf-8', creationflags=subprocess.CREATE_NO_WINDOW)
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
            self.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                'clientInfo': {'name': 'airk-installed-route-measurement', 'version': '1'}})
            self.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
            self.initialize_ms = round((time.perf_counter() - self.started) * 1000, 3)
            return self
        except BaseException as error:
            self.__exit__(type(error), None, None)
            raise

probes = [('list_files', {'limit': 1, 'max_bytes': 4096, 'response_mode': 'full'}),
    ('search', {'query': 'configuration', 'mode': 'lexical', 'limit': 1, 'max_bytes': 4096, 'response_mode': 'full'}),
    ('read_file', {'path': 'README.md', 'start_line': 1, 'end_line': 2, 'max_bytes': 4096, 'response_mode': 'full'}),
    *[('search', {'query': 'configuration', 'mode': mode, 'limit': 1, 'max_bytes': 4096, 'response_mode': 'full'})
      for mode in ('vector', 'hybrid')]]
result = {'schema': 1, 'route': 'exact_configured_installed_launcher', 'samples': [],
    'limits': ['existing_running_service_and_index', 'fresh_frontends_not_cold_filesystem',
        'native_validation_concurrent', 'unmatched_historical_installed_baseline', 'editor_UI_not_timed'],
    'environment': bench.environment_facts()}
for n in range(5):
    row = {}
    result['samples'].append(row)
    bench.sample(fixture, 'airk', probes, session_type=InstalledSession, output=row)
    result['status_summary'] = bench.timings(result['samples'], 5)
    result['probe_summary'] = bench.summarize_samples(probes, result['samples'], 5)
    (out / 'installed-route.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'sample': n + 1, 'initialize_ms': row.get('initialize_ms'),
        'first_status': row.get('first_status'), 'aborted': row.get('aborted')}), flush=True)
