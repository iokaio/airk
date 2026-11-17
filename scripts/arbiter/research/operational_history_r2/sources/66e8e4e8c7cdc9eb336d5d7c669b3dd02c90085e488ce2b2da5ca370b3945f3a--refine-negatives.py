# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/actual-host-qualification/source/packages/vscode/src/test/f09NativeQualification.test.ts')
s=p.read_text(encoding='utf-8');needle="        assert.equal(direct.namespace,record(before.lifecycle).namespace);"
s=s.replace(needle,needle+'''
        // Negative/unknown metadata never becomes registration or a retry loop.
        const missingRoot=path.join(fixture.owner,'scope-never-created');
        const missing=await realHost.sourceScope(binding,missingRoot,null);
        assert.equal(missing.outcome,'missing');assert.equal(missing.registration,null);
        assert.equal(missing.namespace,direct.namespace);
        await assert.rejects(fs.stat(missingRoot),{code:'ENOENT'});
        await assert.rejects(realHost.sourceScope({...binding,runtimeSha256:'00'.repeat(32)},spec.root,spec.collection),/source scope runtime changed/);
        await assert.rejects(realHost.sourceScope({...binding,controlDir:path.join(fixture.owner,'wrong-control')},spec.root,spec.collection),/source scope control changed/);
''')
p.write_text(s,encoding='utf-8',newline='\n')
