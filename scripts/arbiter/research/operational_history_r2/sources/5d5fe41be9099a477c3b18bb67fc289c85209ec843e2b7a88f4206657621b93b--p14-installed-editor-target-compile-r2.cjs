// SPDX-License-Identifier: Apache-2.0
// Root-owned future execution only. Emit explicitly listed external tests, never product files.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const [work,manifestPath]=process.argv.slice(2);assert.ok(work&&manifestPath);
const pkg=path.join(path.resolve(work),'packages/vscode');
process.chdir(pkg); // Match the canonical package compiler's type-root context.
const ts=require(path.join(pkg,'node_modules/typescript'));
const targets=['installedEditor.ts','installedEditorContracts.ts','installedEditorContracts.test.ts'].map(name=>path.join(pkg,'src/test',name));
const normalize=value=>process.platform==='win32'?path.resolve(value).toLowerCase():path.resolve(value);
const allowed=new Set(targets.map(name=>normalize(path.join(pkg,'out/test',path.basename(name,'.ts')+'.js'))));
const config=ts.readConfigFile(path.join(pkg,'tsconfig.json'),ts.sys.readFile);assert.equal(config.error,undefined);
const parsed=ts.parseJsonConfigFileContent(config.config,ts.sys,pkg);assert.equal(parsed.errors.length,0);
const options={...parsed.options,noEmit:false,noEmitOnError:true,incremental:false};
const program=ts.createProgram(targets,options),diagnostics=ts.getPreEmitDiagnostics(program);
if(diagnostics.length){console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics,{getCurrentDirectory:()=>pkg,getCanonicalFileName:name=>name,getNewLine:()=>"\n"}));process.exit(1);}
const emitted=[];
for(const name of targets){
    const source=program.getSourceFile(name);assert.ok(source);
    const result=program.emit(source,(filename,text)=>{
        assert.ok(allowed.has(normalize(filename)),'Refuse non-test emit: '+filename);
        fs.mkdirSync(path.dirname(filename),{recursive:true});fs.writeFileSync(filename,text);emitted.push(path.resolve(filename));
    });
    assert.equal(result.emitSkipped,false);assert.equal(result.diagnostics.length,0);
}
assert.equal(emitted.length,3);assert.equal(new Set(emitted.map(normalize)).size,3);
fs.writeFileSync(manifestPath,JSON.stringify({typecheck:'passed',emitted,production_emit_allowed:false},null,2)+'\n',{flag:'wx'});
console.log('Semantic TypeScript check passed; emitted exactly three external test modules.');
