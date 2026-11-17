# SPDX-License-Identifier: Apache-2.0
"""Read-only probe: eight independent MCP frontends, one registered project."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
import importlib.util
import json
from pathlib import Path
import time
import tomllib

spec = importlib.util.spec_from_file_location('investigation', 'scripts/benchmark-mcp-investigation.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
configuration = tomllib.loads(Path('.codex/config.toml').read_text(encoding='utf-8'))['mcp_servers']['airk']
configuration = {**configuration, 'expected_version': '1.8.4', 'cwd': str(Path.cwd())}
rows = []
def record(client, phase, tool, args, session, attempt=1):
    start = time.perf_counter()
    try:
        raw, timing = session.request('tools/call', {'name': tool, 'arguments': args})
        row = {'client': client, 'phase': phase, 'tool': tool, 'args': args, 'attempt': attempt, 'elapsed_ms': (time.perf_counter()-start)*1000, 'raw': raw, 'parsed': module.body(raw)}
    except Exception as exc:
        row = {'client': client, 'phase': phase, 'tool': tool, 'args': args, 'attempt': attempt, 'elapsed_ms': (time.perf_counter()-start)*1000, 'exception': str(exc)}
    return row

tasks = [
    ('references', {'name': 'Session', 'path_prefix': 'crates/airk-mcp/src/response.rs', 'order': 'path', 'include_count': True, 'include_files': True, 'limit': 30, 'max_bytes': 10000}),
    ('code_facts', {'name': 'Session', 'path_prefix': 'crates/airk-cli/src/embedding_client.rs', 'kind': 'definition', 'limit': 10, 'max_bytes': 10000}),
    ('symbols', {'name': 'Session', 'path_prefix': 'crates/airk-mcp/src/response.rs', 'limit': 10, 'max_bytes': 6000}),
    ('read_file', {'path': 'crates/airk-cli/src/embedding_client.rs', 'start_line': 60, 'end_line': 72, 'max_bytes': 3000}),
]
with ExitStack() as stack, ThreadPoolExecutor(max_workers=8) as pool:
    def start(_):
        return module.Session(configuration, 30, prime_status=False).__enter__()
    sessions = []
    try:
        for session in pool.map(start, range(8)):
            sessions.append(session)
            stack.callback(session.__exit__, None, None, None)
        for phase in range(4):
            def run(item):
                client, session = item
                tool, args = ('status', {'response_mode': 'full'}) if phase == 0 else tasks[(client+phase-1)%4]
                return record(client, phase, tool, args, session)
            batch = list(pool.map(run, enumerate(sessions)))
            rows.extend(batch)
            # Complete the burst first; retry only explicit unsubmitted receipts,
            # once, serially. Submitted/unknown outcomes are never replayed.
            for row in batch:
                error = row.get('parsed', {}).get('error', {})
                if row.get('raw', {}).get('isError') and error.get('query_submitted') is False and error.get('retryable') is True:
                    delay = error.get('retry_after_ms')
                    if isinstance(delay, (int, float)) and 0 <= delay <= 1000:
                        time.sleep((delay+25)/1000)
                        rows.append(record(row['client'], phase, row['tool'], row['args'], sessions[row['client']], 2))
            print(json.dumps({'phase': phase, 'initial_calls': len(batch), 'errors': [{'client': row['client'], 'error': row.get('parsed', {}).get('error'), 'exception': row.get('exception')} for row in batch if row.get('raw', {}).get('isError') or row.get('exception')]}), flush=True)
    finally:
        Path(__file__).with_name('multi-client-probe.json').write_text(json.dumps({'clients': 8, 'note': 'Eight independent stdio frontends use the existing configured launcher/project. No install/index/repair action. Session startup has no status priming; measured calls use raw request, with at most one typed unsubmitted retry.', 'calls': rows}, indent=2)+'\n', encoding='utf-8')
