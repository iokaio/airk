# SPDX-License-Identifier: Apache-2.0
"""Apply public staged patches to owned copies before touching the checkout."""
from pathlib import Path
import hashlib
import json
import re
import shutil
import subprocess
import uuid

base = Path(__file__).resolve().parent
root = base.parents[1]
names = [
    'transport.patch', 'changes-verification.patch', 'combined-retry.patch',
    'scope-filters/query-scope-installed.patch', 'mcp-schemas-context-search.patch',
    'scope-filters/open-filter-diagnostics.patch', 'scope-filters/relationship-limits.patch',
    'unified-diagnostic-names.patch', 'lexical-snippet.patch', 'lexical-guidance.patch',
    'capabilities-summary.patch', 'capabilities-transport.patch', 'transport-ts.patch',
]
out = base / ('composed-review-' + uuid.uuid4().hex[:10])
out.mkdir()
copies = out / 'patches'
copies.mkdir()
tree = out / 'tree'
tree.mkdir()
sha = lambda data: hashlib.sha256(data).hexdigest()
files = set()
manifest = {'status': 'preparing', 'patches': [], 'files': {}}
for number, name in enumerate(names):
    raw = (base / name).read_bytes()
    copy = copies / f'{number:02d}.patch'
    copy.write_bytes(raw)
    manifest['patches'].append({'path': name, 'sha256': sha(raw), 'copy': str(copy)})
    for line in raw.decode('utf-8').splitlines():
        if line.startswith(('--- a/', '+++ b/')):
            path = Path(line[6:])
            assert not path.is_absolute() and '..' not in path.parts
            assert not {'temp', 'private', '.git', 'artifacts'} & set(path.parts)
            files.add(path)
for path in sorted(files):
    destination = tree / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = root / path
    if source.exists():
        assert source.is_file() and not source.is_symlink()
        shutil.copyfile(source, destination)
        manifest['files'][path.as_posix()] = {'original_sha256': sha(source.read_bytes())}
    else:
        manifest['files'][path.as_posix()] = {'original_sha256': None}
for row in manifest['patches']:
    # Independent test additions can share the same original EOF anchor. Merge
    # only proven pure EOF additions; all edits/deletions still use git's exact
    # context application. Preserve the original patch and record this merge.
    lines = Path(row['copy']).read_text(encoding='utf-8').splitlines(keepends=True)
    sections = []
    eof_additions = []
    at = 0
    while at < len(lines):
        while at < len(lines) and not lines[at].startswith('--- '):
            assert lines[at].startswith(('diff --git ', 'index ', 'new file mode 100644', '\n')), lines[at]
            at += 1
        if at == len(lines):
            break
        assert lines[at].startswith('--- '), lines[at]
        end = next((n for n in range(at + 2, len(lines)) if lines[n].startswith(('--- ', 'diff --git '))), len(lines))
        section = lines[at:end]
        path = Path(section[1][6:].strip())
        original_path = root / path
        original_lines = original_path.read_text(encoding='utf-8').splitlines(keepends=True) if original_path.is_file() else []
        retained = section[:2]
        pos = 2
        while pos < len(section):
            header = re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@', section[pos])
            assert header, section[pos]
            next_hunk = next((n for n in range(pos + 1, len(section)) if section[n].startswith('@@ ')), len(section))
            hunk = section[pos:next_hunk]
            old_start, old_count = int(header[1]), int(header[2] or 1)
            old = [line[1:] for line in hunk[1:] if line.startswith((' ', '-'))]
            pure_eof = original_path.is_file() and old_count > 0 and old_start - 1 + old_count == len(original_lines) \
                and len(old) == old_count and old == original_lines[old_start-1:] \
                and not any(line.startswith('-') for line in hunk[1:])
            if pure_eof:
                addition = ''.join(line[1:] for line in hunk[1:] if line.startswith('+'))
                if addition:
                    eof_additions.append((path, addition))
            else:
                retained.extend(hunk)
            pos = next_hunk
        if len(retained) > 2:
            sections.extend(retained)
        at = end
    effective = Path(row['copy']).with_suffix('.effective.patch')
    effective.write_text(''.join(sections), encoding='utf-8', newline='\n')
    command = ['git', '-c', f'safe.directory={root.as_posix()}', 'apply',
               '--ignore-whitespace', '--directory=' + tree.relative_to(root).as_posix(), str(effective)]
    result = subprocess.run(command, cwd=root, text=True, encoding='utf-8', capture_output=True) if sections else subprocess.CompletedProcess(command, 0, '', '')
    row.update(exit_code=result.returncode, stderr=result.stderr)
    if result.returncode:
        manifest['status'] = 'failed'
        (out / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
        print(json.dumps({'status': 'failed', 'patch': row['path'], 'error': result.stderr, 'out': str(out)}))
        raise SystemExit(result.returncode)
    for path, addition in eof_additions:
        destination = tree / path
        current = destination.read_text(encoding='utf-8')
        assert addition not in current, ('duplicate EOF addition', path)
        destination.write_text(current + addition, encoding='utf-8', newline='\n')
    row['merged_eof_additions'] = [path.as_posix() for path, _ in eof_additions]
for path in sorted(files):
    destination = tree / path
    manifest['files'][path.as_posix()]['proposed_sha256'] = sha(destination.read_bytes()) if destination.is_file() else None
manifest['status'] = 'composed'
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
print(json.dumps({'status': 'composed', 'out': str(out), 'files': len(files), 'patches': len(names)}))
