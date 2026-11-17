// SPDX-License-Identifier: Apache-2.0
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process');
const root=path.resolve(__dirname,'../..'),ts=require(path.join(root,'packages/vscode/node_modules/typescript'));
const mode=process.argv[2];if(!['current','candidate'].includes(mode))throw new Error('Expected current or candidate');
const stage=path.join(__dirname,'waiting-observer-staged'),out=path.join(stage,'transpiled-'+mode);fs.mkdirSync(out,{recursive:true});
const nativeOut=path.join(root,'packages/vscode/out');
function compile(source,base,destination,controller){
    const value=ts.transpileModule(fs.readFileSync(source,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
    fs.writeFileSync(destination,value.replace(/require\((['"])(\.[^'"]*)\1\)/g,(_match,_quote,relative)=>{
        const selected=relative==='../explorerController'?controller:path.resolve(base,relative)+'.js';return 'require('+JSON.stringify(selected)+')';
    }));
}
const selected=mode==='current'?path.join(nativeOut,'explorerController.js'):path.join(out,'explorerController.js');
if(mode==='candidate')compile(path.join(stage,'packages/vscode/src/explorerController.ts'),nativeOut,selected,selected);
const test=path.join(out,'explorerShell.test.js');
compile(path.join(stage,'packages/vscode/src/test/explorerShell.test.ts'),path.join(nativeOut,'test'),test,selected);
const result=cp.spawnSync(process.execPath,['--test','--test-name-pattern','waiting automatic observation|already complete current native|retired window and control|external missing malformed|window-generation replacement|initial native catalog page',test],{encoding:'utf8',cwd:root});
const log=String(result.stdout??'')+String(result.stderr??'');fs.writeFileSync(path.join(__dirname,'waiting-observer-'+mode+'.log'),log);process.stdout.write(log);process.exitCode=result.status??1;
