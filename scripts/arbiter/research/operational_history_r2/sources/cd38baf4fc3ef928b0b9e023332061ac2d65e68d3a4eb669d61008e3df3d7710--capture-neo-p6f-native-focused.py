# SPDX-License-Identifier: Apache-2.0
"""Capture only after the primary owner explicitly releases a coherent source freeze."""
import hashlib
import importlib.util
import io
import json
import re
import subprocess
import sys
import tarfile
from pathlib import Path

root = Path(__file__).resolve().parents[2]
label = sys.argv[1]
components_only = sys.argv[2:] == ['--components-only']
assert not sys.argv[2:] or components_only
assert re.fullmatch(r"p6f-focused-[a-z0-9-]+", label)
base = root / "artifacts/v190" / label
base.mkdir(exist_ok=False)
spec = importlib.util.spec_from_file_location("native_builder", root / "scripts/build-service-runtime.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
git = ["git", "-c", f"safe.directory={root.as_posix()}"]
native_before = None if components_only else builder.source_identity()
selection = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo", ".config/nextest.toml", "crates", "third_party", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "scripts/build-service-runtime.py", "scripts/provision-onnx-runtime.py", "scripts/provision-local-model.py", "scripts/int8-policy.py", "scripts/macho-identity.py", "scripts/license-notices.py", "docs/research/contracts-v190/fixtures.json"]
names = subprocess.check_output(git + ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", *selection], cwd=root).split(b"\0")
files = sorted({item.decode() for item in names if item})
def consumed(name):
    return name.startswith(('crates/airk-core/', 'crates/airk-store/', '.cargo/')) or name in {'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.config/nextest.toml', 'docs/research/contracts-v190/fixtures.json'} or name.startswith('crates/') and name.endswith('/Cargo.toml')
consumed_names = [name for name in files if consumed(name)]
consumed_before = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in consumed_names if (root / name).is_file()}
manifest = {}
missing = []
archive = base / "source.tar.gz"
with tarfile.open(archive, "w:gz") as stream:
    for name in files:
        path = root / name
        assert not path.is_symlink() and path.resolve().is_relative_to(root), name
        if not path.exists():
            missing.append(name)
            continue
        assert path.is_file(), name
        body = path.read_bytes()
        manifest[name] = hashlib.sha256(body).hexdigest()
        entry = tarfile.TarInfo(name)
        entry.size = len(body)
        entry.mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
        entry.mtime = int(path.stat().st_mtime)
        stream.addfile(entry, io.BytesIO(body))
if components_only:
    after_names = subprocess.check_output(git + ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", *selection], cwd=root).split(b"\0")
    assert sorted({item.decode() for item in after_names if item and consumed(item.decode())}) == consumed_names, "component input inventory changed during capture"
    assert all(manifest.get(name) == digest and hashlib.sha256((root / name).read_bytes()).hexdigest() == digest for name, digest in consumed_before.items()), "consumed component inputs changed during capture"
else:
    assert builder.source_identity() == native_before, "source changed during capture"
    assert all(hashlib.sha256((root / name).read_bytes()).hexdigest() == digest for name, digest in manifest.items()), "snapshot inputs changed during capture"
component_digest = hashlib.sha256()
for name, digest in sorted(consumed_before.items()):
    raw = name.encode()
    component_digest.update(len(raw).to_bytes(8, 'big'))
    component_digest.update(raw)
    component_digest.update(bytes.fromhex(digest))
component_source = {'sha256': component_digest.hexdigest(), 'files': len(consumed_before), 'boundary': 'Core/Store files, all workspace Cargo manifests, lock/toolchain/config and Core contract fixture; other copied sources are not qualified'}
record = {"schema": "airk.private-focused-native-snapshot/1", "label": label, "entry_commit": subprocess.check_output(git + ["rev-parse", "HEAD"], cwd=root, text=True).strip(), "native_source": native_before, "component_source": component_source, "consumed_files": consumed_before, "files": manifest, "missing": missing, "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(), "scope": "Core and selected Store Unix setup/quota tests only; not full P6 qualification", "git_metadata_in_archive": False}
(base / "snapshot.json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps({"directory": str(base), "native_source": native_before, "component_source": component_source, "files": len(manifest), "missing": missing, "archive_sha256": record["archive_sha256"]}))
