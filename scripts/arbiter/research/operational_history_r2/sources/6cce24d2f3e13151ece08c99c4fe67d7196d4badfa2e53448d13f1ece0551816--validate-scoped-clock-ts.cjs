// SPDX-License-Identifier: Apache-2.0
// Read-only source overlay: validate staged edits without changing source/out.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),ts=require(path.join(root,'packages/vscode/node_modules/typescript'));
const staged=path.join(__dirname,'scoped-clock-staged');
const files=JSON.parse(fs.readFileSync(path.join(__dirname,'scoped-clock-manifest.json'),'utf8')).map(value=>value.path).filter(name=>name.endsWith('.ts'));
const normal=value=>path.resolve(value).replaceAll('\\','/').toLowerCase();
const overlay=new Map(files.map(name=>[normal(path.join(root,name)),fs.readFileSync(path.join(staged,name),'utf8')]));
const config=ts.readConfigFile(path.join(root,'packages/vscode/tsconfig.json'),ts.sys.readFile);
if(config.error)throw new Error(ts.flattenDiagnosticMessageText(config.error.messageText,'\n'));
const parsed=ts.parseJsonConfigFileContent(config.config,ts.sys,path.join(root,'packages/vscode'));
const options={...parsed.options,noEmit:true,typeRoots:[path.join(root,'packages/vscode/node_modules/@types')]};
const host=ts.createCompilerHost(options),originalRead=host.readFile.bind(host);
host.readFile=file=>overlay.get(normal(file))??originalRead(file);
const program=ts.createProgram(parsed.fileNames,options,host),diagnostics=ts.getPreEmitDiagnostics(program);
if(diagnostics.length){process.stdout.write(ts.formatDiagnosticsWithColorAndContext(diagnostics,{getCurrentDirectory:()=>root,getCanonicalFileName:x=>x,getNewLine:()=>"\n"}));process.exit(1);}
process.stdout.write('Staged TypeScript overlay typecheck passed; frozen source/out unchanged.\n');
const emitted=new Map(files.map(name=>[normal(path.join(root,name.replace('/src/','/out/').replace(/\.ts$/,'.js'))),
    ts.transpileModule(overlay.get(normal(path.join(root,name))),{compilerOptions:{...parsed.options,noEmit:false},fileName:path.join(root,name)}).outputText]));
const originalExtension=Module._extensions['.js'];
Module._extensions['.js']=(module,filename)=>{
    const code=emitted.get(normal(filename));
    return code!==undefined?module._compile(code,filename):originalExtension(module,filename);
};
const reportPath=path.join(root,'artifacts/upgrade-1.8.9-neo/upgrade-1.8.9-full-r5/diagnostics-native-failure-detail.log');
const report=JSON.parse(fs.readFileSync(reportPath,'utf8')),reply=report.failure.nativeResponse;
const {readRetryDelay}=require(path.join(root,'packages/vscode/out/readRetry.js'));
const original={...reply.structuredContent.requested_scope.selection,expected_catalog_revision:7};
const retained=structuredClone(reply);
assert.equal(readRetryDelay(reply,original),100,'actual retained Neo 7-to-12 pre-query receipt');
assert.equal(readRetryDelay(reply,{...original,expected_catalog_revision:13}),undefined,'backward native clock');
assert.equal(readRetryDelay(reply,{...original,expected_grant_revision:2}),undefined,'changed authority');
assert.deepEqual(reply,retained,'actual native receipt remains complete and unchanged');
process.stdout.write('Actual retained Neo receipt accepts only forward clock with original authority; negative scope checks passed.\n');
require(path.join(root,'packages/vscode/out/test/nativeReadRetry.test.js'));
require(path.join(root,'packages/vscode/out/test/nativeReadRetryProcess.test.js'));
