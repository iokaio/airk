# SPDX-License-Identifier: Apache-2.0
import difflib
import hashlib
import io
import json
from pathlib import Path
import tarfile

root = Path(__file__).resolve().parent
old = Path("C:/Users/tdotn/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/rustls-0.23.45.crate")

def read(path, version):
    result = {}
    with tarfile.open(path, "r:gz") as tf:
        for member in tf:
            if member.isdir():
                continue
            assert member.isfile(), (member.name, member.type)
            prefix = f"rustls-{version}/"
            assert member.name.startswith(prefix)
            name = member.name[len(prefix):]
            assert name and ".." not in Path(name).parts and not Path(name).is_absolute()
            result[name] = tf.extractfile(member).read()
    return result

a, b = read(old, "0.23.45"), read(root / "rustls-0.23.46.crate", "0.23.46")
names = sorted(n for n in a.keys() | b.keys() if a.get(n) != b.get(n))
diff = []
for name in names:
    diff.extend(difflib.unified_diff(a.get(name, b"").decode().splitlines(True), b.get(name,b"").decode().splitlines(True), "0.23.45/"+name,"0.23.46/"+name))
(root/"full-source.diff").write_text("".join(diff), encoding="utf-8", newline="\n")
inventory=[{"path":n,"bytes":len(data),"sha256":hashlib.sha256(data).hexdigest()} for n,data in sorted(b.items())]
(root/"packaged-files.json").write_text(json.dumps(inventory,indent=2)+"\n",encoding="utf-8")
print(json.dumps({"old_files":len(a),"new_files":len(b),"changed_files":names,"diff_lines":len("".join(diff).splitlines()),"vcs":json.loads(b['.cargo_vcs_info.json'])},indent=2))
