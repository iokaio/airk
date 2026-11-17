# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
root=Path('artifacts/optimizations/f09-draft')
for name,patch,expected_digest in [
    ('lazy-discovery','lazy-discovery.patch','726f032c92f9db1eb1e0a4254c4b73e31e38d010fee2e9e712ea1d4663424dd8'),
    ('actual-host-qualification','actual-host-qualification.patch','1d7047565c342e021c11d33a22668e2da5d0f41710d894375a379ba1eb8ce680')]:
    directory=root/name
    # The directory's documented patch filename is selected explicitly below.
    candidates=list(directory.glob('*.patch'))
    assert len(candidates)==1
    patch_path=candidates[0]
    assert hashlib.sha256(patch_path.read_bytes()).hexdigest()==expected_digest
    for file,expected in json.loads((directory/'BASE.json').read_text()).items():
        source=Path(file)
        if expected.get('absent'):
            assert not source.exists(),file
        else:
            assert hashlib.sha256(source.read_bytes()).hexdigest()==expected['sha256'],file
    print(name,'exact patch and BASE verified',str(patch_path))
