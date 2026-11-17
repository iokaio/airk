# SPDX-License-Identifier: Apache-2.0
import base64, hashlib, importlib.util, json, os, subprocess, tarfile, time
from pathlib import Path
root=Path.cwd(); base=root/'artifacts/local/1.8.12-explorer-20261107/mac-neo'
git=['git','-c','safe.directory='+root.as_posix()]
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
native=builder.source_identity()
assert native=={'sha256':'51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa','files':978},native
excluded={'.git','.codex','.agents','.aws','.mcp.json','.ssh','temp','target','artifacts','node_modules','__pycache__'}
names=subprocess.check_output([*git,'ls-files','-z','--cached','--others','--exclude-standard']).split(b'\0')
names=sorted({os.fsdecode(n) for n in names if n and not(set(Path(os.fsdecode(n)).parts)&excluded)})
manifest={}; archive=base/'source.tar.gz'
with tarfile.open(archive,'x:gz') as out:
 for name in names:
  p=root/name; assert p.is_file() and not p.is_symlink() and p.resolve().is_relative_to(root),name
  manifest[name]=hashlib.sha256(p.read_bytes()).hexdigest();out.add(p,arcname=name,recursive=False)
assert builder.source_identity()==native
assert all(hashlib.sha256((root/n).read_bytes()).hexdigest()==h for n,h in manifest.items()),'Source changed while snapshot was captured'
head=subprocess.check_output([*git,'rev-parse','HEAD']).decode().strip();tree=subprocess.check_output([*git,'rev-parse','HEAD^{tree}']).decode().strip()
objects={head:('commit',subprocess.check_output([*git,'cat-file','commit',head]))}
trees={tree}
for row in subprocess.check_output([*git,'ls-tree','-r','-t','-z','HEAD']).split(b'\0'):
 if row:
  metadata=row.split(b'\t',1)[0].split()
  if metadata[1]==b'tree':trees.add(metadata[2].decode())
for oid in trees:objects[oid]=('tree',subprocess.check_output([*git,'cat-file','tree',oid]))
record={'schema':'airk.neo-source-snapshot/1','entry_commit':head,'entry_tree':tree,'native_source':native,'files':manifest,'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'source_state':'Exact current public worktree bytes; entry commit is original metadata, not a clean-source claim'}
(base/'snapshot.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8',newline='\n')
(base/'git-metadata.json').write_text(json.dumps({oid:{'type':kind,'base64':base64.b64encode(data).decode()} for oid,(kind,data) in objects.items()}),encoding='utf-8',newline='\n')
print(json.dumps({'public_files':len(manifest),'native_source':native,'archive_bytes':archive.stat().st_size,'git_metadata_objects':len(objects)}))
