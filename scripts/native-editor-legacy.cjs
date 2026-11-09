// SPDX-License-Identifier: Apache-2.0
// Qualification of retained installer observations, not a source of authority.
const {createHash}=require('node:crypto');
const path=require('node:path');
const same=require('node:util').isDeepStrictEqual;
const digest=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const identity=value=>typeof value==='string'&&/^[a-f0-9]{32}$/.test(value);
const requireValue=(value,message)=>{if(!value)throw new Error(message);};
function requireLegacyInput(scenarios,environment){
    requireValue(!scenarios.includes('legacy')||(typeof environment.AIRK_EDITOR_LEGACY_FIXTURE==='string'&&environment.AIRK_EDITOR_LEGACY_FIXTURE.trim().length>0),'Selected legacy scenario requires an authentic matched AIRK_EDITOR_LEGACY_FIXTURE before any editor run');
}
function framed(domain,fields){
    const hash=createHash('sha256').update('AIRK-DIGEST-1\0');
    for(const field of [Buffer.from(domain),...fields]){const size=Buffer.alloc(8);size.writeBigUInt64BE(BigInt(field.length));hash.update(size).update(field);}
    return hash.digest('hex');
}
function pairIdentity(pair){
    requireValue([1,2].includes(pair?.schema)&&['version','os','arch'].every(key=>typeof pair[key]==='string')&&digest(pair.airk_sha256)&&(pair.schema===2||digest(pair.embed_sha256)),'Invalid native executable identity');
    const canonical={schema:pair.schema,version:pair.version,os:pair.os,arch:pair.arch,airk_sha256:pair.airk_sha256,...(pair.schema===1?{embed_sha256:pair.embed_sha256}:{})};
    requireValue(same(pair,canonical),'Unknown native executable fields');
    return framed('airk-maintenance-engine/1.4',[Buffer.from(JSON.stringify(canonical))]);
}
/** Metadata is checked against already hashed native payload/inspection bytes;
 * an original launcher is provenance, not the modern transport destination. */
function qualifyCandidateRuntime(metadata,candidate,inspection,inventory,legacyConfig){
    const engineIdentity=pairIdentity(metadata),main=metadata.os==='macos'?'AIRK.app/Contents/MacOS/airk':metadata.os==='windows'?'airk.exe':metadata.os==='linux'?'airk':undefined;
    requireValue(main&&inventory[main]===metadata.airk_sha256&&candidate.engine_sha256===metadata.airk_sha256,'Candidate main differs from its actual inventory');
    if(metadata.schema===2){
        const retired=new Set(['airk-embed','airk-embed.exe','airk-launcher-image','airk-launcher','airk-launcher.exe','airk-service','airk-service.exe','airk-index','airk-index.exe','airk-source','airk-source.exe','onnxruntime.dll','libonnxruntime.so','libonnxruntime.dylib']);
        requireValue(candidate.helper_sha256==null&&!Object.keys(inventory).some(name=>retired.has(path.posix.basename(name)))&&inspection.launcher===null,'Modern candidate must have no helper or launcher');
    }else{
        const helper=metadata.os==='macos'?'AIRK.app/Contents/MacOS/airk-embed':metadata.os==='windows'?'airk-embed.exe':'airk-embed';
        requireValue(inventory[helper]===metadata.embed_sha256&&candidate.helper_sha256===metadata.embed_sha256,'Historical candidate helper differs');
        if(legacyConfig.binding_kind==='stable_launcher')requireValue(samePath(inspection.launcher,legacyConfig.launcher_binding?.installation_launcher?.path),'Historical candidate launcher binding differs');
    }
    return {engineIdentity,runtimeSchema:metadata.schema,registrySchema:metadata.schema===2?13:12,transport:metadata.schema===2?'https':'historical_stdio'};
}
function samePath(a,b){
    if(typeof a!=='string'||typeof b!=='string')return false;
    const normalize=value=>process.platform==='win32'?path.toNamespacedPath(path.resolve(value)).toLowerCase():path.resolve(value);
    return normalize(a)===normalize(b);
}
function qualifyOriginalConnection(value){
    const connection=value.original_connection,scope=connection?.scope;
    requireValue(scope&&digest(scope.binding)&&samePath(scope.root,value.source_root)&&samePath(scope.data_root,value.data_root)&&typeof scope.collection==='string'&&scope.collection.length>0,'Original imported registration differs');
    const expected=['runtime','launch','--install-dir',value.install_dir,'--','--root',scope.root,'--data-dir',scope.data_root,'--collection',scope.collection,'--service-project',scope.binding,'mcp'];
    requireValue(same(connection.args,expected)&&same(value.legacy_config?.args,expected),'Original installation launcher forwarding or scope differs');
    requireValue(connection.migration_provenance==='imported_legacy_snapshot'&&connection.retrieval_authority==='sealed_imported_snapshot','Reviewed catalog migration cannot substitute for imported legacy authority');
    return scope;
}
/** A retired command remains provenance only. No executable is retained or
 * launched to verify it: hashed original native metadata and ownership receipts
 * must agree with complete native retirement of that exact installation. */
function qualifyRetiredLauncher(value,snapshot){
    const config=value.legacy_config,previous=value.predecessor?.install_manifest_sha256,candidate=value.candidate?.install_manifest_sha256;
    requireValue(config?.binding_kind==='retired_installation_launcher'&&snapshot?.schema===1&&digest(previous)&&digest(candidate)&&previous!==candidate,'Distinct retired launcher transition required');
    for(const key of ['manifest_text','runtime_text','launcher_receipt_text'])requireValue(typeof snapshot[key]==='string'&&Buffer.byteLength(snapshot[key])<=1048576,'Bounded original predecessor metadata required');
    const hash=text=>createHash('sha256').update(text).digest('hex');
    requireValue(hash(snapshot.manifest_text)===previous,'Predecessor manifest snapshot changed');
    const manifest=JSON.parse(snapshot.manifest_text),metadata=JSON.parse(snapshot.runtime_text),receipt=JSON.parse(snapshot.launcher_receipt_text);
    pairIdentity(metadata);
    requireValue(metadata.schema===1&&manifest.files?.['airk-runtime.json']===hash(snapshot.runtime_text),'Original predecessor pair metadata differs');
    const main=metadata.os==='windows'?'airk.exe':metadata.os==='macos'?'AIRK.app/Contents/MacOS/airk':'airk';
    const helper=metadata.os==='windows'?'airk-embed.exe':metadata.os==='macos'?'AIRK.app/Contents/MacOS/airk-embed':'airk-embed';
    requireValue(manifest.files[main]===metadata.airk_sha256&&manifest.files[helper]===metadata.embed_sha256,'Predecessor inventory differs from its original pair');
    const command=path.join(value.install_dir,metadata.os==='windows'?'airk-launcher.exe':'airk-launcher');
    requireValue(samePath(config.command,command)&&digest(snapshot.command_sha256)&&same(receipt,{schema:1,sha256:snapshot.command_sha256}),'Original installation launcher ownership differs');
    const inspection=snapshot.native_inspect;
    requireValue(inspection?.payload_verified===true&&inspection.approval?.manifest_sha256===previous&&inspection.selection?.active===previous&&samePath(inspection.launcher,command)&&samePath(inspection.executable,path.join(value.install_dir,'runtimes',previous,main)),'Original native inspection does not bind this predecessor launcher');
    const retirement=value.native?.obsolete_cleanup,launchers=retirement?.launchers,runtimes=retirement?.runtimes;
    requireValue(Array.isArray(value.native?.parsed_evidence)&&value.native.parsed_evidence.some(record=>same(record.value,retirement)),'Complete native retirement absent from hashed evidence');
    requireValue(launchers?.active===candidate&&launchers.legacy_launchers_retired===true&&launchers.launcher===null&&launchers.universal_launcher===null&&launchers.processes?.scope==='verified_launcher_images'&&launchers.processes.remaining===0,'Native launcher retirement incomplete');
    requireValue(runtimes?.active===candidate&&runtimes.complete===true&&runtimes.selected_service_restarted===false&&Array.isArray(runtimes.retired_runtimes)&&runtimes.retired_runtimes.includes(previous)&&!runtimes.retired_runtimes.includes(candidate)&&runtimes.processes?.scope==='verified_superseded_runtime_images'&&runtimes.processes.remaining===0,'Native predecessor retirement incomplete');
    return metadata;
}
async function verifyRetiredPaths(value){
    const fs=require('node:fs/promises'),install=await fs.realpath(value.install_dir);
    requireValue(samePath(install,value.install_dir),'Retired installation path changed');
    for(const file of [value.legacy_config.command,path.join(install,'launcher.json'),path.join(install,'runtimes',value.predecessor.install_manifest_sha256)]){
        const parent=await fs.realpath(path.dirname(file));
        requireValue(samePath(parent,path.dirname(file)),'Retired image parent changed');
        try{await fs.lstat(file);throw new Error('Retired launcher or runtime still exists: '+file);}
        catch(error){if(error.code!=='ENOENT')throw error;}
    }
    requireValue(same((await fs.readdir(path.join(install,'runtimes'))).sort(),[value.candidate.install_manifest_sha256]),'Superseded payload remains in upgraded installation');
}
function qualifyLegacyNative(value,expected){
    const {native,candidate,predecessor}=value,receipt=native?.upgrade_receipt;
    const retained=observation=>native.parsed_evidence.some(record=>same(record.value,observation));
    requireValue(digest(expected.candidateIdentity)&&digest(expected.predecessorIdentity)&&expected.candidateIdentity!==expected.predecessorIdentity,'Distinct verified native artifacts required');
    requireValue(predecessor&&digest(predecessor.install_manifest_sha256)&&predecessor.install_manifest_sha256!==candidate.install_manifest_sha256,'Actual preceding install identity required');
    for(const observation of [receipt,native.activation_receipt,native.post_health,native.inspect])requireValue(observation&&retained(observation),'Complete native observation absent from hashed evidence');
    const activation=native.activation_receipt;
    requireValue(activation.schema===1&&activation.outcome==='selected'&&samePath(activation.install,value.install_dir)&&activation.candidate===candidate.install_manifest_sha256&&activation.preceding===predecessor.install_manifest_sha256&&Number.isSafeInteger(activation.observed_unix_ms)&&activation.observed_unix_ms>0,'Native activation did not select this predecessor/candidate transition');
    const plan=receipt.plan,authority=receipt.authority_v1,readiness=authority?.readiness,observed=readiness?.candidate,base=receipt.readiness?.candidate;
    requireValue(receipt.schema===1&&receipt.stage==='settled'&&identity(plan?.id)&&identity(plan?.namespace)&&samePath(plan.control,value.control_dir)&&plan.installer_identity===expected.candidateIdentity&&digest(receipt.plan_digest),'Native upgrade lineage differs');
    requireValue(authority?.phase==='settled'&&same(authority,native.authority_v1_readiness)&&observed?.schema===1&&identity(observed.nonce)&&observed.installer_identity===expected.candidateIdentity,'Native authority readiness differs');
    const catalog=observed.catalog,basis=observed.basis,runtimeSchema=expected.runtimeSchema??1;
    // Native migration persists schema 12; the first in-process provider write
    // may promote it to 13. Retained preactivation readiness is not live schema.
    const registrySchemas=runtimeSchema===2?[12,13]:runtimeSchema===1?[12]:[];
    requireValue([1,2].includes(runtimeSchema)&&registrySchemas.includes(catalog?.schema)&&catalog.authority_contract===1&&catalog.policy_contract===0&&catalog.activated===false&&catalog.accepted_new_writes===false&&catalog.namespace===plan.namespace&&catalog.upgrade_id===plan.id&&Number.isSafeInteger(catalog.revision)&&catalog.revision>1,'Qualification did not observe the inactive authority-1 catalog');
    requireValue(basis?.kind==='legacy_upgrade'&&basis.upgrade_id===plan.id&&basis.plan_digest===receipt.plan_digest&&digest(basis.seal_digest)&&basis.native_host===receipt.authority_v1_host&&digest(basis.native_host),'Sealed compatibility basis differs');
    requireValue(base?.installer_identity===expected.candidateIdentity&&registrySchemas.includes(base.catalog?.schema)&&base.catalog.schema<=catalog.schema&&base.catalog.authority_contract===0&&base.catalog.namespace===plan.namespace&&base.catalog.upgrade_id===plan.id&&base.catalog.revision+1===catalog.revision&&base.pending_source_imports===0&&base.pending_context_imports===0,'Original P5 readiness is incomplete or crossed');
    requireValue(Array.isArray(plan.indexes)&&plan.indexes.length>0&&plan.indexes.length<=1024&&base.indexes===plan.indexes.length,'Original P5 index coverage differs');
    if(value.legacy_config?.binding_kind==='retired_installation_launcher'){
        const original=qualifyOriginalConnection(value);
        requireValue(plan.indexes.some(index=>same(index.registration,original)),'Original connection differs from complete native upgrade registration');
    }
    const args=value.legacy_config?.args??[],at=args.indexOf('--service-project');
    requireValue(at>=0&&args.filter(arg=>arg==='--service-project').length===1&&plan.indexes.some(index=>index.registration?.binding===args[at+1]&&samePath(index.registration.root,value.source_root)&&samePath(index.registration.data_root,value.data_root)),'Original manual connection is outside the native upgrade roster');
    requireValue(Array.isArray(readiness.required_predecessors)&&same(readiness.required_predecessors,[expected.predecessorIdentity]),'Predecessor qualification coverage differs');
    for(const refusals of [receipt.readiness.predecessor_refusals,readiness.predecessor_refusals]){
        requireValue(Array.isArray(refusals)&&refusals.length===1&&refusals[0].payload_identity===expected.predecessorIdentity,'Native predecessor refusal is incomplete');
        const refusal=refusals[0],provenance=refusal.native_provenance,entries=provenance?.native_entrypoints;
        requireValue(['service_requests_list','hidden_upgrade_probe'].includes(entries?.read)&&['service_requests_forget','hidden_upgrade_migrate'].includes(entries?.write),'Unsupported actual predecessor entry points');
        for(const direction of ['read','write']){
            const diagnostic=provenance[direction];
            requireValue(digest(diagnostic?.final_classified_diagnostic_digest)&&(diagnostic.initial_admission_pending_digest===undefined||digest(diagnostic.initial_admission_pending_digest)),'Incomplete native refusal diagnostic');
        }
        const diagnostic=direction=>({...(provenance[direction].initial_admission_pending_digest===undefined?{}:{initial_admission_pending_digest:provenance[direction].initial_admission_pending_digest}),final_classified_diagnostic_digest:provenance[direction].final_classified_diagnostic_digest});
        const canonical={native_entrypoints:{read:entries.read,write:entries.write},read:diagnostic('read'),write:diagnostic('write')};
        requireValue(same(canonical,provenance),'Unknown native refusal provenance fields');
        const bytes=Buffer.from(JSON.stringify(canonical));
        for(const direction of ['read','write'])requireValue(refusal[direction+'_refusal_digest']===framed('airk/catalog-native-refusal-provenance/1',[Buffer.from(expected.predecessorIdentity,'hex'),bytes,Buffer.from(direction)]),'Native refusal provenance digest differs');
    }
    const health=native.post_health;
    requireValue(health.status==='health'&&health.observation?.runtime_sha256===candidate.install_manifest_sha256&&health.observation.executable_sha256===candidate.engine_sha256&&identity(health.observation.instance)&&health.observation.policy?.accepting===true&&health.observation.catalog_clock?.namespace===plan.namespace&&health.observation.catalog_clock.revision===catalog.revision&&native.inspect.registry_namespace===plan.namespace,'Post-activation native health differs from the qualified catalog');
    return {status:'qualified_native_fixture',candidate_runtime_schema:runtimeSchema,candidate_transport:runtimeSchema===2?'https':'historical_stdio',original_configuration:'preserved_predecessor_bytes',scope:'Actual native installer transition and retained complete observations; installed-editor HTTPS migration/execution remains a separate check'};
}
/** A passing predecessor refusal is useful evidence, but cannot qualify a new
 * transport's retrieval. The actual driver retains all canonical probe bodies. */
function qualifyLegacyEditorEvidence(evidence,fixture){
    if(fixture.legacy_config.binding_kind==='pinned_predecessor'){
        requireValue(evidence.legacyTransport==='retained_pinned_stdio_refusal'&&evidence.legacyRetrievalParity===false&&evidence.legacyMigration===undefined,'Pinned refusal cannot claim HTTPS migration');
        return {legacyUpgradeCoverage:false,legacyPinnedRefusalCoverage:true,legacyOriginalConfigurationPreserved:true,transport:'retained_pinned_stdio_refusal'};
    }
    const migration=evidence.legacyMigration,connection=migration?.connection,selection=connection?.selection?.selection,plan=fixture.native.upgrade_receipt.plan;
    const args=fixture.legacy_config.args,project=args[args.indexOf('--service-project')+1],registration=plan.indexes.find(index=>index.registration.binding===project)?.registration;
    requireValue(fixture.candidate_runtime?.schema===2&&evidence.legacyTransport==='https_imported_migration'&&evidence.legacyRetrievalParity===true&&migration?.original_config_unchanged===true&&migration.original_config_sha256===fixture.legacy_config.sha256&&migration.authority_basis==='sealed_imported_snapshot','Complete actual imported HTTPS migration evidence required');
    requireValue(registration&&same(migration.registration,registration)&&connection.schema===1&&identity(connection.connection_id)&&connection.selection.kind==='imported_legacy'&&same(selection.registration,registration)&&selection.configuration_receipt===fixture.legacy_config.sha256&&selection.namespace===plan.namespace,'HTTPS migration changed original configuration authority');
    const endpoint=connection.endpoint,provenance=selection.grant?.provenance;
    requireValue(endpoint?.schema===1&&endpoint.state==='ready'&&endpoint.namespace===plan.namespace&&identity(endpoint.service_instance)&&endpoint.executable_sha256===fixture.candidate.engine_sha256&&/^https:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})\/mcp$/.test(endpoint.url)&&Number(new URL(endpoint.url).port)<=65535,'HTTPS migration native endpoint differs');
    requireValue(provenance?.kind==='imported_legacy_snapshot'&&provenance.legacy_binding===registration.binding&&provenance.upgrade_id===plan.id&&provenance.plan_digest===fixture.native.upgrade_receipt.plan_digest,'HTTPS migration lacks the original sealed imported provenance');
    requireValue(Array.isArray(evidence.native)&&evidence.native.some(value=>value?.legacyProbe&&typeof value.legacyProbe==='object'),'Complete native legacy retrieval probe absent');
    return {legacyUpgradeCoverage:true,legacyPinnedRefusalCoverage:false,legacyOriginalConfigurationPreserved:true,transport:'https_imported_migration',connection_id:connection.connection_id,original_config_sha256:fixture.legacy_config.sha256};
}
module.exports={requireLegacyInput,pairIdentity,qualifyCandidateRuntime,qualifyRetiredLauncher,verifyRetiredPaths,qualifyOriginalConnection,qualifyLegacyNative,qualifyLegacyEditorEvidence};
