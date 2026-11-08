// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {createHash} from 'node:crypto';
import {globalConfigLocations,globalConfigDeclarations,inventoryGlobalAgentConfigs,transformGlobalContainer,type GlobalConfigFile} from '../globalAgentInventory';

test('global inventory plans only standard and explicit client homes, independent of workspace scope',()=>{
    const f=globalConfigLocations({home:'C:\\Users\\owner',platform:'win32',env:{CODEX_HOME:'D:\\profiles\\codex',CLAUDE_CONFIG_DIR:'D:\\profiles\\claude',APPDATA:'C:\\Users\\owner\\AppData\\Roaming'},globalStoragePath:'D:\\vscode-data\\User\\profiles\\abc\\globalStorage\\iokaio.airk'});
    assert.deepEqual(f.blockers,[]);
    assert.ok(f.locations.some(row=>row.destination==='C:\\Users\\owner\\.codex\\config.toml'));
    assert.ok(f.locations.some(row=>row.destination==='D:\\profiles\\codex\\config.toml'));
    assert.ok(f.locations.some(row=>row.destination==='D:\\profiles\\claude\\.claude.json'));
    assert.ok(f.locations.some(row=>row.destination==='D:\\vscode-data\\User\\profiles\\abc\\mcp.json'));
    assert.ok(f.vscodeRoots.includes('D:\\vscode-data\\User'));
    const invalid=globalConfigLocations({home:'/home/owner',platform:'linux',env:{CODEX_HOME:'relative',CLAUDE_CONFIG_DIR:''},globalStoragePath:'/workspace/pretend/globalStorage/iokaio.airk'});
    assert.deepEqual(invalid.blockers.map(row=>row.code),['invalid_codex_home','invalid_claude_config_dir','unknown_vscode_profile_layout']);
    assert.ok(!invalid.locations.some(row=>row.destination.startsWith('/workspace')));
});

test('global declarations retain arbitrary aliases, full fields and exact Claude project containers without inferring scope',()=>{
    const fields={command:'/owned/airk-launcher',args:['--root','/actual/project','mcp'],env:{PRIVATE:'never print'},disabled:true};
    const source=JSON.stringify({account:{token:'unrelated'},mcpServers:{custom_alias:fields},projects:{'/not-authority':{mcpServers:{airk:{...fields,args:['--root','/other/project','mcp']}}}}});
    const rows=globalConfigDeclarations('claude',source);
    assert.deepEqual(rows.map(row=>[row.serverKey,row.containerPath]),[['custom_alias',['mcpServers']],['airk',['projects','/not-authority','mcpServers']]]);
    assert.deepEqual(JSON.parse(JSON.stringify(rows[0].fields)),fields);assert.equal(rows[1].fields.args instanceof Array,true);
    assert.throws(()=>globalConfigDeclarations('claude','{"mcpServers":{},"mcpServers":{}}'),/duplicate_configuration_key/);
    assert.throws(()=>globalConfigDeclarations('copilot','{"servers":{"airk":false}}'),/invalid_mcp_server_declaration/);
});

test('Codex inventory retains original declaration fields and blocks unsupported or ambiguous TOML',()=>{
    const rows=globalConfigDeclarations('codex','# preserved\nmodel = "fixture"\n[mcp_servers.custom_alias]\ncommand = "/owned/airk-launcher"\nargs = ["--root","/exact/project","mcp"]\n[mcp_servers.custom_alias.env]\nPRIVATE = "do not log"\n');
    assert.equal(rows[0].serverKey,'custom_alias');assert.deepEqual(rows[0].fields.args,['--root','/exact/project','mcp']);assert.equal((rows[0].fields.env as any).PRIVATE,'do not log');
    for(const source of ['[mcp_servers.airk]\ncommand="one"\ncommand="two"','[mcp_servers.airk]\n[mcp_servers.airk]','[[mcp_servers.airk]]\ncommand="x"','[mcp_servers.airk]\ncommand="unterminated'])assert.throws(()=>globalConfigDeclarations('codex',source));
    assert.deepEqual(globalConfigDeclarations('codex','mcp_servers = {}'),[]);
    assert.equal(globalConfigDeclarations('codex',"[mcp_servers.airk]\ncommand = 'literal'")[0].fields.command,'literal');
    assert.deepEqual(globalConfigDeclarations('codex','[profiles.work.mcp_servers.alias]\ncommand="x"')[0].containerPath,['profiles','work','mcp_servers']);
    assert.equal(globalConfigDeclarations('codex','mcp_servers.alias.command="x"')[0].fields.command,'x');
});

test('nested global migration preserves every byte outside the selected MCP container',()=>{
    const source='{\n "account": {"token":"retain secret"},\n "projects": {"/literal.key": {"trust": true, "mcpServers": {"airk": {"command":"old"}}},"/sibling":{"mcpServers":{"other":{"command":"untouched"}}}}\n}';
    const file:GlobalConfigFile={agent:'claude',destination:'/home/.claude.json',source,sha256:createHash('sha256').update(source).digest('hex'),declarations:globalConfigDeclarations('claude',source)};
    const result=transformGlobalContainer(file,['projects','/literal.key','mcpServers'],wrapped=>wrapped.replace('"command":"old"','"type":"http","url":"https://127.0.0.1:1/mcp"'));
    assert.equal(result,source.replace('"command":"old"','"type":"http","url":"https://127.0.0.1:1/mcp"'));
    assert.throws(()=>transformGlobalContainer(file,['projects','/sibling','missing'],value=>value),/unknown_configuration_container/);
    assert.equal(file.source,source);
});

test('actual bounded inventory reads exact known files and named profiles but never walks arbitrary descendants',async()=>{
    const home=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-global-inventory-')));
    try{
        const user=path.join(home,'custom-user'),codex=path.join(home,'.codex');await fs.mkdir(codex);
        const original='[mcp_servers.alias]\ncommand = "/owned/airk-launcher"\nargs = ["--root","/retained/root","mcp"]\n';await fs.writeFile(path.join(codex,'config.toml'),original);
        await fs.writeFile(path.join(codex,'named.config.toml'),'[mcp_servers.named]\ncommand="/owned/airk-launcher"');
        await fs.mkdir(path.join(user,'profiles','one'),{recursive:true});await fs.writeFile(path.join(user,'profiles','one','mcp.json'),'{"servers":{"alias":{"command":"/owned/airk-launcher","args":[]}}}');
        await fs.mkdir(path.join(home,'arbitrary'),{recursive:true});await fs.writeFile(path.join(home,'arbitrary','mcp.json'),'invalid');
        const inventory=await inventoryGlobalAgentConfigs({home,env:{},vscodeUserDirectories:[user]});
        assert.deepEqual(inventory.blockers,[]);assert.equal(inventory.files.length,3);
        const read=inventory.files.find(row=>row.agent==='codex')!;assert.equal(read.source,original);assert.equal(read.sha256,createHash('sha256').update(Buffer.from(original)).digest('hex'));
        assert.ok(!inventory.checked.some(file=>file.includes('arbitrary')));assert.equal(await fs.readFile(path.join(codex,'config.toml'),'utf8'),original);
        await fs.writeFile(path.join(home,'.claude.json'),'{"mcpServers":false}');
        const refused=await inventoryGlobalAgentConfigs({home,env:{},vscodeUserDirectories:[user]});assert.ok(refused.blockers.some(row=>row.code==='invalid_mcp_server_group'));assert.ok(!JSON.stringify(refused.blockers).includes(original));
    }finally{await fs.rm(home,{recursive:true,force:true});}
});

test('aliased or oversized global configuration is blocked, never interpreted as absent',async()=>{
    const home=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-global-inventory-bounds-')));
    try{
        await fs.mkdir(path.join(home,'.codex'));const config=path.join(home,'.codex','config.toml');await fs.writeFile(config,'x'.repeat(1_048_577));
        let result=await inventoryGlobalAgentConfigs({home,env:{}});assert.ok(result.blockers.some(row=>row.destination===config));
        await fs.writeFile(config,'[mcp_servers.airk]\ncommand="original"');await fs.link(config,path.join(home,'hardlink'));
        result=await inventoryGlobalAgentConfigs({home,env:{}});assert.ok(result.blockers.some(row=>row.destination===config));assert.equal(result.files.length,0);
    }finally{await fs.rm(home,{recursive:true,force:true});}
});


test('VSCode top-level transform retains credential inputs and unrelated settings',()=>{
    const source='{"servers":{"airk":{"command":"old"}},"inputs":[{"id":"unrelated"}],"future":{"retained":true}}';
    const file:GlobalConfigFile={agent:'copilot',destination:'/User/mcp.json',source,sha256:'00'.repeat(32),declarations:globalConfigDeclarations('copilot',source)};
    const after=transformGlobalContainer(file,['servers'],before=>{
        assert.equal(before,source);const value=JSON.parse(before);value.inputs.push({id:'owned-credential'});value.servers.airk={type:'http',url:'https://127.0.0.1:1/mcp'};return JSON.stringify(value);
    });
    assert.deepEqual(JSON.parse(after).inputs,[{id:'unrelated'},{id:'owned-credential'}]);assert.deepEqual(JSON.parse(after).future,{retained:true});
});

test('profile inventory overflow produces an explicit cleanup blocker',async()=>{
    const home=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-global-profile-bound-')));
    try{const user=path.join(home,'User');for(let n=0;n<33;n++)await fs.mkdir(path.join(user,'profiles',String(n)),{recursive:true});
        const result=await inventoryGlobalAgentConfigs({home,env:{},vscodeUserDirectories:[user]});
        assert.ok(result.blockers.some(row=>row.code==='vscode_profile_count_bound'));
        assert.ok(result.checked.length<=64);
    }finally{await fs.rm(home,{recursive:true,force:true});}
});

test('valid unrelated Codex multiline instructions and commented MCP declarations do not block inventory',()=>{
    const source=`model = "fixture"
instructions = """
These are documentation, not real MCP declarations:
[mcp_servers.airk]
command = '/owned/airk-launcher'
"""
literal_instructions = '''
[mcp_servers.another_fake]
command = "fake"
'''
[mcp_servers.other] # actual unrelated server
command = 'node' # literal string with trailing comment
args = [
  'server.js', # retained item
  "#not-a-comment",
]
enabled = true # standard comment
[mcp_servers.other.env]
ROOT = 'C:\\Users\\owner'
`;
    const rows=globalConfigDeclarations('codex',source);
    assert.equal(rows.length,1);assert.equal(rows[0].serverKey,'other');assert.equal(rows[0].fields.command,'node');
    assert.deepEqual(rows[0].fields.args,['server.js','#not-a-comment']);assert.equal(rows[0].fields.enabled,true);
    assert.equal((rows[0].fields.env as Record<string,unknown>).ROOT,'C:\\Users\\owner');
    assert.equal(globalConfigDeclarations('codex',source.slice(0,source.indexOf('[mcp_servers.other]'))).length,0);
});

test('Codex inline and dotted declarations retain actual scope arguments without evaluating strings',()=>{
    const source=`mcp_servers = { custom = { command = '/owned/airk-launcher', args = ['--root', '/exact/root', 'mcp'], env = { SAFE = "yes" } } }
[profiles.work.mcp_servers."alias.with.dot"]
command = "/owned/airk-launcher"
args = ["--root", "/different/root", "mcp"]
`;
    const rows=globalConfigDeclarations('codex',source);
    assert.equal(rows.length,2);assert.deepEqual(rows[0].fields.args,['--root','/exact/root','mcp']);
    assert.equal(rows[1].serverKey,'alias.with.dot');assert.deepEqual(rows[1].containerPath,['profiles','work','mcp_servers']);
    assert.deepEqual(rows[1].fields.args,['--root','/different/root','mcp']);
});
