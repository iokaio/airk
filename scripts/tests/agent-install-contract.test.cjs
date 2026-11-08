// SPDX-License-Identifier: Apache-2.0
const test=require('node:test'),assert=require('node:assert/strict');
const {connectionId,httpConfig}=require('../agent-install-contract.cjs');
const id='ab'.repeat(16),url='https://127.0.0.1:57422/mcp';
test('provider live credentials yield only their opaque identity',()=>{
    assert.equal(connectionId({headers:{Authorization:'Bearer airk1.'+id+'.'+'cd'.repeat(32)}}),id);
    for(const header of ['Bearer token','Bearer airk1.'+id+'.short','Bearer ${env:OTHER}','Basic secret'])assert.throws(()=>connectionId({headers:{Authorization:header}}));
});
test('leaked serialized credentials cannot appear in assertion stacks',()=>{
    const token='airk1.'+id+'.'+'cd'.repeat(32);
    for(const value of [{url:token},{command:token},{headers:{Authorization:'Bearer '+token}}]){
        try{httpConfig(value,'vscode',url);assert.fail('Expected fixed credential refusal');}
        catch(error){assert.equal(error.message,'Serialized agent configuration contains a credential');assert.ok(!error.stack.includes(token));}
    }
});
test('current family exports preserve disabled state with secret references and no process',()=>{
    httpConfig({url,enabled:false,bearer_token_env_var:'AIRK_MCP_TOKEN_'+id.toUpperCase()},'codex',url);
    for(const family of ['claude','vscode']){
        const value={url,type:'http',disabled:true,headers:{Authorization:family==='vscode'?'Bearer ${input:airkToken_'+id+'}':'Bearer ${AIRK_MCP_TOKEN_'+id.toUpperCase()+'}'}};
        httpConfig(value,family,url);
        for(const change of [{command:'old-airk'},{args:[]},{env:{AIRK_CLIENT:'fixture'}},{disabled:false},{url:url+'wrong'},{headers:{Authorization:'Bearer airk1.'+id+'.'+'cd'.repeat(32)}}])assert.throws(()=>httpConfig({...value,...change},family,url));
    }
});
test('the actual generated installed-editor driver parses with both historical and current package inputs',()=>{
    const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
    const source=fs.readFileSync(path.join(__dirname,'../check-agent-install.cjs'),'utf8');
    const begin=source.indexOf("await fs.writeFile(path.join(driver,'driver.js'),")+"await fs.writeFile(path.join(driver,'driver.js'),".length;
    const end=source.indexOf('`);',begin)+1;
    assert.ok(begin>0&&end>begin);
    for(const previousVsix of [undefined,'old.vsix']){
        const context={path,JSON,extensions:'/owned/extensions',canonical:'/owned/workspace',collection:'fixture',run:'/owned/run',install:'/owned/install',previousVersion:previousVsix?'1.8.12':null,version:'1.8.14',previousVsix,candidate:{payload:{'native/runtime/airk-install.json':'digest'}},files:['a','b','c'],name:'old',__dirname:path.dirname(__dirname)};
        const driver=vm.runInNewContext(source.slice(begin,end),context);
        new vm.Script(driver);
        assert.match(driver,/\^Bearer \[\$\]\[\{\]input:airkToken_/);
        assert.match(driver,/runtime\.httpConnectionInfo/);
        assert.match(driver,/McpHttpSession\.open/);
    }
});


test('installed-editor acceptance isolates every client home and disables shell integration',()=>{
    const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
    const {privateEditorEnvironment,privateClientEnvironment}=require('../editor-ui.cjs');
    const source=fs.readFileSync(path.join(__dirname,'../check-agent-install.cjs'),'utf8');
    const parent={HOME:'/ambient',USERPROFILE:'/ambient',CODEX_HOME:'/ambient/codex',CLAUDE_CONFIG_DIR:'/ambient/claude',XDG_CONFIG_HOME:'/ambient/config',APPDATA:'/ambient/app',LOCALAPPDATA:'/ambient/local',PATH:'unchanged',VSCODE_IPC_HOOK_CLI:'ambient',ELECTRON_RUN_AS_NODE:'1'};
    const before={...parent},clientHome=path.resolve('owned-client-home');
    const declaration=source.match(/const env=(privateClientEnvironment\([^;]+);/);
    assert.ok(declaration);
    const env=vm.runInNewContext(declaration[1],{privateEditorEnvironment,privateClientEnvironment,process:{env:parent},clientHome});
    assert.deepEqual(parent,before);assert.equal(env.PATH,parent.PATH);
    for(const key of ['HOME','USERPROFILE','CODEX_HOME','CLAUDE_CONFIG_DIR','XDG_CONFIG_HOME','APPDATA','LOCALAPPDATA']){
        const relative=path.relative(clientHome,env[key]);assert.ok(relative===''||!relative.startsWith('..')&&!path.isAbsolute(relative),key);
    }
    assert.equal(env.VSCODE_IPC_HOOK_CLI,undefined);assert.equal(env.ELECTRON_RUN_AS_NODE,undefined);
    const settings=source.match(/writeFile\(path\.join\(profile,'User\/settings\.json'\),JSON\.stringify\((\{[\s\S]*?\})\)\);/);
    assert.ok(settings);
    const value=vm.runInNewContext('('+settings[1]+')',{path,run:path.resolve('owned-run'),install:path.resolve('owned-install')});
    assert.equal(value['airk.cli.shellIntegration'],'off');
    assert.ok(source.includes('No shell startup file or user PATH is changed (airk.cli.shellIntegration is off).'));
    assert.ok(!source.includes('Adds a marked PATH block'));
    assert.match(source,/const nativeRunner=async\(command,args\)=>.*execute\(command,args,\{cwd:path.dirname\(command\),env,/);
    assert.ok(!source.includes('{verifyBundledRuntime,nativeRunner}'));
});
