# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import zipfile

root = Path(__file__).resolve().parents[2]
out = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('corpus', root / 'scripts/benchmark-real-corpora.py')
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
task = Path(tempfile.mkdtemp(prefix='airk-final-182-'))
record = {'task': str(task), 'runs': []}
(out / 'location.json').write_text(json.dumps(record, indent=2))
for version, archive, expected_archive, approved in [
    ('1.8.2', 'installation-review-1.8.2-20261101/native-corrected/publish', '0db2ebde8fdb40c2bc835f5369f590bf3fe7f05b620ceb78d5f6636a666d7a25', '6b3942fa9ebd168746281e2ceec8c604913545c10afe95a32ab99fe0cdab8fa7'),
    ('1.7.6', 'local-1.7.6/native/publish', '9febad284f35f13edacb7058084104ba843cc3b7cf9b27d345765c5206a2fa00', '04c57b54ad8ecd8a1b78675eea4a01ad4beafae5e626c24a2b84f6ecdb3b87ec')]:
    package = root / 'artifacts' / archive / f'airk-{version}-x86_64-pc-windows-msvc.zip'
    assert bench.digest(package) == expected_archive
    runtime = task / f'runtime-{version}'
    runtime.mkdir()
    with zipfile.ZipFile(package) as z:
        for name in z.namelist():
            assert not Path(name).is_absolute() and '..' not in Path(name).parts
        z.extractall(runtime)
    bench.verify_runtime(runtime, approved)
    report = task / f'corpus-{version}.json'
    command = ['python', str(root / 'scripts/benchmark-real-corpora.py'), '--runtime-dir', str(runtime),
        '--runtime-sha256', approved, '--airk', str(root), '--bookbuilder', 'D:/code/Github/bookbuilder',
        '--state-parent', str(task), '--report', str(report), '--samples', '30', '--semantic',
        '--excerpt-path', 'README.md', '--command-timeout', '120', '--job-timeout', '3600']
    print(f'Starting {version}', flush=True)
    with (out / f'campaign-{version}.log').open('x') as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
    record['runs'].append({'version': version, 'exit_code': result.returncode,
        'report_sha256': bench.digest(report) if report.exists() else None})
    if report.exists():
        (out / f'corpus-{version}.json').write_bytes(report.read_bytes())
    (out / 'location.json').write_text(json.dumps(record, indent=2))
    print(json.dumps(record['runs'][-1]), flush=True)
