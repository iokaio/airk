// SPDX-License-Identifier: Apache-2.0
// Called only from the ignored copy of the installed-editor acceptance driver.
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const tls=require('node:tls');
const {createHash,X509Certificate}=require('node:crypto');
const {readRetryDelay}=require(path.join(path.resolve(__dirname,'../../../..'),'packages/vscode/out/test/nativeReadRetry.js'));
const {UNIFIED_TOOLS}=require(path.join(path.resolve(__dirname,'../../../..'),'packages/vscode/out/model.js'));
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
exports.run=async({vscode,extension,definition,selection,mark,deadline,validate})=>{
    const evidence={schema:1,client:'actual VS Code MCP host',editor:vscode.version,model_requests:0,started:false,stopped:false,calls:[]};
    let id;
    const retain=()=>mark('actual-code-mcp-host',evidence);
    const bounded=async(promise,until=deadline)=>{
        let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Actual Code MCP host deadline')),Math.max(1,until-Date.now()));})]);}finally{clearTimeout(timer);}
    };
    try{
        assert.equal(vscode.version,'1.145.2','Exact reviewed Code host projection');
        assert.equal(extension.id,'iokaio.airk');
        const provided=await extension.exports.getServerDefinitions();assert.equal(provided.length,1);
        const raw=provided[0];assert.ok(raw instanceof vscode.McpHttpServerDefinition);
        assert.equal(raw.uri.toString(),definition.uri.toString());assert.equal(raw.version,definition.version);
        // Exact Code 1.145.2 extension-host projection: lowercased extension ID + '/' + provider label.
        // This observed provider supplies one unique label; do not start '*'.
        assert.equal(raw.label,'AIRK');id=extension.id.toLowerCase()+'/'+raw.label;
        evidence.definition={id,label:raw.label,url:raw.uri.toString(),version:raw.version,connection_id:definition.nativeConnection.connection_id,
            selection:definition.nativeConnection.selection};
        const expectedCa=definition.nativeConnection.endpoint.ca_sha256;
        const fingerprint=pem=>new X509Certificate(pem).fingerprint256.replaceAll(':','').toLowerCase();
        const expectedExtraFile=path.join(process.env.AIRK_EDITOR_RUN,process.env.AIRK_EDITOR_SCENARIO,'code-additive-ca.pem');
        const observedExtra=process.env.NODE_EXTRA_CA_CERTS;
        const extraFileMatches=!!observedExtra&&await fs.realpath(observedExtra).then(value=>value.toLowerCase()===expectedExtraFile.toLowerCase(),()=>false);
        const extra=tls.getCACertificates?.('extra'),defaults=tls.getCACertificates?.('default');
        evidence.actualHostTrustObservation={node:process.versions.node,electron:process.versions.electron,
            extra_ca_environment_present:!!observedExtra,extra_ca_file_matches_owned_bundle:extraFileMatches,
            owned_bundle_sha256:extraFileMatches?createHash('sha256').update(await fs.readFile(expectedExtraFile)).digest('hex'):undefined,
            extra_ca_count:extra?.length,extra_ca_contains_native_issuer:extra?.some(pem=>fingerprint(pem)===expectedCa),
            default_ca_count:defaults?.length,default_ca_contains_native_issuer:defaults?.some(pem=>fingerprint(pem)===expectedCa),
            use_system_ca_flag_present:process.execArgv.includes('--use-system-ca')};
        await retain();
        // Exercise the SAME global fetch used by Code's native MCP transport,
        // in this actual connecting extension host. Node's CA inventory alone
        // cannot prove the patched fetch/proxy certificate path trusts it.
        // One unauthenticated GET; no token, session, redirect, retry or model.
        const endpoint=new URL(raw.uri.toString());
        assert.equal(endpoint.protocol,'https:');assert.equal(endpoint.hostname,'127.0.0.1');
        assert.equal(endpoint.pathname,'/mcp');assert.equal(endpoint.username,'');assert.equal(endpoint.password,'');
        assert.equal(endpoint.search,'');assert.equal(endpoint.hash,'');
        const http=vscode.workspace.getConfiguration('http');
        const networkSettings=Object.fromEntries(['proxySupport','systemCertificates','systemCertificatesNode',
            'experimental.systemCertificatesV2','fetchAdditionalSupport','electronFetch'].map(key=>[key,http.get(key)]));
        const probe={kind:'same_extension_host_global_fetch',method:'GET',authorization_sent:false,
            session_sent:false,redirect_policy:'manual',dispatches:1,
            patched_fetch_installed:typeof globalThis.__vscodePatchedFetch==='function',network_settings:networkSettings};
        evidence.actualHostUnauthenticatedFetch=probe;
        const probeUntil=Math.min(deadline,Date.now()+10000),probeStarted=Date.now(),probeAbort=new AbortController();
        const probeTimer=setTimeout(()=>probeAbort.abort(),Math.max(1,probeUntil-Date.now()));
        try{
            const response=await bounded(globalThis.fetch(endpoint.href,{method:'GET',
                headers:{Accept:'application/json'},credentials:'omit',redirect:'manual',cache:'no-store',
                signal:probeAbort.signal}),probeUntil);
            probe.http_response_observed=true;probe.status=response.status;
            probe.same_response_url=response.url===endpoint.href;
            probe.credential_required=response.status===401;
            // Do not retain bodies or headers that could acquire secret/session
            // material. A completed verified HTTPS response is the observation.
            if(response.body)try{await bounded(response.body.cancel(),probeUntil);}
            catch{probe.response_body_cancel_incomplete=true;}
        }catch(error){
            probe.http_response_observed=false;probe.error_chain=[];
            const seen=new Set();
            for(let cause=error;cause!==undefined&&cause!==null&&probe.error_chain.length<5&&!seen.has(cause);cause=cause.cause){
                seen.add(cause);probe.error_chain.push({name:String(cause.name??typeof cause).slice(0,128),
                    code:typeof cause.code==='string'?cause.code.slice(0,128):undefined,
                    message:String(cause.message??cause).slice(0,2048)});
            }
        }finally{
            clearTimeout(probeTimer);probeAbort.abort();probe.elapsed_ms=Date.now()-probeStarted;await retain();
        }
        const before=vscode.lm.tools.filter(tool=>tool.tags.includes('mcp'));
        assert.equal(before.length,0,'Isolated host must have no other cached MCP tool catalog');
        const commands=await vscode.commands.getCommands(true);
        assert.ok(commands.includes('workbench.mcp.startServer')&&commands.includes('workbench.mcp.stopServer'));
        evidence.start_dispatches=1;await retain();
        await bounded(vscode.commands.executeCommand('workbench.mcp.startServer',id,{waitForLiveTools:true}));
        evidence.started=true;
        let tools=[];
        for(let observations=0;observations<200;observations++){
            tools=vscode.lm.tools.filter(tool=>tool.tags.includes('mcp'));
            if(tools.length)break;
            assert.ok(Date.now()+100<deadline);await sleep(100);
        }
        assert.ok(tools.length>0,'Actual Code MCP host must publish its discovered tools');
        evidence.catalog=tools.map(tool=>({name:tool.name,inputSchema:tool.inputSchema,tags:tool.tags}));await retain();
        // Reviewed Code1.145.2 projection: first isolated 'airk' server bucket
        // yields 'mcp_airk_', then exact original tool name (dots -> '_', max64).
        // Compare the entire catalog; suffixes would confuse search/draft_search.
        const projectName=name=>('mcp_airk_'+name).replaceAll('.','_').slice(0,64);
        const expectedNames=UNIFIED_TOOLS.map(projectName);
        assert.equal(new Set(expectedNames).size,UNIFIED_TOOLS.length);
        assert.deepEqual(tools.map(tool=>tool.name).sort(),expectedNames.sort(),'Exact isolated AIRK original-tool identity catalog');
        evidence.originalToolMapping=Object.fromEntries(UNIFIED_TOOLS.map(name=>[name,projectName(name)]));
        const toolFor=name=>{
            assert.ok(UNIFIED_TOOLS.includes(name));
            const exact=projectName(name),found=tools.filter(tool=>tool.name===exact);
            assert.equal(found.length,1,'One exact original-tool identity in actual MCP catalog: '+name);return found[0].name;
        };
        const bodyOf=result=>{
            const parts=result.content.filter(part=>typeof part.value==='string');
            assert.equal(parts.length,1,'Retain one complete native JSON text result');
            const parsed=JSON.parse(parts[0].value);assert.ok(parsed&&typeof parsed==='object');return parsed;
        };
        const call=async(name,input,negative=false)=>{
            const exact={response_mode:'full',...input},tool=toolFor(name),original=Math.min(deadline,Date.now()+60000);
            for(let attempt=0;attempt<41;attempt++){
                assert.ok(Date.now()<original,'Original actual Code read deadline');
                const result=await bounded(vscode.lm.invokeTool(tool,{input:exact,toolInvocationToken:undefined}),original);
                // Complete public tool result retained; raw bearer headers are never recorded.
                const raw=JSON.parse(JSON.stringify(result));const body=bodyOf(result);
                const exposedProperties=Object.getOwnPropertyNames(result);
                const actualFlags=Object.fromEntries(['isError','hasError'].filter(key=>key in result).map(key=>[key,result[key]]));
                evidence.calls.push({name,input:exact,result:raw,exposedProperties,actualFlags});await retain();
                // Normalize text-part spelling only. Code's public LM bridge can
                // omit the original MCP isError; do not manufacture that flag
                // from body.status or equate the separate hasError signal.
                const normalized={...raw,content:[{type:'text',text:JSON.stringify(body)}]};
                if('isError' in result)normalized.isError=result.isError;
                const delay=readRetryDelay(normalized);
                if(!negative&&delay!==undefined&&Date.now()+delay<original){await sleep(delay);continue;}
                if(!negative&&(body.status==='retry'||body.error||actualFlags.isError===true||actualFlags.hasError===true))throw Error('Actual Code result refused the read; no further replay is authorized by its exposed contract');
                return body;
            }
            throw Error('Actual Code MCP typed read attempt bound');
        };
        const search=await call('search',{query:'WindowAlphaEvidence',mode:'lexical',limit:5});validate(search,false);
        assert.ok(search.hits.some(hit=>hit.citation.path==='fixture.ts'&&hit.text.includes('WindowAlphaEvidence')));
        const read=await call('read_file',{path:'fixture.ts',start_line:1,end_line:1});validate(read,false);
        assert.ok(read.hits.some(hit=>hit.citation.path==='fixture.ts'&&hit.text.includes('WindowAlphaEvidence')));
        const hidden=await call('read_file',{path:'hidden.ts',start_line:1,end_line:1},true);validate(hidden,true);
        const escaped=await call('read_file',{path:'../workspace-b/fixture.ts',start_line:1,end_line:1},true);validate(escaped,true);
        evidence.passed=true;
    }catch(error){evidence.passed=false;evidence.error={name:error.name,message:String(error.message)};throw error;}
    finally{
        try{if(evidence.start_dispatches&&id){evidence.stop_dispatches=1;await bounded(vscode.commands.executeCommand('workbench.mcp.stopServer',id));evidence.stopped=true;}}
        finally{await retain();}
    }
};
