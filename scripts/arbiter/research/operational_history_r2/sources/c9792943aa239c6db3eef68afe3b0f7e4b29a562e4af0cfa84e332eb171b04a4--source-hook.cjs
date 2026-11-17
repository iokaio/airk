// SPDX-License-Identifier: Apache-2.0
// Fresh validation-only source loader. No source or compiled output is written.
const fs=require('node:fs'),path=require('node:path');
const repository=path.resolve(__dirname,'../../../../..');
const ts=require(path.join(repository,'packages/vscode/node_modules/typescript'));
if(ts.version!=='5.9.3')throw Error('Pinned TypeScript5.9.3 is required');
const configFile=ts.readConfigFile(path.join(repository,'packages/vscode/tsconfig.json'),ts.sys.readFile);
if(configFile.error)throw Error(ts.flattenDiagnosticMessageText(configFile.error.messageText,'\n'));
const parsed=ts.parseJsonConfigFileContent(configFile.config,ts.sys,path.join(repository,'packages/vscode'));
const options={...parsed.options,noEmit:false,jsx:ts.JsxEmit.ReactJSX,jsxImportSource:'preact',sourceMap:false,inlineSourceMap:false};
for(const extension of ['.ts','.tsx'])require.extensions[extension]=(module,file)=>{
 const source=fs.readFileSync(file,'utf8');
 const result=ts.transpileModule(source,{compilerOptions:options,fileName:file,reportDiagnostics:true});
 const errors=(result.diagnostics??[]).filter(item=>item.category===ts.DiagnosticCategory.Error);
 if(errors.length)throw Error(ts.formatDiagnostics(errors,{getCanonicalFileName:name=>name,getCurrentDirectory:()=>repository,getNewLine:()=> '\n'}));
 module._compile(result.outputText,file);
};
