// SPDX-License-Identifier: Apache-2.0
// Read-only actual RuntimeHost/CLI parity inside an already owned upgrade fixture.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const sha=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const inside=(root,value)=>{const relative=path.relative(root,value);return relative!==''&&!relative.startsWith('..'+path.sep)&&relative!=='..'&&!path.isAbsolute(relative);};
let stream,evidence={schema:1,status:'failed'};
const same=(a,b)=>fs.realpathSync.native(a)===fs.realpathSync.native(b);
function moduleInventory(root){
    const values={};let bytes=0;
    function visit(folder){for(const row of fs.readdirSync(folder,{withFileTypes:true})){
        if(row.name==='test')continue;const file=path.join(folder,row.name);assert(!row.isSymbolicLink());
        if(row.isDirectory())visit(file);else if(row.isFile()&&row.name.endsWith('.js')){
            const stat=fs.statSync(file);bytes+=stat.size;assert(bytes<=64*1024*1024&&Object.keys(values).length<2048);
            values[path.relative(root,file).split(path.sep).join('/')]=sha(file);
        }
    }}
    visit(root);return values;
}
async function main(){
    assert.deepEqual(process.argv.slice(2).filter((_,i)=>i%2===0),['--input','--output']);
    const inputFile=fs.realpathSync.native(process.argv[3]),outputFile=path.resolve(process.argv[5]);
    assert(fs.statSync(inputFile).size<=3*65536);
    const spec=JSON.parse(fs.readFileSync(inputFile,'utf8'));assert.equal(spec.schema,1);
    assert(['review','authorized','applied'].includes(spec.phase));
    const fixture=fs.realpathSync.native(spec.fixture),parent=fs.realpathSync.native(spec.fixture_parent);
    assert(inside(parent,fixture)&&inside(fixture,inputFile)&&inside(fixture,outputFile));
    assert.equal(fs.realpathSync.native(path.dirname(outputFile)),fixture);
    stream=fs.openSync(outputFile,'wx');
    Object.assign(evidence,{phase:spec.phase,fixture,install:spec.install,control:spec.control});
    assert(same(spec.install,path.join(fixture,'install'))&&same(spec.control,path.join(fixture,'state/control')));
    assert(inside(fixture,fs.realpathSync.native(spec.install))&&inside(fixture,fs.realpathSync.native(spec.control)));
    const modules=fs.realpathSync.native(spec.modules),extension=fs.realpathSync.native(spec.extension);
    assert(!inside(fixture,modules)&&!inside(fixture,extension));
    const beforeModules=moduleInventory(modules),inputSha=sha(inputFile);
    const {RuntimeHost,nativeRunner,verifyBundledRuntime}=require(path.join(modules,'serviceBootstrap.js'));
    const {validateRetirementReply,retirementRequest,retirementJson,retirementMarkdown}=require(path.join(modules,'runtimeLegacyRetirement.js'));
    const bundle=await verifyBundledRuntime(extension,'1.8.8');
    assert.equal(bundle.digest,spec.runtime_manifest_sha256);assert.equal(sha(bundle.main),spec.runtime_main_sha256);
    assert.equal(sha(path.join(spec.runtime,'airk-install.json')),bundle.digest);
    assert.equal(sha(path.join(spec.runtime,'airk.exe')),spec.runtime_main_sha256);
    const expected=validateRetirementReply(spec.expected),request=retirementRequest(expected);
    assert.equal(expected.status,spec.phase);assert.deepEqual(request,spec.request);
    assert(/^idx_[a-f0-9]{32}$/.test(spec.request.index_id));
    const calls=[],exports=[],deadline=Date.now()+120000;
    Object.assign(evidence,{complete_native_calls:calls,exports});
    function exported(reply,name){
        validateRetirementReply(reply);const json=retirementJson(reply),markdown=retirementMarkdown(reply);
        assert.deepEqual(JSON.parse(json),reply);
        const marker='\n```json\n',start=markdown.indexOf(marker);assert(start>=0&&markdown.endsWith('\n```\n'));
        assert.deepEqual(JSON.parse(markdown.slice(start+marker.length,-5)),reply);
        assert(markdown.includes('Physical cleanup remains pending')&&markdown.includes('not current activation readiness'));
        exports.push({name,json,markdown});
    }
    const readonlyRunner=async(command,args,timeout,input)=>{
        assert.equal(command,bundle.main);assert(Array.isArray(args));
        const prefix=['runtime',args[1],'--install-dir',spec.install];
        if(args[1]==='location'){assert.deepEqual(args,['runtime','location','--channel','stable']);assert.equal(input,undefined);}
        else if(spec.phase==='review'&&args[1]==='legacy-retirement-candidates'){
            assert.deepEqual(args,prefix);assert.equal(input,undefined);
        }else if(spec.phase==='review'&&args[1]==='legacy-retirement-review'){
            assert.deepEqual(args,[...prefix,'--index-id',spec.request.index_id]);assert.equal(input,undefined);
        }else if(spec.phase!=='review'&&args[1]==='legacy-retirement-receipt'){
            assert.deepEqual(args,prefix);assert.equal(input,JSON.stringify(spec.request));
        }else throw new Error('Readonly parity probe refused native command before dispatch: '+JSON.stringify(args));
        const left=deadline-Date.now();assert(left>0,'Shared readonly parity deadline expired');
        const effectiveTimeout=Math.min(timeout??60000,left,60000);
        const call={command,args:[...args],input:input??null,timeout_ms:effectiveTimeout};calls.push(call);
        let result;
        try{result=await nativeRunner(command,args,effectiveTimeout,input);}
        catch(error){call.error={name:error.name,message:String(error),code:error.code,signal:error.signal,killed:error.killed,timedOut:error.timedOut,stderr:error.stderr};throw error;}
        assert(Buffer.byteLength(JSON.stringify(result))<=2*1024*1024);
        call.result=structuredClone(result);
        return result; // Canonical actual CLI result, never a synthetic or projected response.
    };
    const host=new RuntimeHost(extension,'1.8.8',{channel:'stable',installDir:spec.install},readonlyRunner,()=>true);
    if(spec.phase==='review'){
        const page=await host.legacyRetirementCandidates();
        assert.deepEqual(page,calls.at(-1).result);assert(page.items.some(row=>row.index_id===spec.request.index_id));
        const reply=await host.legacyRetirementReview(spec.request.index_id);
        assert.deepEqual(reply,calls.at(-1).result);
        const actual=retirementRequest(reply);
        for(const field of ['caller','upgrade_id','plan_digest','installer_identity','index_id','registration','expected_authorizations_digest'])assert.deepEqual(actual[field],request[field]);
        assert.equal(actual.key.namespace,request.key.namespace);assert.equal(actual.key.epoch,request.key.epoch);
        exported(reply,'actual_host_native_review');exported(expected,'saved_original_native_review');
    }else{
        const reply=await host.legacyRetirementReceipt(spec.request),wire=calls.at(-1).result;
        assert.equal(wire.schema,1);assert(Object.hasOwn(wire,'observation'));
        assert.notEqual(reply,null);assert.deepEqual(reply,wire.observation);assert.deepEqual(reply,expected);
        assert.deepEqual(retirementRequest(reply),spec.request);exported(reply,'actual_host_native_'+spec.phase);
    }
    assert.equal(sha(inputFile),inputSha);assert.deepEqual(moduleInventory(modules),beforeModules);
    assert.equal(sha(bundle.main),spec.runtime_main_sha256);
    return {schema:1,status:'passed',phase:spec.phase,fixture,install:spec.install,control:spec.control,
        source:'Actual compiled RuntimeHost with canonical nativeRunner and same verified candidate; readonly only',
        module_inputs:beforeModules,runtime_manifest_sha256:bundle.digest,runtime_main_sha256:spec.runtime_main_sha256,
        input_sha256:inputSha,complete_native_calls:calls,exports,
        limits:['Review native nonce is new: complete samecall raw reply compared; no second Review normalized to old nonce.',
            'Receipt wire schema wrapper retained in raw calls; complete observation CoreReply compared/exported.',
            'This probe performs no authorization, activation, service start, indexing, source write or mutation replay.']};
}
main().then(value=>fs.writeFileSync(stream,JSON.stringify(value,null,2)+'\n')).catch(error=>{
    if(stream!==undefined)fs.writeFileSync(stream,JSON.stringify({...evidence,status:'failed',error:String(error),stack:error.stack},null,2)+'\n');
    console.error(error);process.exitCode=1;
}).finally(()=>{if(stream!==undefined)fs.closeSync(stream);});
