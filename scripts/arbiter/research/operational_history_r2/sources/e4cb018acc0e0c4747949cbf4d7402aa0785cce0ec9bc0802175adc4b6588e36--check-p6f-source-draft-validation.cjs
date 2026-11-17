// SPDX-License-Identifier: Apache-2.0
'use strict';
// Actual native capture -> compiled ServiceClient validator qualification.
// No native process, response producer, Console transport or editor is mocked
// as qualified: the positive response is the unchanged external native capture.
const fs=require('node:fs/promises'),path=require('node:path');
const assert=require('node:assert/strict'),{createHash}=require('node:crypto');
const repo=path.resolve(__dirname,'../..');
const hash=value=>createHash('sha256').update(value).digest('hex');
const clone=value=>structuredClone(value);
const arg=name=>{const i=process.argv.indexOf(name);assert.ok(i>=0&&process.argv[i+1],`Missing ${name}`);return path.resolve(process.argv[i+1]);};
const flip=value=>value.slice(0,-1)+(value.endsWith('0')?'1':'0');
const started=Date.now(),deadline=started+10000;
const budget=()=>assert.ok(Date.now()<deadline,'Source draft capture validation deadline exceeded');
async function main(){
    const casesPath=arg('--cases'),reportPath=arg('--report'),bytes=await fs.readFile(casesPath);
    assert.ok(bytes.length<=128*1024,'Source draft capture exceeds 128KiB');
    const captured=JSON.parse(bytes);
    assert.equal(captured.schema,1);assert.equal(captured.fixture_kind,'compiled_schema12_authority1_handler_transport_only');
    assert.equal(captured.coverage,'actual_native_readonly_capture_to_compiled_service_client');
    assert.equal(captured.response.status,'catalog_source_mutation_draft');
    const selection=captured.input.catalog_source_mutation_draft.selection;
    assert.equal(selection.change.operation,'add_root');
    const modules=['serviceClient','indexingCatalog','nativeGate'],sourceHashes={},compiledHashes={};
    for(const name of modules){sourceHashes[name]=hash(await fs.readFile(path.join(repo,'packages/vscode/src',name+'.ts')));compiledHashes[name]=hash(await fs.readFile(path.join(repo,'packages/vscode/out',name+'.js')));}
    const driverHash=hash(await fs.readFile(__filename));
    const {ServiceClient}=require(path.join(repo,'packages/vscode/out/serviceClient.js'));
    const frozen=JSON.stringify(captured),cases=[];let calls=0,disposed=0;
    const report={schema:1,status:'running',fixture_kind:captured.fixture_kind,coverage:captured.coverage,
        input_sha256:hash(bytes),driver_sha256:driverHash,runtime_manifest_sha256:captured.runtime_manifest_sha256,
        source_hashes:sourceHashes,compiled_hashes:compiledHashes,
        qualification:{native_readonly_response:true,capture_transport:true,persistent_console:false,installed_editor:false,installer:false,native_apply:false},
        actual_native_input:captured.input,actual_native_response:captured.response,
        native_identity_observations:captured.native_identity_observations,cases,
        limitations:['Positive transport replays the complete unchanged actual native response captured by Python; it does not qualify persistent Console transport.',
            'Source/compiled hashes identify bytes; matching compilation provenance requires the lead build receipt.',
            'Distinct identities exercise this fixture only; native collection and registry identities may legitimately coincide.',
            'No canonicalized path equality is claimed without a native input-echo contract.']};
    async function check(id,input,response,negative){
        budget();let localCalls=0,reply;const original=JSON.stringify(response),attempt={id,negative,input:clone(input)};
        cases.push(attempt);
        const client=new ServiceClient({request:async method=>{
            budget();assert.ok(++calls<=8,'Capture validation call budget exceeded');localCalls++;
            assert.deepEqual(method,{catalog_source_mutation_draft:{selection:input}},'ServiceClient changed the supported native input');
            attempt.outgoing_native_method=clone(method);reply=clone(response);return reply;
        },dispose:()=>{disposed++;}});
        let caught;
        try{
            const review=await client.catalogSourceDraft(input);
            attempt.review=clone(review);
            assert.equal(negative,false,`Crossed native continuity accepted: ${id}`);
            assert.equal(review.response,reply,'Validator reduced or replaced the complete native response');
            assert.deepEqual(review.response,response);assert.deepEqual(review.request,response.draft.request);
            assert.equal(review.previewDigest,response.draft.preview_digest);assert.equal(review.requestDigest,response.draft.preview.request_digest);
            attempt.full_response_preserved=true;
        }catch(error){caught=error;attempt.error={name:error.name,message:error.message,code:error.code};}
        finally{client.dispose();}
        assert.equal(localCalls,1,'Each capture validation must reach its actual transport seam exactly once');
        assert.equal(JSON.stringify(response),original,'Validator mutated the source capture');
        if(negative){assert.ok(caught,`Crossed native continuity was accepted: ${id}`);assert.equal(attempt.review,undefined,'Negative must be rejected by production validation, not fixture assertion');attempt.rejected=true;}
        else if(caught)throw caught;
        attempt.passed=true;
    }
    let failure;
    try{
        const nativePlan=captured.response.draft.request.operation.plan;
        const identities={registry_namespace:selection.key.namespace,native_caller:captured.response.draft.request.caller,
            backing_binding:nativePlan.registration.binding,collection_incarnation:nativePlan.configuration.incarnation,
            collection_settings_digest:nativePlan.configuration.settings};
        assert.deepEqual(captured.native_identity_observations,identities,'Identity observations must come from the full actual native capture');
        assert.notEqual(identities.collection_incarnation,identities.registry_namespace,'Fixture failed to exercise independent collection incarnation');
        assert.notEqual(identities.collection_settings_digest,identities.backing_binding,'Fixture failed to exercise independent settings digest');
        await check('actual_native_source_draft',clone(selection),captured.response,false);
        const key=clone(selection);key.key.nonce=flip(key.key.nonce);await check('crossed_native_key',key,captured.response,true);
        const index=clone(selection);index.index_id=flip(index.index_id);await check('crossed_native_index',index,captured.response,true);
        const revision=clone(selection);revision.expected_catalog_revision++;await check('crossed_native_revision',revision,captured.response,true);
        const alias=clone(selection);alias.change.alias+='-crossed';await check('crossed_native_alias',alias,captured.response,true);
        const preview=clone(captured.response);preview.draft.preview.operation.id=flip(preview.draft.preview.operation.id);
        await check('crossed_native_request_preview',clone(selection),preview,true);
        assert.equal(JSON.stringify(captured),frozen,'Actual native input/response capture changed');
        for(const name of modules){assert.equal(hash(await fs.readFile(path.join(repo,'packages/vscode/src',name+'.ts'))),sourceHashes[name]);assert.equal(hash(await fs.readFile(path.join(repo,'packages/vscode/out',name+'.js'))),compiledHashes[name]);}
        assert.equal(hash(await fs.readFile(__filename)),driverHash,'Capture driver source changed during qualification');
        report.status='passed';
    }catch(error){failure=error;report.status='failed';report.failure={name:error.name,message:error.message,stack:error.stack};}
    report.capture_transport_calls=calls;report.disposed_capture_transports=disposed;
    report.native_processes_started=0;report.elapsed_ms=Date.now()-started;
    const output=JSON.stringify(report,null,2)+'\n';assert.ok(Buffer.byteLength(output)<=1024*1024,'Capture validation report exceeds1MiB');
    await fs.writeFile(reportPath,output,{flag:'wx'});
    process.stdout.write(JSON.stringify({schema:1,status:report.status,report:reportPath,cases:cases.length})+'\n');
    if(failure)throw failure;
}
main().catch(error=>{process.stderr.write(`Native source draft capture validation failed: ${error.name}\n`);process.exitCode=1;});
