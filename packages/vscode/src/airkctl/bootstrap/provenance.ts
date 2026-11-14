// SPDX-License-Identifier: Apache-2.0
/** Offline cryptographic verification; no key discovery, credentials or subprocesses. */
import { bundleFromJSON } from '@sigstore/bundle';
import { TrustedRoot } from '@sigstore/protobuf-specs';
import { toSignedEntity, toTrustMaterial, Verifier, type Signer } from '@sigstore/verify';
// The pinned decoder is bundled here; the sealed loader never resolves it externally.
const {uncompress}=require('snappyjs') as {uncompress(input:Buffer,maximum:number):Buffer};
import { PUBLIC_GOOD_ROOT_JSON } from './provenanceTrust';

export interface ProvenanceExpectation {
    readonly name:string; readonly sha256:string; readonly commit:string; readonly runId:string; readonly attempt:string;
}
const REPOSITORY='https://github.com/iokaio/airk';
const WORKFLOW='.github/workflows/github-release.yml';
const verifier=new Verifier(toTrustMaterial(TrustedRoot.fromJSON(JSON.parse(PUBLIC_GOOD_ROOT_JSON))),
    {ctlogThreshold:1,tlogThreshold:1,timestampThreshold:1});
function object(value:unknown):Record<string,unknown>{
    if(!value||typeof value!=='object'||Array.isArray(value)){throw new Error('Malformed build provenance.');}
    return value as Record<string,unknown>;
}
export function verifyBundleCryptography(value:unknown):{statement:Record<string,unknown>;signer:Signer}{
    const raw=object(value),material=object(raw.verificationMaterial),envelope=object(raw.dsseEnvelope);
    if(raw.mediaType!=='application/vnd.dev.sigstore.bundle.v0.3+json'||envelope.payloadType!=='application/vnd.in-toto+json'||
        typeof envelope.payload!=='string'||envelope.payload.length>1024*1024||!Array.isArray(envelope.signatures)||envelope.signatures.length!==1||
        !Array.isArray(material.tlogEntries)||material.tlogEntries.length<1||material.tlogEntries.length>4){throw new Error('Unsupported or unbounded provenance bundle.');}
    const bundle=bundleFromJSON(value);
    const signer=verifier.verify(toSignedEntity(bundle));
    const statement=object(JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(Buffer.from(envelope.payload,'base64'))));
    return {statement,signer};
}
function oidValue(text:string):Buffer{
    const value=Buffer.from(text,'utf8');
    if(value.length>=128){throw new Error('Provenance policy constant exceeds DER bound.');}
    return Buffer.concat([Buffer.from([12,value.length]),value]);
}
/** Checks only already cryptographically verified identity and statement fields. */
export function validateAirkProvenance(statement:Record<string,unknown>,signer:Signer,expected:ProvenanceExpectation):void{
    if(!/^[a-f0-9]{64}$/.test(expected.sha256)||!/^[a-f0-9]{40}$/.test(expected.commit)||
        !/^[1-9][0-9]*$/.test(expected.runId)||!/^[1-9][0-9]*$/.test(expected.attempt)) {throw new Error('Invalid release provenance expectation.');}
    if(statement._type!=='https://in-toto.io/Statement/v1'||statement.predicateType!=='https://slsa.dev/provenance/v1'||!Array.isArray(statement.subject)||statement.subject.length<1||statement.subject.length>128){throw new Error('Wrong build attestation predicate.');}
    const matches=statement.subject.map(object).filter(subject=>subject.name===expected.name);
    if(matches.length!==1||object(matches[0].digest).sha256!==expected.sha256){throw new Error('Build attestation subject differs from installer.');}
    const predicate=object(statement.predicate),definition=object(predicate.buildDefinition),external=object(definition.externalParameters),workflow=object(external.workflow);
    const internal=object(object(definition.internalParameters).github),run=object(predicate.runDetails),metadata=object(run.metadata);
    const ref=workflow.ref;
    if(definition.buildType!=='https://actions.github.io/buildtypes/workflow/v1'||workflow.repository!==REPOSITORY||workflow.path!==WORKFLOW||
        (ref!=='refs/heads/main'&&ref!=='refs/heads/feat/advanced-split')||internal.runner_environment!=='github-hosted'||
        internal.repository_id!=='1418150478'||internal.repository_owner_id!=='152147161'||internal.event_name!=='workflow_dispatch'){throw new Error('Build repository, workflow or runner is not approved.');}
    const invocation=`${REPOSITORY}/actions/runs/${expected.runId}/attempts/${expected.attempt}`;
    if(metadata.invocationId!==invocation){throw new Error('Build attestation release run differs.');}
    const dependencies=definition.resolvedDependencies;
    if(!Array.isArray(dependencies)||dependencies.length>128||dependencies.map(object).filter(item=>item.uri===`git+${REPOSITORY}@${ref}`&&object(item.digest).gitCommit===expected.commit).length!==1){throw new Error('Build source revision differs.');}
    const san=`${REPOSITORY}/${WORKFLOW}@${ref}`;
    if(object(run.builder).id!==san){throw new Error('Build attestation builder identity differs.');}
    if(signer.identity?.extensions?.issuer!=='https://token.actions.githubusercontent.com'||signer.identity.subjectAlternativeName!==san){throw new Error('Build certificate issuer or workflow identity differs.');}
    // Fulcio certificate OIDs bind the signed runner, immutable repository IDs,
    // source revision and run identity, independently of statement claims.
    for(const [last,text] of [[11,'github-hosted'],[12,REPOSITORY],[13,expected.commit],[15,'1418150478'],[17,'152147161'],[20,'workflow_dispatch'],[21,invocation]] as const){
        const found=signer.identity.oids?.find(item=>item.oid?.id.join('.')===`1.3.6.1.4.1.57264.1.${last}`);
        if(!found||!Buffer.from(found.value).equals(oidValue(text))){throw new Error('Build certificate source/runner/run binding differs.');}
    }
}
export function verifyAirkAttestation(value:unknown,expected:ProvenanceExpectation):void{
    const {statement,signer}=verifyBundleCryptography(value);
    validateAirkProvenance(statement,signer,expected);
}

export function decodeAttestationBundle(compressed:Buffer):unknown{
    if(compressed.length>2*1024*1024){throw new Error('Compressed attestation exceeds bound.');}
    const raw=uncompress(compressed,2*1024*1024);
    if(raw.length>2*1024*1024){throw new Error('Attestation exceeds decoded bound.');}
    return JSON.parse(new TextDecoder('utf8',{fatal:true}).decode(raw));
}
