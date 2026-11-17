# SPDX-License-Identifier: Apache-2.0
"""Read-only, no-link-following size inventory of this checkout."""
import collections, json, os, pathlib, time, uuid
root = pathlib.Path('D:/code/Github/airk').resolve()
start = time.monotonic()
totals = collections.defaultdict(lambda: [0, 0, 0])
stack = [(root, ())]
errors, targets, entries, links = [], set(), 0, 0
while stack:
    folder, parts = stack.pop()
    try:
        with os.scandir(folder) as children:
            for entry in children:
                entries += 1
                try:
                    metadata = entry.stat(follow_symlinks=False)
                    path = pathlib.Path(entry.path)
                    if entry.is_symlink() or getattr(metadata, 'st_file_attributes', 0) & 0x400:
                        links += 1
                        continue
                    keys = [str(pathlib.PurePosixPath(*parts[:level])) for level in range(1, min(len(parts), 3) + 1)]
                    if entry.is_dir(follow_symlinks=False):
                        child = (*parts, entry.name)
                        totals[str(pathlib.PurePosixPath(*child))][2] += 1
                        for key in keys:
                            totals[key][2] += 1
                        if entry.name == '.fingerprint' and folder.name in {'debug', 'release'}:
                            targets.add(str(folder.parent.relative_to(root)))
                        stack.append((path, child))
                    elif entry.is_file(follow_symlinks=False):
                        if entry.name == '.rustc_info.json':
                            targets.add(str(folder.relative_to(root)))
                        for key in keys or ['.']:
                            totals[key][0] += metadata.st_size
                            totals[key][1] += 1
                except OSError as error:
                    errors.append({'path': str(path.relative_to(root)), 'error': str(error)})
                if entries % 200000 == 0:
                    print(json.dumps({'entries_scanned': entries, 'seconds': round(time.monotonic() - start, 1)}), flush=True)
    except OSError as error:
        errors.append({'path': str(folder.relative_to(root)), 'error': str(error)})
rows = [{'path': path, 'bytes': value[0], 'gib': round(value[0] / 2**30, 3), 'files': value[1], 'directories': value[2]}
        for path, value in totals.items() if len(pathlib.PurePosixPath(path).parts) <= 3]
top = sorted((row for row in rows if len(pathlib.PurePosixPath(row['path']).parts) <= 1), key=lambda row: row['bytes'], reverse=True)
record = {'root': str(root), 'logical_bytes': sum(row['bytes'] for row in top), 'seconds': round(time.monotonic() - start, 2),
          'entries_scanned': entries, 'reparse_links_not_followed': links, 'top_level': top,
          'largest_subdirectories': sorted(rows, key=lambda row: row['bytes'], reverse=True)[:55],
          'cargo_target_candidates': sorted(targets), 'errors': errors}
output = root / 'artifacts/v190' / ('p9-disk-usage-' + uuid.uuid4().hex[:12] + '.json')
output.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'report': str(output), 'logical_gib': round(record['logical_bytes'] / 2**30, 3), 'top_level': top,
                  'largest_subdirectories': record['largest_subdirectories'][:18], 'cargo_target_candidates': sorted(targets),
                  'error_count': len(errors), 'seconds': record['seconds']}), flush=True)
