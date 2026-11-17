# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/packages/vscode/src')
f=p/'serviceBootstrap.ts';s=f.read_text(encoding='utf-8');pos=s.index('export interface NativeSourceScope')
s=s[:pos]+'''function sameNativeLocation(left:string,right:string):boolean {
    return path.relative(process.platform==='win32'?path.toNamespacedPath(left):left,
        process.platform==='win32'?path.toNamespacedPath(right):right)==='';
}
'''+s[pos:]
s=s.replace("path.relative(canonical,root)!==''",'!sameNativeLocation(canonical,root)').replace("path.relative(registration.root,canonical)!==''",'!sameNativeLocation(registration.root,canonical)')
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'test/serviceWorkspace.test.ts';s=f.read_text(encoding='utf-8');s+='''
test('Windows exact native source accepts normal/verbatim equivalents and refuses foreign roots',{skip:process.platform!=='win32'},async()=>{
    const root=path.resolve(os.tmpdir(),'airk-f09-normalization'),verbatim=path.toNamespacedPath(root);
    const binding:RuntimeBinding={installDir:path.dirname(root),controlDir:path.join(path.dirname(root),'control'),runtimeSha256:'ab'.repeat(32),version:'1.8.0',executable:path.join(path.dirname(root),'airk.exe'),launcher:path.join(path.dirname(root),'launcher.exe'),lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
    let nativeRoot=verbatim,registeredRoot=root;
    const host=new RuntimeHost(path.dirname(root),'1.8.0',{channel:'stable'},async()=>({install_dir:binding.installDir,control_dir:binding.controlDir,runtime_sha256:binding.runtimeSha256,
        scope:{schema:1,root:nativeRoot,collection:null,namespace:'11'.repeat(16),revision:1,observed_unix_ms:1,outcome:'registered',
            registration:{binding:'cd'.repeat(32),root:registeredRoot,data_root:path.join(path.dirname(root),'data'),collection:'fixture'},fencing_generation:1}}));
    assert.equal((await host.sourceScope(binding,root,null)).outcome,'registered');
    nativeRoot=root;registeredRoot=verbatim;
    assert.equal((await host.sourceScope(binding,verbatim,null)).outcome,'registered');
    registeredRoot=path.join(path.dirname(root),'foreign');
    await assert.rejects(host.sourceScope(binding,root,null),/crossed_source_scope/);
    registeredRoot=root;nativeRoot=path.join(path.dirname(root),'foreign');
    await assert.rejects(host.sourceScope(binding,root,null),/crossed_source_scope/);
});
''';f.write_text(s,encoding='utf-8',newline='\n')
