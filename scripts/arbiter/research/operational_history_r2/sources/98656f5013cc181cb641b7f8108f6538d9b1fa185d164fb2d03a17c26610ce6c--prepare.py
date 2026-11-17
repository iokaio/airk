# SPDX-License-Identifier: Apache-2.0
"""Produce diagnostics-only sibling copies; never mutate the pinned drivers."""
from pathlib import Path
import ast
import hashlib
import json

WORK = Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
OUT = Path(__file__).resolve().parent
names = ('check-usage-inputs.py', 'check-service-clients.py')
bases = {name: (WORK / 'scripts' / name).read_bytes() for name in names}
helper = bases[names[1]].decode('utf-8').replace('\r\n', '\n')
old = '''        def read(until):
            result = self.run(*exact, check=False, timeout=remaining(until, 90))
            assert result.returncode in (0, 1), f"Native read process failed: exit {result.returncode}"
            envelope = json.loads(result.stdout)
            assert result.returncode == 0 or read_retry_delay(envelope) is not None, "Native read failed without a typed rejection"
            return envelope
        envelope, _ = read_envelope(read, deadline, 41 if retry or admission_only else 1, admission_only)
        return tool_value(envelope)
'''
new = '''        last_result = None
        def read(until):
            nonlocal last_result
            result = self.run(*exact, check=False, timeout=remaining(until, 90))
            last_result = result
            assert result.returncode in (0, 1), f"Native read process failed: exit {result.returncode}"
            envelope = json.loads(result.stdout)
            assert result.returncode == 0 or read_retry_delay(envelope) is not None, "Native read failed without a typed rejection"
            return envelope
        try:
            envelope, _ = read_envelope(read, deadline, 41 if retry or admission_only else 1, admission_only)
            return tool_value(envelope)
        except BaseException as error:
            self.capture_failed_read(exact, last_result, error)
            raise

    def capture_failed_read(self, exact, result, error):
        # Diagnostics only: no native calls, retries, environment dump or source
        # scans. Each stream is capped at 64 KiB; at most 32 failed calls retained.
        try:
            with self.diagnostic_lock:
                if self.diagnostic_count >= 32:
                    error.add_note("Additional failed-read diagnostics omitted after 32 records")
                    return
                self.diagnostic_count += 1
                sequence = self.diagnostic_count
            def bounded(raw):
                raw = raw or b""
                if isinstance(raw, str):
                    raw = raw.encode("utf-8", errors="replace")
                kept = raw[:65536]
                return {"utf8": kept.decode("utf-8", errors="replace"),
                        "total_bytes": len(raw), "retained_bytes": len(kept),
                        "truncated": len(raw) > len(kept)}
            path = self.owner / f"failed-read-{sequence:02d}.json"
            value = {"schema": "airk.fixture-failed-read/1", "unix_ms": int(time.time() * 1000),
                     "command": [str(self.binary), *map(str, exact)], "cwd": str(self.runtime),
                     "returncode": result.returncode if result is not None else None,
                     "stdout": bounded(result.stdout if result is not None else getattr(error, "stdout", None)),
                     "stderr": bounded(result.stderr if result is not None else getattr(error, "stderr", None)),
                     "failure_type": type(error).__name__, "failure": str(error)[:4096]}
            with path.open("x", encoding="utf-8") as output:
                json.dump(value, output, indent=2)
            error.add_note(f"Bounded exact failed-read evidence: {path}")
        except BaseException as diagnostic_error:
            error.add_note(f"Failed to retain diagnostic: {type(diagnostic_error).__name__}: {diagnostic_error}")
'''
assert helper.count(old) == 1
helper = helper.replace(old, new)
old_init = '        self.job_timeout = job_timeout\n'
assert helper.count(old_init) == 1
helper = helper.replace(old_init, old_init + '        self.diagnostic_lock = threading.Lock()\n        self.diagnostic_count = 0\n')
driver = bases[names[0]].decode('utf-8').replace('\r\n', '\n')
driver = driver.replace('from contextlib import ExitStack\n', 'from contextlib import ExitStack, contextmanager\n')
marker = '\ndef main():\n'
retention = '''
@contextmanager
def retained_usage_fixture():
    temporary = tempfile.TemporaryDirectory(prefix="airk-usage-inputs-")
    try:
        yield temporary.name
    except BaseException as error:
        # ExitStack has already drained the exact owned service. Retain only
        # this fictional fixture; no normal installation/state is copied.
        temporary._finalizer.detach()
        error.add_note(f"Owned fictional usage fixture retained: {temporary.name}")
        print(json.dumps({"status": "failed", "retained_fixture": temporary.name,
                          "diagnostics": "failed-read-*.json plus original service.stderr/control logs"}),
              flush=True)
        raise
    else:
        temporary.cleanup()

'''
assert driver.count(marker) == 1
driver = driver.replace(marker, '\n' + retention + marker)
old_context = 'with tempfile.TemporaryDirectory(prefix="airk-usage-inputs-") as temporary, ExitStack() as stack:'
assert driver.count(old_context) == 1
driver = driver.replace(old_context, 'with retained_usage_fixture() as temporary, ExitStack() as stack:')
outputs = {names[0]: driver, names[1]: helper}
manifest = {'status': 'diagnostics_only_not_executed', 'files': []}
for name, content in outputs.items():
    ast.parse(content, filename=name)
    path = OUT / name
    path.write_text(content, encoding='utf-8', newline='\n')
    manifest['files'].append({'path': name,
        'live_base_sha256': hashlib.sha256(bases[name]).hexdigest(),
        'diagnostic_sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
# Assert the full workload from its first operation through its final assertions
# remains byte-equivalent modulo EOL. Only the outer temp context is replaced.
assert driver[driver.index('        indexed = run("index")'):] == bases[names[0]].decode('utf-8').replace('\r\n', '\n')[bases[names[0]].decode('utf-8').replace('\r\n', '\n').index('        indexed = run("index")'):]
manifest['preserved'] = ['original read assertions/classifier and deadlines', 'retry=False/admission_only=True',
    'max_workers=4/range(40)', 'writer exclusion lifetime', 'all usage counters and exact call-count assertions',
    'all workload code from indexed=run(index) to original main entry point', 'success cleanup',
    'service-owned shutdown and drain']
manifest['limits'] = ['No runtime execution or root cause claim',
    'No whole-query retry or mutation replay', '32 diagnostics maximum, each stdout/stderr first64KiB plus truncation labels',
    'Failed fixture retained in original OS temp location with original ACL; no normal state copied',
    'Timing gains small diagnostic writes only on failure; no pre-failure subprocess/workload changes']
(OUT / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print(json.dumps(manifest, indent=2))
