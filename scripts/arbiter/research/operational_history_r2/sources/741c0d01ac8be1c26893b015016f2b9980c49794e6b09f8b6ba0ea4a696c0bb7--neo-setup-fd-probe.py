# SPDX-License-Identifier: Apache-2.0
import json
import os
import platform
import sys
import tempfile
from pathlib import Path

run = Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh')
result = {'platform': platform.platform(), 'machine': platform.machine(),
          'python': sys.version, 'operations': {}, 'cleanup': False}
with tempfile.TemporaryDirectory(prefix='setup-fd-probe-', dir=str(run)) as temp:
    base = Path(temp)
    parent = base / 'parent'
    data = parent / 'data'
    data.mkdir(parents=True)
    (data / 'original.txt').write_bytes(b'original-owned-fixture')
    fd = os.open(str(data), os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        fd_path = '/dev/fd/' + str(fd)
        def child_bytes():
            child = os.open(fd_path + '/original.txt', os.O_RDONLY | os.O_NOFOLLOW)
            try:
                return os.read(child, 1024).decode('ascii')
            finally:
                os.close(child)
        for name, operation in [
                ('read_child', child_bytes),
                ('scandir', lambda: sorted(entry.name for entry in os.scandir(fd_path))),
                ('same_inode', lambda: os.stat(fd_path).st_ino == os.fstat(fd).st_ino)]:
            try:
                result['operations'][name] = {'ok': True, 'value': operation()}
            except OSError as error:
                result['operations'][name] = {'ok': False, 'errno': error.errno, 'message': str(error)}
        parent.rename(base / 'old-parent')
        data.mkdir(parents=True)
        (data / 'replacement.txt').write_bytes(b'unreviewed-replacement')
        for name, operation in [
                ('read_after_parent_replace', child_bytes),
                ('scandir_after_parent_replace', lambda: sorted(entry.name for entry in os.scandir(fd_path)))]:
            try:
                result['operations'][name] = {'ok': True, 'value': operation()}
            except OSError as error:
                result['operations'][name] = {'ok': False, 'errno': error.errno, 'message': str(error)}
        result['replacement_preserved'] = (data / 'replacement.txt').read_bytes() == b'unreviewed-replacement'
    finally:
        os.close(fd)
result['cleanup'] = not base.exists()
print(json.dumps(result, sort_keys=True))
