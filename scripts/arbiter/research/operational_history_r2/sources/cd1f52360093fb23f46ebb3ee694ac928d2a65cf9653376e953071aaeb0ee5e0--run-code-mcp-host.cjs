// SPDX-License-Identifier: Apache-2.0
// Reuse the canonical isolated installed-editor lifecycle and actual UI setup.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const {createHash}=require('node:crypto');
if(process.platform!=='win32')throw Error('This exact host/identity-projection audit is scoped to Windows Code 1.145.2');
const root=path.resolve(__dirname,'../../../..'),original=path.join(root,'scripts/check-native-editor.cjs');
const codeAudit=JSON.parse(fs.readFileSync(path.join(__dirname,'code-source-identity.json'),'utf8'));
const codeOut=path.dirname(path.resolve(process.argv[3]));
for(const [relative,evidence]of Object.entries(codeAudit.sources)){
    const actual=createHash('sha256').update(fs.readFileSync(path.join(codeOut,relative))).digest('hex');
    if(actual!==evidence.sha256)throw Error('Reviewed Code MCP projection source changed: '+relative);
}
let source=fs.readFileSync(original,'utf8');
const replaceOnce=(needle,replacement)=>{if(source.split(needle).length!==2)throw Error('Reviewed native editor harness injection point changed');source=source.replace(needle,replacement);};
const auditPaths=['run-code-mcp-host.cjs','code-driver-wrapper.cjs','code-mcp-host.cjs','prepare-code-ca.py','code-user-trust.py','code-source-identity.json'].map(name=>path.relative(root,path.join(__dirname,name)).split(path.sep).join('/'));
replaceOnce('const harnessPaths=[','const harnessPaths=['+JSON.stringify('scripts/native-editor-failure.cjs')+','+auditPaths.map(name=>JSON.stringify(name)).join(',')+',');
replaceOnce("JSON.stringify(path.join(root,'packages/vscode/out/test/installedEditor.js'))",`JSON.stringify(${JSON.stringify(path.join(__dirname,'code-driver-wrapper.cjs'))})`);
const hook="const roles=['same','different'].includes(scenario)?['a','b']:['a'],children=[],uis={},ports={};";
const prepare=path.join(__dirname,'prepare-code-ca.py');
replaceOnce('let bundle;','let bundle;const codeTrustOwners=[];');
replaceOnce(hook,`codeTrustOwners.push(folder);
        const caPreparation=await execute('python',[${JSON.stringify(prepare)},'--runtime',bundle.source,'--owner',folder],{windowsHide:true,env:environment,cwd:root,timeout:300000,maxBuffer:65536});
        const prepared=JSON.parse(caPreparation.stdout);if(!prepared.native_stop_complete||prepared.os_user_trust_installed!==true)throw Error('Owned Code CA preparation/trust did not complete');
        const inherited=environment.NODE_EXTRA_CA_CERTS?await fs.readFile(environment.NODE_EXTRA_CA_CERTS):Buffer.alloc(0);
        const nativeCa=await fs.readFile(prepared.ca_file),bundleFile=path.join(folder,'code-additive-ca.pem');
        await fs.writeFile(bundleFile,Buffer.concat([inherited,Buffer.from('\\n'),nativeCa]),{flag:'wx'});
        environment.NODE_EXTRA_CA_CERTS=bundleFile;
        await fs.writeFile(path.join(folder,'code-ca-preparation.json'),JSON.stringify({native_stop_complete:true,os_user_trust_installed:true,public_ca_file:prepared.ca_file,additive_bundle:bundleFile,inherited_extra_ca_preserved:inherited.length>0,default_node_roots_preserved:true},null,2));
        ${hook}`);
const removeTrust=path.join(__dirname,'code-user-trust.py');
replaceOnce('process.exitCode=1;});',`process.exitCode=1;}).finally(async()=>{
    for(const folder of codeTrustOwners){
        try{await execute('python',[${JSON.stringify(removeTrust)},'--remove-owner',folder],{windowsHide:true,env:environment,cwd:root,timeout:90000,maxBuffer:65536});}
        catch(error){console.error(JSON.stringify({owned_code_trust_removal_failed:true,folder,error:String(error)}));process.exitCode=1;}
    }
});`);
process.env.AIRK_EDITOR_SCENARIOS='multi';
const loaded=new Module(original,module);loaded.filename=original;loaded.paths=Module._nodeModulePaths(path.dirname(original));loaded._compile(source,original);
