# SPDX-License-Identifier: Apache-2.0
import hashlib, json, re, shutil
from pathlib import Path
base = Path(__file__).resolve().parent
out = base / 'windows-r1'
target = base.parents[1] / 'artifacts/v190/p10-target/cargo-timings'
saved = base / 'cargo-timings'
saved.mkdir(exist_ok=False)
reports = {}
commands = []
for log in sorted(out.glob('*.log')):
    names = sorted(set(re.findall(r'Timing report saved to ([^\r\n]+\.html)', log.read_text(encoding='utf-8', errors='replace'))))
    if not names: continue
    receipt = log.with_suffix('.receipt.json')
    assert receipt.is_file(), str(log)
    record = json.loads(receipt.read_text(encoding='utf-8'))
    copied = []
    for name in names:
        path = Path(name)
        assert path.resolve().is_relative_to(target.resolve()) and path.is_file(), name
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if path.name not in reports:
            shutil.copyfile(path, saved / path.name)
            reports[path.name] = digest
        else: assert reports[path.name] == digest
        copied.append(path.name)
    commands.append({'log': str(log), 'command': record['command'], 'target': record['target'],
        'incremental': record['cargo_incremental'], 'seconds': record['seconds'], 'exit_code': record['exit_code'], 'reports': copied})
assert reports and commands
with (base / 'cargo-timings-r1.json').open('x', encoding='utf-8') as stream:
    json.dump({'reports': reports, 'commands': commands, 'qualification': 'Observed existing runs only; no new measurement or build executed.'}, stream, indent=2)
print(json.dumps({'timing_reports_retained': len(reports), 'commands': len(commands), 'directory': str(saved)}))
