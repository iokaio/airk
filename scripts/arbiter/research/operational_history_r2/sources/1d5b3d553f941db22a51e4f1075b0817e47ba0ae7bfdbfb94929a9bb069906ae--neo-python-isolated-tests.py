# SPDX-License-Identifier: Apache-2.0
import importlib.util,io,tarfile
from pathlib import Path
s=importlib.util.spec_from_file_location('tool',Path('artifacts/advanced-split/neo-python-isolated-tool.py'));m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
for name,target in [('python/../escape',None),('python/bin/link','../../outside'),('python/bin/a','a'),('python/bin/a','missing')]:
 buffer=io.BytesIO()
 with tarfile.open(fileobj=buffer,mode='w') as t:
  item=tarfile.TarInfo(name)
  if target is None:item.size=1;t.addfile(item,io.BytesIO(b'x'))
  else:item.type=tarfile.SYMTYPE;item.linkname=target;t.addfile(item)
 buffer.seek(0)
 with tarfile.open(fileobj=buffer) as t:
  try:m.inventory(t)
  except ValueError:pass
  else:raise AssertionError('Unsafe fixture accepted')
with tarfile.open(next(Path('artifacts/advanced-split/tools/neo-python-source').glob('*.gz'))) as t:
 rows=m.inventory(t)
 assert len(rows)==1692
 assert sum(v['kind']=='symlink' for v in rows.values())==9
print('5 inventory cases passed:4refusals+actual1692member pinnedarchive')
