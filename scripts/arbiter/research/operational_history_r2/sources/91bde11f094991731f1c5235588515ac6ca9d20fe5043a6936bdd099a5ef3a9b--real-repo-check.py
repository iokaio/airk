# SPDX-License-Identifier: Apache-2.0
# Local-only qualification driver. Repository contents and derived state stay ignored.
import hashlib, importlib.util, json, os, queue, subprocess, sys, threading, time
from pathlib import Path

project = Path(r'D:\code\Github\airk')
build = Path(sys.argv[1]).resolve()
binary = build / 'native' / 'stage' / 'airk.exe'
output = build / 'real-repo-tests'
output.mkdir(exist_ok=False)
data = output / 'state'
env = {**os.environ, 'AIRK_USAGE_MODE': 'disabled', 'AIRK_ERROR_FORMAT': 'json', 'AIRK_CLIENT': 'local-qualification'}
os.environ.update({key: env[key] for key in ('AIRK_USAGE_MODE', 'AIRK_ERROR_FORMAT', 'AIRK_CLIENT')})
spec = importlib.util.spec_from_file_location('detail_driver', project / 'scripts/benchmark-detail.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
results = []

def log(**value):
    print(json.dumps(value), flush=True)

def save():
    (output / 'results.json').write_text(json.dumps({'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'note': 'Local observational timings, no baseline comparison; source workspaces may remain active.', 'repositories': results}, indent=2), encoding='utf-8')

def run(command, arguments, timeout=1200):
    start = time.perf_counter()
    completed = subprocess.run(command + arguments, capture_output=True, text=True, encoding='utf-8', env=env,
        timeout=timeout, creationflags=subprocess.CREATE_NO_WINDOW)
    if completed.returncode:
        raise RuntimeError(completed.stderr[-8000:])
    return json.loads(completed.stdout), round(time.perf_counter() - start, 3)

class Reader(module.Mcp):
    def __init__(self, *args):
        self.verified = 0
        self.changed = 0
        super().__init__(*args)
    def validate(self, value):
        if isinstance(value, list):
            for item in value: self.validate(item)
        elif isinstance(value, dict):
            if 'collection' in value:
                assert value['collection'] == self.collection, 'foreign collection'
            if 'path' in value and 'content_hash' in value:
                source = (self.root / value['path']).resolve()
                assert source.is_relative_to(self.root.resolve()), 'escaped citation'
                if source.is_file() and hashlib.sha256(source.read_bytes()).hexdigest() == value['content_hash']:
                    self.verified += 1
                else:
                    self.changed += 1
            for item in value.values(): self.validate(item)

def query(reader, mode):
    value, elapsed = reader.call('airk_search', {'query': 'configuration' if mode == 'lexical' else 'how is the application configured',
        'mode': mode, 'limit': 3, 'max_bytes': 6000})
    return {'mode': mode, 'elapsed_seconds': round(elapsed, 3), 'hits': len(value.get('hits', [])),
        'discovery': value.get('discovery_readiness', {}).get('state'), 'semantic': value.get('semantic_readiness', {}).get('state'),
        'truncated': value.get('truncated'), 'verified_citations': reader.verified, 'changed_live_citations': reader.changed}

for name in ('iokaio-web', 'bookbuilder'):
    root = Path(r'D:\code\Github') / name
    collection = 'local-125-' + name
    command = [str(binary), '--root', str(root), '--data-dir', str(data), '--collection', collection]
    row = {'name': name, 'state': 'started'}
    results.append(row); save()
    reader = None
    try:
        if not (data / 'models/local/model.json').exists():
            run(command, ['collections', 'vectors', 'install', str(project / 'target/local-vector-model')])
        reader = Reader(command, root, collection, output / (name + '-mcp-stderr.log'))
        assert len(reader.request('tools/list', {})['result']['tools']) == 15
        start = time.perf_counter()
        child = subprocess.Popen(command + ['index', '--scheduled', '--progress'], stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, encoding='utf-8', env=env, creationflags=subprocess.CREATE_NO_WINDOW)
        lines = queue.Queue()
        def consume(pipe):
            for line in pipe: lines.put(line)
        threading.Thread(target=consume, args=(child.stderr,), daemon=True).start()
        stdout = []
        threading.Thread(target=lambda: stdout.append(child.stdout.read()), daemon=True).start()
        last_progress = last_log = start
        errors = []
        while child.poll() is None or not lines.empty():
            try: line = lines.get(timeout=1)
            except queue.Empty:
                if time.perf_counter() - last_progress > 600 or time.perf_counter() - start > 3600:
                    child.kill(); child.wait()
                    raise TimeoutError('scan exceeded the editor idle/total time bound')
                continue
            try: event = json.loads(line)
            except json.JSONDecodeError:
                errors.append(line); continue
            if event.get('phase'):
                last_progress = time.perf_counter()
                row['scan_progress'] = event
                if event['phase'] == 'base_committed' and 'first_publication_seconds' not in row:
                    row['first_publication_seconds'] = round(time.perf_counter() - start, 3)
                    try: row['early_mcp_search'] = query(reader, 'lexical')
                    except Exception as error: row['early_mcp_search_error'] = str(error)[:2000]
                    log(repository=name, first_publication_seconds=row['first_publication_seconds'], early_search=row.get('early_mcp_search'))
                    save()
                if time.perf_counter() - last_log >= 20:
                    log(repository=name, phase=event['phase'], processed=event.get('processed_artifacts'), committed=event.get('committed_artifacts'), elapsed_seconds=round(time.perf_counter()-start, 1))
                    last_log = time.perf_counter()
            else: errors.append(line)
        if child.returncode:
            raise RuntimeError(''.join(errors)[-8000:])
        while not stdout: time.sleep(.01)
        report = json.loads(stdout[0])
        row['source_scan_seconds'] = round(time.perf_counter() - start, 3)
        row['source_report'] = {key: report.get(key) for key in ('indexed', 'reused', 'removed', 'source_bytes', 'first_ready_us', 'scan_extract_us', 'publish_us', 'embedding_inferences', 'current')}
        row['skipped_files'] = len(report.get('skipped', []))
        row['state'] = 'semantic_pending'; save()
        status, _ = run(command, ['status'])
        batch = 0
        semantic_start = time.perf_counter()
        while status['semantic_readiness']['state'] not in ('ready', 'disabled'):
            batch += 1
            outcome, elapsed = run(command, ['semantic', 'run', '--scheduled'])
            if outcome['state'] not in ('enriched', 'idle'):
                raise RuntimeError('semantic worker: ' + json.dumps(outcome))
            status, _ = run(command, ['status'])
            ready = status['semantic_readiness']
            row['semantic_batches'] = batch
            row['semantic_progress'] = ready
            row['elapsed_seconds'] = round(time.perf_counter() - start, 3)
            save()
            log(repository=name, phase='semantic', batch=batch, batch_seconds=elapsed, completed=ready['completed_artifacts'], selected=ready['selected_artifacts'], vectors=status['indexed_vectors'])
        row['semantic_seconds'] = round(time.perf_counter() - semantic_start, 3)
        row['fully_ready_seconds'] = round(time.perf_counter() - start, 3)
        row['files'] = status['files']; row['vectors'] = status['indexed_vectors']
        row['queries'] = [query(reader, mode) for mode in ('lexical', 'lexical', 'hybrid', 'hybrid', 'vector')]
        row['state'] = 'passed'
        save(); log(repository=name, state='passed', source_scan_seconds=row['source_scan_seconds'], fully_ready_seconds=row['fully_ready_seconds'], files=row['files'], vectors=row['vectors'])
    except Exception as error:
        row['state'] = 'failed'; row['error'] = str(error)[:8000]
        save(); log(repository=name, state='failed', error=row['error'])
    finally:
        if reader: reader.close()
log(report=str(output / 'results.json'))
sys.exit(0 if all(row['state'] == 'passed' for row in results) else 1)
