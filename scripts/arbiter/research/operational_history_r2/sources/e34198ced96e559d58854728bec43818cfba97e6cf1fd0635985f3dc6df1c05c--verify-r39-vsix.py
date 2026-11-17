# SPDX-License-Identifier: Apache-2.0
import hashlib,json,zipfile
from pathlib import Path

base=Path(__file__).resolve().parent
publish=base/'windows-debug-r39-vsix-final/publish'
native=base/'windows-debug-r39-native/publish'
runtime=base/'windows-debug-r39/runtime'
sha=lambda data:hashlib.sha256(data).hexdigest()
manifest=json.loads((publish/'manifest.json').read_text())
vsix=publish/manifest['archive']
nm=json.loads((native/'manifest.json').read_text())
install=json.loads((runtime/'airk-install.json').read_text())
build=json.loads((runtime/'airk-build.json').read_text())
qualified=json.loads((base/'clean-break-r39-extension-inputs-before-r1.json').read_text())
assert manifest['version']=='1.8.16' and manifest['pre_release'] is True
assert sha(vsix.read_bytes())==manifest['archive_sha256']
assert build['source']==json.loads((base/'clean-break-r39-reviewed-inputs-r1.json').read_text())['native_source']
normalized={('readme.md' if n=='README.md' else 'LICENSE.txt' if n=='LICENSE' else n):h for n,h in manifest['payload'].items()}
outputs=[]
with zipfile.ZipFile(vsix) as z:
    names=z.namelist()
    assert len(names)==len(set(names))==len(normalized)+2
    assert set(names)=={'extension/'+n for n in normalized}|{'[Content_Types].xml','extension.vsixmanifest'}
    for name,digest in normalized.items():
        data=z.read('extension/'+name)
        assert sha(data)==digest,name
        if name.startswith('out/'):
            assert sha(data)==qualified['compiled_files']['packages/vscode/'+name]['sha256'],name
            outputs.append(name)
    for name,digest in install['files'].items():
        assert sha(z.read('extension/native/runtime/'+name))==digest,name
    assert z.read('extension/native/runtime/airk-install.json')==(runtime/'airk-install.json').read_bytes()
    assert json.loads(z.read('extension/package.json'))['version']=='1.8.16'
    assert z.read('extension/native/candidate.json')==(native/'manifest.json').read_bytes()
assert len(outputs)==89
assert manifest['native_archive_sha256']==nm['archive_sha256']
archive=native/nm['archive']
assert sha(archive.read_bytes())==nm['archive_sha256']
with zipfile.ZipFile(archive) as z:
    assert set(z.namelist())==set(nm['payload'])
    for name,digest in nm['payload'].items():assert sha(z.read(name))==digest,name
proof={'status':'VERIFIED_FROZEN_VSIX','vsix':str(vsix),'sha256':manifest['archive_sha256'],'bytes':vsix.stat().st_size,'zip_members':len(names),'production_outputs_identical_to_qualified_roster':len(outputs),'sealed_runtime_files':len(install['files']),'native_source':build['source'],'native_zip_sha256':nm['archive_sha256'],'main_sha256':sha((runtime/'airk.exe').read_bytes()),'install_manifest_sha256':sha((runtime/'airk-install.json').read_bytes()),'qualification':'Complete archive and qualified-input proof; installed editor remains separate'}
with (base/'clean-break-r39-package-proof-r1.json').open('x') as stream:json.dump(proof,stream,indent=2)
print(json.dumps(proof))
