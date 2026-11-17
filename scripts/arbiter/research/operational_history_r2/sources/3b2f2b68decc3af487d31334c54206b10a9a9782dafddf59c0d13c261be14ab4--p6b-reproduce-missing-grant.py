# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import queue
import struct
import subprocess
import tempfile
import threading
import uuid

binary = Path('artifacts/v190/p6a-qualified-runtime/airk-embed.exe').resolve(strict=True)
with binary.open('rb') as stream:
    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
assert digest == 'd0e6076126f9936ed77f10de4c3022adc21194de65b5541e1e51563443afa8fe'
with tempfile.TemporaryDirectory(prefix='airk-p6b-grant-repro-') as temporary:
    with (Path(temporary) / 'stderr.log').open('wb') as stderr:
        child = subprocess.Popen([str(binary), 'pool'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
        replies = queue.Queue(maxsize=8)
        def read():
            try:
                while prefix := child.stdout.read(4):
                    assert len(prefix) == 4
                    length, = struct.unpack('>I', prefix)
                    assert 0 < length <= 131072
                    data = child.stdout.read(length)
                    assert len(data) == length
                    replies.put_nowait(json.loads(data))
            except BaseException as error:
                replies.put_nowait({'reader_error': type(error).__name__})
        reader = threading.Thread(target=read, daemon=True)
        reader.start()
        def send(value):
            data = json.dumps(value).encode()
            child.stdin.write(struct.pack('>I', len(data)) + data)
            child.stdin.flush()
        try:
            instance = uuid.uuid4().hex
            send(dict(runtime=str(Path(temporary) / 'runtime'), service=uuid.uuid4().hex,
                      pool=instance, inference_threads=1))
            ready = replies.get(timeout=15)
            assert ready['status'] == 'ready' and ready['pool'] == instance
            send({'renew': {'worker': uuid.uuid4().hex}})
            renewal = replies.get(timeout=10)
            assert renewal == {'status': 'failed'}
            send('status')
            observed = replies.get(timeout=10)
            assert observed['status'] == 'status' and child.poll() is None
            send('stop')
            child.stdin.close()
            exit_code = child.wait(timeout=20)
            assert exit_code == 0
            result = dict(schema='airk.private-helper-reproduction/1', binary_sha256=digest,
                          missing_grant_reply=renewal, subsequent_status=observed,
                          helper_remained_healthy=True, explicit_stop_exit_code=exit_code,
                          limitation='Controlled missing grant; does not diagnose historical unexplained exits')
            Path('artifacts/v190/p6b-missing-grant-before.json').write_text(json.dumps(result, indent=2) + '\n')
            print(json.dumps(result, indent=2))
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=10)
            if not child.stdin.closed:
                child.stdin.close()
            reader.join(timeout=5)
            child.stdout.close()
