# Independent Mac source copy: prepared commands, not executed

The read-only inventory observed original repository
/Users/tylerjensen/Code/airk at detached7ca445f8c56194243a99f314d693df22f5923197,
with mixed user/campaign modifications. It is the only Git worktree. Its index
SHA remained1fee8695db65ebfff52dc5ed652bddef144c7f6c293e5b4d122da80f3ddfcc64
before/after inventory. Proposed destination
/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1 did not exist.
No Git clone, source transfer or source mutation has happened. Subsequently,
root authorized and this owner completed only the isolated Node/model
prerequisites below; their receipts retain the actual outcome.

Use an independent `git clone --no-hardlinks --no-checkout`, not worktree-add,
shared object storage, reset/clean of the original, or an in-place source overlay.
The earlier campaign apply-source scripts deliberately write Code/airk and are
not invoked for this preservation requirement. A fresh independent .git owns
all fetch/checkout/index changes; the original supplies objects read-only.

## Exact existing inputs

- Python: /Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13,
  version3.13.18, SHA40cc9e3e819af266d931e7313a66e132a9a2c0efb401b545fe2560487874fb88.
- ORT environment: /Users/tylerjensen/Code/airk/artifacts/advanced-split/ort130-arm64-env.json,
  SHA45282be883e6b5a5505fe747466f685bf3f7c8a3a0aac50987182d03181f0b39.
- ORT_LIB_LOCATION: /Users/tylerjensen/Code/airk/artifacts/advanced-split/ort130-arm64/aggregate.
  Keep this exact external immutable cache path; do not relocate/rewrite producer receipts.
- ORT build receipt: SHA08b752a2dc3ed54fcd6ba2c7ba5d9282e89cf2ff7cf7a2b1bc2d847ca30d2fb6;
  aggregate receipt: SHAd7484c307138fe41ff25de7da64328b296549d674ee1518eb8acfd0dee9457e8.
- Existing public DeveloperID: Developer ID Application: Ioka LLC (5PX7D99PXA),
  exact identity SHA1 4D8496DCAB73794378194D040D9177FB28A0F982;
  keychain /Users/tylerjensen/Library/Keychains/airk-signing.keychain-db.
  Public identity inventory does not establish unlocked signing/notary access.
- Current pinned MiniLM source directory on Windows:
  D:/code/Github/airk/artifacts/advanced-split/bundled-model-v2.
  U8 ONNX e7399e81737d3f9707a304be116a229983bc9fff0974d1b087293789021d2d7b;
  S8 ONNX4788597b6d26061b778ac6255723d4bb004e4927446c480ecaebe19e4ea7f47c;
  tokenizer be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037;
  LICENSE9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd.
  The old checkout's bundled-model-v2 location is absent. The exact original
  eight files are now authenticated and copied to
  /Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1/inputs/bundled-model-v2.
  neo-model-transfer.json and neo-model-verification.json retain the actual
  11.785second transfer and0.627second verification; archive SHA
  201bc15ee2b04ef4da530cf9d98254c52c597a23e8238abaae7c7809c88b63b0.
  All eight sizes/hashes and the local original source/catalog still match.
  New parent-bag exports are a different identity and are not substituted here.
- Pinned Node24.15.0/npm11.12.1 are now provisioned at
  /Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1/tools/node-v24.15.0-darwin-arm64/bin/node.
  Actual official checksum and archive match
  372331b969779ab5d15b949884fc6eaf88d5afe87bde8ba881d6400b9100ffc4;
  Node executable SHA3200fbd9f7fd4410426dd541e10d1ab829d3472f270d743c7fabd1696c03fe32.
  neo-node-provision.json retains the7.392second actual successful command and
  original Git-index preservation. The unrelated Homebrew26.3.1 is unchanged.

Full observed identities and absence evidence are in neo-isolation-readonly.json
and neo-node-assets-readonly.json. No new full static closure rehash is needed
during preparation; final canonical native builders still validate their inputs.

## Freeze and prepare one local transfer packet after root review

Before this step root completes Help/native schema/UI generation and freezes the
coherent source. The current local Git index has no staged diff; assert it still
has none. This keeps the new clone's HEAD index roster equivalent, including
tracked-but-deleted paths, without copying a platform Git index or inventing a
commit. Never stage or commit user changes merely to satisfy this assumption.

The following local Python is a preparation command, not an invocation already
performed. It uses the existing receipt/source owners and writes only a fresh
private packet. Invoke it through a reviewed temporary file or stdin with normal
PowerShell quoting. It copies no credentials, caches, private artifacts or live
state. The four existing source groups plus docs/root guidance cover native,
frontend, script, fixture and authored Help inputs.

```python
import hashlib, importlib.util, io, json, os, subprocess, tarfile
from pathlib import Path
root = Path('D:/code/Github/airk')
out = root/'artifacts/advanced-split/mac-delivery-transfer-r1'
git = ['git', '-c', 'safe.directory='+root.as_posix()]
def command(*args):
    return subprocess.check_output([*git, *args], cwd=root)
assert subprocess.run([*git, 'diff', '--cached', '--quiet'], cwd=root).returncode == 0
head = command('rev-parse', 'HEAD').decode().strip()
out.mkdir(exist_ok=False)
def load(name, file):
    spec = importlib.util.spec_from_file_location(name, root/file)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module
receipt = load('delivery_receipt', 'scripts/dev/receipt_runner.py')
runtime = load('delivery_runtime', 'scripts/build-service-runtime.py')
groups = ['native', 'harness', 'fixtures', 'frontend']
before = receipt.observe(groups)
files = {name: value for group in before.values() for name, value in group['files'].items()}
for path in receipt.tracked_files(('docs', 'README.md', 'CONTRIBUTING.md', 'AGENTS.md', 'CLAUDE.md')):
    files[path.relative_to(root).as_posix()] = receipt.digest(path) if path.is_file() else 'missing'
rows = {}
source_identity = runtime.source_identity()
with tarfile.open(out/'public-source.tar.gz', 'w:gz') as archive:
    for name, expected in sorted(files.items()):
        path = root/name
        assert not set(Path(name).parts) & {'..', '.git', 'artifacts', 'temp', '.airk', 'node_modules', 'target'}
        assert not path.is_symlink()
        if expected == 'missing':
            rows[name] = {'sha256': 'missing'}; continue
        data = path.read_bytes(); assert hashlib.sha256(data).hexdigest() == expected
        row = tarfile.TarInfo(name); row.size = len(data); row.mode = 0o644
        archive.addfile(row, io.BytesIO(data))
        rows[name] = {'sha256': expected, 'bytes': len(data)}
assert receipt.observe(groups) == before
assert runtime.source_identity() == source_identity
assert command('rev-parse', 'HEAD').decode().strip() == head
assert subprocess.run([*git, 'diff', '--cached', '--quiet'], cwd=root).returncode == 0
subprocess.run([*git, 'bundle', 'create', str(out/'source.bundle'), 'HEAD',
                '^7ca445f8c56194243a99f314d693df22f5923197'], cwd=root, check=True)
assert command('rev-parse', 'HEAD').decode().strip() == head
packet = {'head': head, 'source_identity': source_identity, 'groups': before, 'files': rows,
          'public_archive_sha256': receipt.digest(out/'public-source.tar.gz'),
          'git_bundle_sha256': receipt.digest(out/'source.bundle')}
(out/'source.json').write_text(json.dumps(packet, indent=2)+'\n', encoding='utf-8')
print(json.dumps({'head': head, 'source_identity': source_identity,
                  'source_json_sha256': receipt.digest(out/'source.json'),
                  'archive_sha256': packet['public_archive_sha256'],
                  'bundle_sha256': packet['git_bundle_sha256']}))
```

Root reviews the actual file/deletion roster and hashes printed by that completed
preparation. If a source input moves during preparation, retain failure and
diagnose it; do not copy a mixture or reuse a stale packet. The current catalog
model directory already exists in the isolated prerequisite root; no duplicate
model transfer is needed. No checkpoint, training/evaluation corpus, credentials
or private service state was copied.

## Root-reviewed transport and fresh independent copy

Transport only source.json, public-source.tar.gz and source.bundle into the fresh
sibling source transfer directory. Model inputs are already verified separately.
Use the existing ignored
SSH connection JSON without printing its contents. The existing reviewed pattern
is subprocess argv with ssh_options, -i identity_path, -p port, user@host and
`sh -s`; no credential-bearing command line is included in receipts. Preserve
the SSH exit, exact stdin/script SHA and copied-file hashes. No existing checkout
is an extraction destination.

The following is the exact POSIX-sh sequence after root has checked transferred
hashes against the reviewed packet and verified no compiler descendants are
active. SOURCE_JSON_SHA256 must be the actual local printed hash, not a guess.
It is intentionally guarded until filled. It runs no Cargo, models or signing.

```sh
set -eu
umask 077
original=/Users/tylerjensen/Code/airk
delivery=/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1
transfer=/Users/tylerjensen/airk-builds/advanced-split-local-delivery-transfer-r1
python=/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13
expected_source_json=SOURCE_JSON_SHA256
[ "$expected_source_json" != SOURCE_JSON_SHA256 ] || exit 21
[ ! -e "$delivery" ] || exit 22
[ "$(git -C "$original" rev-parse HEAD)" = 7ca445f8c56194243a99f314d693df22f5923197 ] || exit 23
"$python" -I -B - "$transfer" "$expected_source_json" <<'PY'
import hashlib,json,sys
from pathlib import Path
folder=Path(sys.argv[1]); raw=(folder/'source.json').read_bytes()
assert hashlib.sha256(raw).hexdigest()==sys.argv[2]
packet=json.loads(raw)
for name,key in [('source.bundle','git_bundle_sha256'),('public-source.tar.gz','public_archive_sha256')]:
    assert hashlib.sha256((folder/name).read_bytes()).hexdigest()==packet[key]
PY
mkdir "$delivery"
git clone --no-hardlinks --no-checkout "$original" "$delivery/source"
git -C "$delivery/source" config core.autocrlf false
git -C "$delivery/source" fetch "$transfer/source.bundle" HEAD
git -C "$delivery/source" checkout --detach FETCH_HEAD
"$python" -I -B - "$delivery/source" "$transfer" <<'PY'
import hashlib,importlib.util,json,os,stat,subprocess,sys,tarfile
from pathlib import Path,PurePosixPath
root=Path(sys.argv[1]).resolve(); transfer=Path(sys.argv[2]); packet=json.loads((transfer/'source.json').read_bytes())
assert root==Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1/source')
assert root!=Path('/Users/tylerjensen/Code/airk').resolve()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==packet['head']
with tarfile.open(transfer/'public-source.tar.gz','r:gz') as archive:
    expected={name:row for name,row in packet['files'].items() if row['sha256']!='missing'}
    members=archive.getmembers(); assert len(members)==len(expected) and {m.name for m in members}==set(expected)
    payload={}
    for member in members:
        name=PurePosixPath(member.name)
        assert not name.is_absolute() and not set(name.parts)&{'..','.git','artifacts','temp','.airk','node_modules','target'}
        assert member.isfile() and not member.issym() and not member.islnk()
        data=archive.extractfile(member).read(); row=expected[member.name]
        assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
        payload[member.name]=data
    for name,row in packet['files'].items():
        relative=PurePosixPath(name)
        assert not relative.is_absolute() and not set(relative.parts)&{'..','.git','artifacts','temp','.airk','node_modules','target'}
        path=root/name; assert path.is_relative_to(root)
        assert all(not part.is_symlink() for part in (path,*path.parents))
        if path.exists():assert path.is_file() and stat.S_ISREG(path.stat().st_mode) and path.stat().st_nlink==1
        if row['sha256']=='missing':
            if path.exists():path.unlink()
        else:
            path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(payload[name])
for name,row in packet['files'].items():
    path=root/name
    assert (not path.exists()) if row['sha256']=='missing' else hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256']
os.chdir(root)
spec=importlib.util.spec_from_file_location('copied_runtime',root/'scripts/build-service-runtime.py')
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
assert owner.source_identity()==packet['source_identity']
print(json.dumps({'independent_source_verified':True,'source_identity':packet['source_identity']}))
PY
```

Do not run git add/reset/clean on the original or the copy as a workaround for
a source-identity mismatch. Retain the exact copied state and diagnose the
first mismatch. The checked-out tracked executable modes remain from Git;
ordinary source files are overwritten in place in the fresh copy only. Compare
the original checkout's HEAD, index SHA and status with its pre-transfer values
afterwards to retain preservation proof. No source/payload reuse success is
inferred from clone exit0 alone.

## Actual Node/model prerequisites for the independent copy

Root explicitly authorized isolated prerequisite provisioning. The official
versioned Darwin archive and official SHASUMS256.txt were downloaded, authenticated
and extracted successfully into the prerequisite root. No download/extraction
or model copy needs repeating. After the source-copy seal is reviewed and copied,
the following uses those exact assets and the existing current-catalog verifier:

```sh
prerequisites=/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1
node_bin="$prerequisites/tools/node-v24.15.0-darwin-arm64/bin"
[ "$("$node_bin/node" --version)" = v24.15.0 ]
PATH="$node_bin:$HOME/.cargo/bin:$PATH"
export PATH
cd "$delivery/source"
"$python" -I -B scripts/provision-local-model.py --destination "$prerequisites/inputs/bundled-model-v2"
```

The existing provisioner authenticates already copied assets/manifests against
the current catalog, refuses changed inputs and does no NN work. The successful
original eight-file copy is not a model-quality or product-promotion result.
If root promotes a new catalog before
freeze, revise these profile/input identities to its actually qualified closure.

## Subsequent root-only native batch parameters

After source/tool/model receipt review: CARGO_TARGET_DIR is exactly
/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1/source/artifacts/v190/p10-target;
CARGO_INCREMENTAL=0, MACOSX_DEPLOYMENT_TARGET=13.3,
AIRK_BUNDLED_MODEL_DIR=/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1/inputs/bundled-model-v2.
Use the existing external ORT environment file above through its canonical
static-runtime-environment.py owner. Do not path-rewrite or symlink that cache.
Root selects scoped native Clippy/tests before a release pair; no actual Cargo
batch is authorized by this command preparation alone. Seal Admin under pinned
Node before embedding, then build the pair through build-service-runtime.py.

Retain all receipts, logs/timings and deliverable bytes outside this exact target
before root runs cargo clean --target-dir on it after the batch, including a
failed batch. Preserve the original checkout, original target and ORT/tool/model
caches. Signing/notarization and package/native journeys are later separate root
steps; arm64 results establish no Intel/oldest-OS or release qualification.
