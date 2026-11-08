// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as path from 'node:path';
import {migrateAgentConfig} from '../agentMigration';
import {parseTomlDeclarations} from '../agentToml';
import type {LaunchSpec} from '../model';
import type {HttpConnectionInfo} from '../serviceHttp';
const storage=path.resolve('fixture-storage'),install=path.resolve('fixture-install');
const original:LaunchSpec={root:path.resolve('fixture-project'),dataDir:path.resolve('fixture-data'),collection:'fixture',name:'fixture',command:path.join(install,'airk-launcher'),service:{installDir:install,project:'a'.repeat(64)}};
const info:HttpConnectionInfo={schema:1,connection_id:'1'.repeat(32),endpoint:{schema:1,installation:'2'.repeat(64),user:'fixture',namespace:'3'.repeat(32),generation:1,service_instance:'4'.repeat(32),executable_sha256:'5'.repeat(64),url:'https://127.0.0.1:57421/mcp',ca_sha256:'6'.repeat(64),certificate_sha256:'7'.repeat(64),state:'ready',predecessor_digest:null},selection:{kind:'scoped',selection:{schema:1,context_key:'8'.repeat(64),namespace:'3'.repeat(32),expected_catalog_revision:1,grant_id:'grt_'+'9'.repeat(32),expected_grant_revision:1,index_id:'idx_'+'a'.repeat(32),project_ids:['prj_'+'b'.repeat(32)]}},ca_certificate_file:path.join(install,'ca.pem'),credential_file:path.join(install,'credential-'+ '1'.repeat(32)+'.token')};
const selected={...original,http:info};
const instructions=`model = "retained-model"
instructions = """
[mcp_servers.airk]
command = "spoofed launcher text, not configuration"
"""
`;
const unrelated=`[mcp_servers.other] # retained comment
command = 'node' # literal string
args = [
  'server.js', # comment inside array
  "#a literal hash",
]
`;
const owned=`[mcp_servers.airk] # owned note
command = ${JSON.stringify(original.command)} # original launcher
args = [
  "--root",
  ${JSON.stringify(original.root)}, # original source
  "mcp",
]
enabled = false
`;

test('actual guarded Codex HTTPS migration preserves unrelated multiline instructions and other servers byte-for-byte',()=>{
    const before=instructions+owned+unrelated;
    const after=migrateAgentConfig('codex',selected,storage,before,false);
    assert.ok(after.startsWith(instructions),'unrelated multiline settings must remain byte-identical');
    assert.ok(after.endsWith(unrelated),'unrelated MCP syntax/comments must remain byte-identical');
    const rows=parseTomlDeclarations(after),airk=rows.find(row=>row.serverKey==='airk')!;
    assert.equal(airk.fields.url,info.endpoint.url);assert.equal(airk.fields.enabled,false);assert.equal(airk.fields.command,undefined);
    assert.equal(airk.fields.bearer_token_env_var,'AIRK_MCP_TOKEN_'+info.connection_id.toUpperCase());
    assert.ok(after.includes('# owned note')&&after.includes('# original launcher')&&after.includes('# original source'));
    assert.equal(migrateAgentConfig('codex',selected,storage,after,false),after,'second pass must be byte-identical');
});

test('spoofed AIRK headers inside a TOML string cannot create or change a credential reference',()=>{
    const before=instructions+unrelated;
    assert.equal(migrateAgentConfig('codex',selected,storage,before,false),before);
    assert.ok(!before.includes('AIRK_MCP_TOKEN_'));
});

test('guarded writer still refuses foreign owned-name entries and malformed overlapping inline tables',()=>{
    assert.throws(()=>migrateAgentConfig('codex',selected,storage,instructions+owned.replace(JSON.stringify(original.command),'"foreign"')+unrelated,false),/Unrecognized/);
    assert.throws(()=>parseTomlDeclarations('mcp_servers={ airk={ command="one" } }\n[mcp_servers.airk]\nargs=[]'),/conflicting_inline/);
});
