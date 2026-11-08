// SPDX-License-Identifier: Apache-2.0
// Assertions shared by the actual installed-editor fixture and inert regressions.
const assert=require('node:assert/strict');
function connectionId(definition){
    const header=definition.headers?.Authorization;
    assert.equal(typeof header,'string','HTTP definition requires an authorization reference');
    // Inspect a provider's live header only in memory. Never return its secret.
    const match=/^Bearer (?:airk1\.([a-f0-9]{32})\.[a-f0-9]{64}|\$\{input:airkToken_([a-f0-9]{32})\}|\$\{(?:env:)?AIRK_MCP_TOKEN_([A-F0-9]{32})\})$/.exec(header);
    assert.ok(match,'HTTP definition requires a known native credential identity');
    return (match[1]??match[2]??match[3]).toLowerCase();
}
function httpConfig(value,family,url){
    // Assertion libraries render actual values on failure. Refuse credentials
    // before any field assertion can include them in editor logs or artifacts.
    if(JSON.stringify(value).includes('airk1.'))throw new Error('Serialized agent configuration contains a credential');
    assert.equal(value.url,url,'Agent URL must match the verified native endpoint');
    for(const field of ['command','args','env'])assert.equal(value[field],undefined,'HTTP configuration must not retain a process launch');
    if(family==='codex'){
        assert.match(value.bearer_token_env_var,/^AIRK_MCP_TOKEN_[A-F0-9]{32}$/);
        assert.equal(value.enabled,false);
    }else{
        assert.equal(value.type,'http');assert.equal(value.disabled,true);
        const id=connectionId(value);
        assert.equal(value.headers.Authorization,family==='vscode'?'Bearer ${input:airkToken_'+id+'}':'Bearer ${AIRK_MCP_TOKEN_'+id.toUpperCase()+'}');
    }
    // A serialized agent file contains references, never credential bytes.
}
module.exports={connectionId,httpConfig};
