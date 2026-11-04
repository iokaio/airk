// SPDX-License-Identifier: Apache-2.0
// Qualification of retained installer observations, not a source of authority.
const {createHash}=require('node:crypto');
const path=require('node:path');
const same=require('node:util').isDeepStrictEqual;
const digest=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const identity=value=>typeof value==='string'&&/^[a-f0-9]{32}$/.test(value);
const requireValue=(value,message)=>{if(!value)throw new Error(message);};
function framed(domain,fields){
    const hash=createHash('sha256').update('AIRK-DIGEST-1\0');
    for(const field of [Buffer.from(domain),...fields]){const size=Buffer.alloc(8);size.writeBigUInt64BE(BigInt(field.length));hash.update(size).update(field);}
    return hash.digest('hex');
}
function pairIdentity(pair){
    requireValue(pair?.schema===1&&['version','os','arch'].every(key=>typeof pair[key]==='string')&&digest(pair.airk_sha256)&&digest(pair.embed_sha256),'Invalid native pair identity');
    const canonical={schema:pair.schema,version:pair.version,os:pair.os,arch:pair.arch,airk_sha256:pair.airk_sha256,embed_sha256:pair.embed_sha256};
    requireValue(same(pair,canonical),'Unknown native pair fields');
    return framed('airk-maintenance-engine/1.4',[Buffer.from(JSON.stringify(canonical))]);
}
function samePath(a,b){
    if(typeof a!=='string'||typeof b!=='string')return false;
    const normalize=value=>process.platform==='win32'?path.toNamespacedPath(path.resolve(value)).toLowerCase():path.resolve(value);
    return normalize(a)===normalize(b);
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
    const catalog=observed.catalog,basis=observed.basis;
    requireValue(catalog?.schema===12&&catalog.authority_contract===1&&catalog.policy_contract===0&&catalog.activated===false&&catalog.accepted_new_writes===false&&catalog.namespace===plan.namespace&&catalog.upgrade_id===plan.id&&Number.isSafeInteger(catalog.revision)&&catalog.revision>1,'Qualification did not observe the inactive authority-1 catalog');
    requireValue(basis?.kind==='legacy_upgrade'&&basis.upgrade_id===plan.id&&basis.plan_digest===receipt.plan_digest&&digest(basis.seal_digest)&&basis.native_host===receipt.authority_v1_host&&digest(basis.native_host),'Sealed compatibility basis differs');
    requireValue(base?.installer_identity===expected.candidateIdentity&&base.catalog?.authority_contract===0&&base.catalog.namespace===plan.namespace&&base.catalog.upgrade_id===plan.id&&base.catalog.revision+1===catalog.revision&&base.pending_source_imports===0&&base.pending_context_imports===0,'Original P5 readiness is incomplete or crossed');
    requireValue(Array.isArray(plan.indexes)&&plan.indexes.length>0&&plan.indexes.length<=1024&&base.indexes===plan.indexes.length,'Original P5 index coverage differs');
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
    return {status:'qualified_native_fixture',scope:'Actual native installer transition and retained complete observations; installed-editor execution remains a separate check'};
}
module.exports={pairIdentity,qualifyLegacyNative};
