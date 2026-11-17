# SPDX-License-Identifier: Apache-2.0
"""Private bounded Windows qualification; only an exact owned fixture helper handle is terminated."""
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import time

assert os.name == 'nt'
root = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('clients', root / 'scripts/check-service-clients.py')
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)
runtime = root / 'artifacts/v190/p6b-r4-qualified-runtime'
helper = (runtime / 'airk-embed.exe').resolve(strict=True)
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel.OpenProcess.restype = wintypes.HANDLE
kernel.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
kernel.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
kernel.CloseHandle.argtypes = [wintypes.HANDLE]
receipt = {'schema': 'airk.private-helper-outages/1', 'events': [], 'passed': False,
           'main_sha256': hashlib.sha256((runtime / 'airk.exe').read_bytes()).hexdigest(),
           'helper_sha256': hashlib.sha256(helper.read_bytes()).hexdigest()}

def event(kind, **values):
    row = {'kind': kind, 'elapsed_s': round(time.monotonic()-started, 3), **values}
    receipt['events'].append(row)
    print(json.dumps(row), flush=True)

def wait_for(action, timeout=90):
    deadline = time.monotonic()+timeout
    while time.monotonic() < deadline:
        value = action()
        if value:
            return value
        time.sleep(.25)
    raise AssertionError('bounded native observation deadline expired')

def kill_owned_helper(fixture, observed):
    assert fixture.child.poll() is None and observed['available'] and observed['worker']
    handle = kernel.OpenProcess(0x1000 | 0x100000 | 0x1, False, observed['process_id'])
    assert handle, ctypes.get_last_error()
    try:
        image = ctypes.create_unicode_buffer(32768)
        length = wintypes.DWORD(len(image))
        assert kernel.QueryFullProcessImageNameW(handle, 0, image, ctypes.byref(length))
        assert Path(image.value).resolve(strict=True) == helper, image.value
        current = fixture.rpc('embedding_status')
        assert (current['worker'], current['process_id']) == (observed['worker'], observed['process_id'])
        assert kernel.WaitForSingleObject(handle, 0) == 258
        assert kernel.TerminateProcess(handle, 0xE1230001), ctypes.get_last_error()
        assert kernel.WaitForSingleObject(handle, 10000) == 0
    finally:
        kernel.CloseHandle(handle)

started = time.monotonic()
try:
    with tempfile.TemporaryDirectory(prefix='airk-p6b-outages-') as temporary:
        owner = Path(temporary)
        source, data = owner/'source', owner/'data'
        source.mkdir(); data.mkdir()
        text = 'def copper_beacon():\n    return 42\n'
        (source/'fixture.py').write_text(text)
        with clients.Fixture(runtime, owner) as fixture:
            fixture.register('outages', source, data)
            fixture.job('outages', 'index')
            def lexical():
                result = fixture.tool('outages', 'airk_search', {'query':'copper_beacon','mode':'lexical','response_mode':'full'})
                assert result['hits']
            lexical()
            def available():
                value = fixture.rpc('embedding_status')
                return value if value.get('available') else None
            def saved():
                with sqlite3.connect(fixture.control/'service-1.4.sqlite3') as database:
                    row = database.execute("SELECT failures,next_ms,started_ms,suppressed FROM service_recovery_budget WHERE scope='helper'").fetchone()
                    return row
            initial = wait_for(available)
            worker_method = {'worker': {'project': fixture.projects['outages']['binding']}}
            worker_identity = fixture.rpc(worker_method)['identity']
            assert worker_identity
            identities = [initial['worker']]
            anchor = None
            for attempt in range(1,4):
                current = wait_for(available)
                killed_at = time.monotonic()
                kill_owned_helper(fixture, current)
                state = wait_for(lambda: (row if (row:=saved()) and row[0]==attempt else None), 30)
                anchor = state[2] if anchor is None else anchor
                assert state[2] == anchor and not state[3]
                lexical()
                assert fixture.rpc(worker_method)['identity'] == worker_identity
                event('owned_helper_loss', attempt=attempt, worker=current['worker'], saved=list(state))
                if attempt < 3:
                    replacement = wait_for(available)
                    assert replacement['worker'] not in identities
                    identities.append(replacement['worker'])
                else:
                    assert not available()
                    # Restart the actual owner during cooldown, retaining its durable registry.
                    fixture.rpc('shutdown'); fixture.child.wait(timeout=45)
                    fixture.start(); lexical()
                    worker_identity = fixture.rpc(worker_method)['identity']
                    assert worker_identity
                    assert saved()[2] == anchor
                    replacement = wait_for(available, 95)
                    elapsed = time.monotonic()-killed_at
                    assert elapsed >= 59, elapsed
                    assert replacement['worker'] not in identities
                    identities.append(replacement['worker'])
                    event('cooldown_survived_service_restart', delay_s=elapsed, worker=replacement['worker'])
            # Existing status/worker exchanges establish post-window health; no injected clock.
            last_read = [time.monotonic()]
            def healthy_reset():
                if time.monotonic()-last_read[0] >= 5:
                    lexical()
                    last_read[0] = time.monotonic()
                assert available()
                return saved() is None
            wait_for(healthy_reset, 100)
            event('healthy_reset_observed')
            current = wait_for(available)
            kill_owned_helper(fixture, current)
            state = wait_for(lambda: (row if (row:=saved()) and row[0]==1 else None), 30)
            assert state[2] > anchor
            replacement = wait_for(available)
            event('new_outage_after_healthy_reset', saved=list(state), worker=replacement['worker'])
            stopped = fixture.rpc({'stop_embedding':{'worker':replacement['worker'],'force':True}})
            assert stopped['status']=='embedding_stopped'
            fixture.rpc('shutdown'); fixture.child.wait(timeout=45)
            fixture.start(); lexical()
            assert not available() and saved()[3]
            event('explicit_stop_survived_restart')
            assert fixture.rpc('start_embedding')['status']=='embedding_starting'
            final = wait_for(available)
            lexical()
            assert (source/'fixture.py').read_text()==text
            logs = json.loads(fixture.run('service','logs','--runtime-dir',fixture.control,'--all','--format','json').stdout)
            codes = [record.get('code') for record in logs['records']]
            for code in ['helper_exit_code_3777167361','helper_cooldown_elapsed','helper_healthy_reset','helper_explicit_stop','helper_explicit_start']:
                assert code in codes, (code,codes)
            receipt['diagnostic_codes'] = codes
            event('explicit_start_recovered', worker=final['worker'])
            receipt['passed']=True
finally:
    (root/'artifacts/v190/p6b-native-outages.json').write_text(json.dumps(receipt,indent=2)+'\n')
