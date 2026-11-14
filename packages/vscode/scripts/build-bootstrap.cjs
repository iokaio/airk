// SPDX-License-Identifier: Apache-2.0
// One sealed Node bundle contains the entire pinned verifier and public trust root.
const path=require('node:path');
const fs=require('node:fs');
const crypto=require('node:crypto');
const {builtinModules}=require('node:module');
const esbuild=require('esbuild');
const ts=require('typescript');
const root=path.resolve(__dirname,'..'),repository=path.resolve(root,'../..');
const SOURCES=['packages/vscode/package-lock.json','packages/vscode/src/airkctl/bootstrap/provenance.ts',
    'packages/vscode/src/airkctl/bootstrap/provenanceTrust.ts','packages/vscode/scripts/build-bootstrap.cjs',
    'third_party/bootstrap/sigstore-js-LICENSE.txt','third_party/bootstrap/sigstore-js-license-provenance.json'];
const RUNTIME_BUILTINS=new Set(['crypto','util']);
const PACKAGES={'@sigstore/verify':'3.1.1','@sigstore/bundle':'4.0.0','@sigstore/core':'3.2.1','@sigstore/protobuf-specs':'0.5.0','snappyjs':'0.7.0'};
function sourceSeal(){
    const hash=crypto.createHash('sha256').update('AIRK-BOOTSTRAP-PROVENANCE-1\0');
    for(const name of SOURCES){hash.update(name).update('\0').update(fs.readFileSync(path.join(repository,name))).update('\0');}
    return hash.digest('hex');
}
function noticesText(){
    const notices=[];
    for(const [name,version] of Object.entries(PACKAGES)){
        const folder=path.join(root,'node_modules',name),manifest=JSON.parse(fs.readFileSync(path.join(folder,'package.json'),'utf8'));
        if(manifest.name!==name||manifest.version!==version||!['MIT','Apache-2.0'].includes(manifest.license)){throw new Error('Unreviewed bootstrap dependency identity: '+name);}
        const license=name==='@sigstore/verify'?path.join(repository,'third_party/bootstrap/sigstore-js-LICENSE.txt'):path.join(folder,'LICENSE');
        notices.push(name+' '+version+' ('+manifest.license+')\n'+fs.readFileSync(license,'utf8'));
    }
    return notices.join('\n\n');
}
function header(seal=sourceSeal()){return '/* SPDX-License-Identifier: Apache-2.0 */\n/* AIRK bootstrap provenance source-sha256: '+seal+' */\n';}
function verifyBundle(bytes,seal=sourceSeal(),compare=true){
    if(bytes.length>4*1024*1024){throw new Error('Bootstrap bundle exceeds bound');}
    const text=new TextDecoder('utf-8',{fatal:true}).decode(bytes);
    if(!text.startsWith(header(seal))||!text.includes('AIRK bootstrap provenance dependencies\n'+noticesText()+'\n*/')){throw new Error('Bootstrap source seal or complete notices differ');}
    verifyJavaScript(text,RUNTIME_BUILTINS);
    if(compare&&!Buffer.from(bytes).equals(Buffer.from(compileProvenance().outputFiles[0].contents))){throw new Error('Bootstrap provenance differs from exact pinned source compilation');}
}
function verifyJavaScript(text,permitted){
    const tree=ts.createSourceFile('bootstrap.js',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.JS);
    if(tree.parseDiagnostics.length){throw new Error('Bootstrap bundle is not valid JavaScript');}
    const allowed=new Set(builtinModules.map(name=>name.replace(/^node:/,'')));
    function visit(node){
        if(ts.isImportDeclaration(node)||ts.isImportEqualsDeclaration(node)||ts.isExportDeclaration(node)&&node.moduleSpecifier){throw new Error('Bootstrap contains an external static import');}
        if(ts.isIdentifier(node)&&node.text==='require'){
            const call=node.parent;
            if(!ts.isCallExpression(call)||call.expression!==node||call.arguments.length!==1||!ts.isStringLiteral(call.arguments[0])||(!allowed.has(call.arguments[0].text.replace(/^node:/,''))||!permitted.has(call.arguments[0].text.replace(/^node:/,'')))){throw new Error('Bootstrap contains dynamic or external require');}
        }
        if(ts.isCallExpression(node)&&(node.expression.kind===ts.SyntaxKind.ImportKeyword||ts.isIdentifier(node.expression)&&['eval','Function'].includes(node.expression.text))||ts.isNewExpression(node)&&ts.isIdentifier(node.expression)&&node.expression.text==='Function'){throw new Error('Bootstrap contains dynamic code loading');}
        ts.forEachChild(node,visit);
    }
    visit(tree);
}

const RELEASE_SOURCE='packages/vscode/src/airkctl/bootstrap/release.ts';
const PROJECTION='fixtures/arbiter/publication_catalog.json';
const CATALOG='crates/airk-retrieval/src/arbiter/release-catalog.json';
function nativeProjection(required=false){
    const source=path.join(repository,PROJECTION);
    if(!fs.existsSync(source)){if(required){throw new Error('Qualified native optional publication capture is required for this package');}return undefined;}
    const bytes=fs.readFileSync(source);
    if(bytes.length>1024*1024){throw new Error('Native optional projection exceeds build-input bound');}
    const value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
    const version=JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8')).version;
    if(value.schema!=='airk.arbiter.publication/1'||value.tag!=='v'+version||value.compiled_catalog_sha256!==crypto.createHash('sha256').update(fs.readFileSync(path.join(repository,CATALOG))).digest('hex')){throw new Error('Native optional publication capture is stale for this exact version/catalog');}
    return value;
}
function releaseSeal(){
    const hash=crypto.createHash('sha256').update('AIRK-BOOTSTRAP-RELEASE-1\0');
    for(const name of [RELEASE_SOURCE,'packages/vscode/package.json','packages/vscode/scripts/build-bootstrap.cjs',CATALOG,PROJECTION]){
        const source=path.join(repository,name);hash.update(name).update('\0').update(fs.existsSync(source)?fs.readFileSync(source):Buffer.from('absent')).update('\0');
    }
    return hash.digest('hex');
}
function compileRelease(){
    const projection=nativeProjection();
    const header='/* SPDX-License-Identifier: Apache-2.0 */\n/* AIRK bootstrap release source-sha256: '+releaseSeal()+' */\n';
    const result=esbuild.buildSync({absWorkingDir:root,entryPoints:[path.join(repository,RELEASE_SOURCE)],outfile:path.join(root,'out/airkctl/bootstrap/release.js'),bundle:true,write:false,platform:'node',format:'cjs',target:'node20',metafile:true,banner:{js:header},define:{__AIRK_OPTIONAL_PUBLICATION__:JSON.stringify(projection??null)}});
    if(result.outputFiles.length!==1||Object.values(result.metafile.outputs).some(value=>value.imports.length)){throw new Error('Release bootstrap has unexpected runtime imports/outputs');}
    for(const name of Object.keys(result.metafile.inputs)){if(path.resolve(root,name)!==path.join(repository,RELEASE_SOURCE)){throw new Error('Release bootstrap has an unreviewed source');}}
    verifyJavaScript(new TextDecoder('utf-8',{fatal:true}).decode(result.outputFiles[0].contents),new Set());
    return Buffer.from(result.outputFiles[0].contents);
}
function verifyReleaseBundle(bytes,required=true){
    if(bytes.length>4*1024*1024){throw new Error('Release bootstrap bundle exceeds bound');}
    if(required){nativeProjection(true);}
    if(!Buffer.from(bytes).equals(compileRelease())){throw new Error('Release bootstrap differs from exact source and native projection');}
}
function compileProvenance(){
    const banner=header()+'/*!\nAIRK bootstrap provenance dependencies\n'+noticesText()+'\n*/';
    const result=esbuild.buildSync({absWorkingDir:root,entryPoints:[path.join(root,'src/airkctl/bootstrap/provenance.ts')],outfile:path.join(root,'out/airkctl/bootstrap/provenance.js'),bundle:true,write:false,platform:'node',format:'cjs',target:'node20',metafile:true,legalComments:'inline',banner:{js:banner}});
    const allowed=new Set(builtinModules.map(name=>name.replace(/^node:/,'')));
    for(const item of Object.values(result.metafile.outputs).flatMap(output=>output.imports)){
        if(!item.external||(!allowed.has(item.path.replace(/^node:/,''))||!RUNTIME_BUILTINS.has(item.path.replace(/^node:/,'')))){throw new Error('Bootstrap bundle retains an external runtime package: '+item.path);}
    }
    for(const name of Object.keys(result.metafile.inputs)){
        const full=path.resolve(root,name),relative=path.relative(root,full).split(path.sep).join('/');
        if(!['src/airkctl/bootstrap/provenance.ts','src/airkctl/bootstrap/provenanceTrust.ts'].includes(relative)&&!Object.keys(PACKAGES).some(packageName=>relative.startsWith('node_modules/'+packageName+'/'))){throw new Error('Bootstrap bundle contains an unreviewed source: '+relative);}
    }
    if(result.outputFiles.length!==1){throw new Error('Bootstrap build produced unexpected files');}
    return result;
}
function build(){
    const result=compileProvenance();
    verifyBundle(result.outputFiles[0].contents,sourceSeal(),false);
    fs.writeFileSync(path.join(root,'out/airkctl/bootstrap/provenance.js'),result.outputFiles[0].contents);
    fs.writeFileSync(path.join(root,'out/airkctl/bootstrap/provenance.meta.json'),JSON.stringify(result.metafile,null,2)+'\n');
    const release=compileRelease();verifyReleaseBundle(release,false);
    fs.writeFileSync(path.join(root,'out/airkctl/bootstrap/release.js'),release);
}
function boundedStdin(){
    const parts=[];let size=0;
    while(true){const buffer=Buffer.alloc(Math.min(65536,4*1024*1024+1-size));const used=fs.readSync(0,buffer,0,buffer.length,null);if(!used){break;}size+=used;if(size>4*1024*1024){throw new Error('Bootstrap stdin exceeds bound');}parts.push(buffer.subarray(0,used));}
    return Buffer.concat(parts);
}
module.exports={sourceSeal,verifyBundle,releaseSeal,compileRelease,verifyReleaseBundle};
if(require.main===module){
    const args=process.argv.slice(2);
    if(args.length===1&&args[0]==='--seal'){process.stdout.write(sourceSeal()+'\n');}
    else if(args.length===1&&args[0]==='--release-seal'){process.stdout.write(releaseSeal()+'\n');}
    else if(args.length===1&&args[0]==='--verify-stdin'){verifyBundle(boundedStdin());process.stdout.write('verified\n');}
    else if(args.length===1&&args[0]==='--verify-release-stdin'){verifyReleaseBundle(boundedStdin());process.stdout.write('verified\n');}
    else if(args.length===0){build();}
    else{throw new Error('Unknown bootstrap build arguments');}
}
