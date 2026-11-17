// SPDX-License-Identifier: Apache-2.0
// Read-only extension-host observation, without any MCP/provider/network request.
const vscode=require('vscode'),fs=require('node:fs/promises'),tls=require('node:tls'),path=require('node:path');
const {createHash,X509Certificate}=require('node:crypto');
exports.activate=async()=>{
    const result={schema:1,editor:vscode.version,node:process.versions.node,electron:process.versions.electron,
        mcp_requests:0,model_requests:0,os_trust_mutated:false};
    try{
        const expected=process.env.AIRK_PROBE_CA_SHA256;
        const file=process.env.AIRK_PROBE_CA_FILE,observed=process.env.NODE_EXTRA_CA_CERTS;
        const fingerprint=pem=>new X509Certificate(pem).fingerprint256.replaceAll(':','').toLowerCase();
        result.extra_ca_env_present=!!observed;
        result.extra_ca_env_matches_owned_file=!!observed&&await fs.realpath(observed).then(value=>value.toLowerCase()===file.toLowerCase(),()=>false);
        if(result.extra_ca_env_matches_owned_file)result.bundle_sha256=createHash('sha256').update(await fs.readFile(file)).digest('hex');
        result.get_ca_certificates_supported=typeof tls.getCACertificates==='function';
        if(result.get_ca_certificates_supported)for(const type of ['extra','default','system','bundled']){
            const certs=tls.getCACertificates(type);result[type+'_ca_count']=certs.length;
            result[type+'_ca_contains_native_issuer']=certs.some(pem=>fingerprint(pem)===expected);
        }
        result.use_system_ca_flag_present=process.execArgv.includes('--use-system-ca');
        result.observed=true;
    }catch(error){result.observed=false;result.error={name:error.name,message:error.message};}
    finally{
        await fs.writeFile(process.env.AIRK_PROBE_RESULT,JSON.stringify(result,null,2),{flag:'wx'});
        await vscode.commands.executeCommand('workbench.action.quit');
    }
};
