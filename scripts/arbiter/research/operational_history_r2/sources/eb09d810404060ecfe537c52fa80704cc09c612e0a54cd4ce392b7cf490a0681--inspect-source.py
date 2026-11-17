# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib
run=pathlib.Path(__file__).resolve().parent;source=run/'source';r=json.loads((run/'snapshot.json').read_text());actual={str(p.relative_to(source)):hashlib.file_digest(p.open('rb'),'sha256').hexdigest() for p in source.rglob('*') if p.is_file() and not p.is_relative_to(source/'.git')}
print(json.dumps({'extra':sorted(actual.keys()-r['public_files'].keys()),'missing':sorted(r['public_files'].keys()-actual.keys()),'changed':[n for n in r['public_files'] if n in actual and actual[n]!=r['public_files'][n]]},indent=2))
