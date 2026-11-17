// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=path.resolve(__dirname,'../../../..'),original=path.join(root,'packages/vscode/out/test/installedEditor.js');
let source=fs.readFileSync(original,'utf8');
const needle='let client = new Mcp(definition, definition.ca);';
if(source.split(needle).length!==2)throw Error('Reviewed installed-driver injection point changed');
source=source.replace(needle,`await require(${JSON.stringify(path.join(root,'scripts/native-editor-failure.cjs'))}).withFailureStop({
    run:()=>require(${JSON.stringify(path.join(__dirname,'code-mcp-host.cjs'))}).run({vscode,extension,definition,selection:transportSelection,mark,deadline:sharedDeadline,validate:(value,denied)=>scoped(value,transportSelection,denied)}),
    stop:()=>ui('serviceStop',{installDir:path.join(folder,'native-service'),runtimeSha256:process.env.AIRK_EDITOR_RUNTIME_SHA256},'#manage/service'),
    publishStop:stop=>publishMailbox(path.join(folder,'native-stop.json'),stop),
    retainFailure:value=>mark('actual-code-failure-cleanup',value)
});\n${needle}`);
const loaded=new Module(original,module);loaded.filename=original;loaded.paths=Module._nodeModulePaths(path.dirname(original));loaded._compile(source,original);module.exports=loaded.exports;
