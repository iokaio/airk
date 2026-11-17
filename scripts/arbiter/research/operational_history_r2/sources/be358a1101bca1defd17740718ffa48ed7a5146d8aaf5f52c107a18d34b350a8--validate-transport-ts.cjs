// SPDX-License-Identifier: Apache-2.0
// Read-only TypeScript overlay and test loader; original source/out bytes stay intact.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=path.resolve(__dirname,'../..'),ts=require(path.join(root,'packages/vscode/node_modules/typescript'));
const staged=path.join(__dirname,'transport-ts-staged'),files=JSON.parse(fs.readFileSync(path.join(__dirname,'transport-ts-manifest.json'),'utf8')).map(value=>value.path);
const normal=value=>path.resolve(value).replaceAll('\\','/').toLowerCase();
const overlay=new Map(files.map(name=>[normal(path.join(root,name)),fs.readFileSync(path.join(staged,name),'utf8')]));
const config=ts.readConfigFile(path.join(root,'packages/vscode/tsconfig.json'),ts.sys.readFile);
if(config.error)throw new Error(ts.flattenDiagnosticMessageText(config.error.messageText,'\n'));
const parsed=ts.parseJsonConfigFileContent(config.config,ts.sys,path.join(root,'packages/vscode'));
const options={...parsed.options,noEmit:true,typeRoots:[path.join(root,'packages/vscode/node_modules/@types')]},host=ts.createCompilerHost(options),originalRead=host.readFile.bind(host);
host.readFile=file=>overlay.get(normal(file))??originalRead(file);
const program=ts.createProgram(parsed.fileNames,options,host),diagnostics=ts.getPreEmitDiagnostics(program);
if(diagnostics.length){process.stdout.write(ts.formatDiagnosticsWithColorAndContext(diagnostics,{getCurrentDirectory:()=>root,getCanonicalFileName:x=>x,getNewLine:()=>"\n"}));process.exit(1);}
process.stdout.write('Staged TypeScript overlay typecheck passed; production source/out files unchanged.\n');
const emitted=new Map(files.map(name=>{
 const code=ts.transpileModule(overlay.get(normal(path.join(root,name))),{compilerOptions:{...parsed.options,noEmit:false},fileName:path.join(root,name),reportDiagnostics:true});
 if(code.diagnostics?.length)throw new Error(code.diagnostics.map(d=>ts.flattenDiagnosticMessageText(d.messageText,'\n')).join('\n'));
 return [normal(path.join(root,name.replace('/src/','/out/').replace(/\.ts$/,'.js'))),code.outputText];
}));
const originalExtension=Module._extensions['.js'];
Module._extensions['.js']=(module,filename)=>{
 const code=emitted.get(normal(filename));
 if(code!==undefined)return module._compile(code,filename);
 return originalExtension(module,filename);
};
require(path.join(root,'packages/vscode/out/test/nativeReadRetry.test.js'));
require(path.join(root,'packages/vscode/out/test/nativeReadRetryProcess.test.js'));
