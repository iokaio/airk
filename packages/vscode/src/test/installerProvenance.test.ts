// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {createHash} from 'node:crypto';
import type {Signer} from '@sigstore/verify';
import {verifyBundleCryptography,verifyAirkAttestation,validateAirkProvenance,decodeAttestationBundle} from '../airkctl/bootstrap/provenance';
import {approvedAttestationBundleUrl,approvedRedirect} from '../airkctl/bootstrap/release';

const fixturePath=path.resolve(__dirname,'../../../../fixtures/split/bootstrap/github-cli-build-bundle.json');
const fixture=JSON.parse(fs.readFileSync(fixturePath,'utf8'));
const fixtureProvenance=JSON.parse(fs.readFileSync(path.join(path.dirname(fixturePath),'provenance.json'),'utf8'));
const clone=<T>(value:T):T=>structuredClone(value);

test('real public GitHub Actions bundle verifies chain, SCT, Rekor and authenticated timestamp',()=>{
    assert.equal(createHash('sha256').update(fs.readFileSync(fixturePath)).digest('hex'),fixtureProvenance.bundle_sha256);
    const verified=verifyBundleCryptography(fixture);
    assert.equal(verified.statement.predicateType,'https://slsa.dev/provenance/v1');
    assert.equal(verified.signer.identity?.subjectAlternativeName,'https://github.com/cli/cli/.github/workflows/deployment.yml@refs/heads/trunk');
    assert.throws(()=>verifyAirkAttestation(fixture,{name:'gh_2.104.0_linux_amd64.tar.gz',sha256:'98cb101ad87dbbd3f6b2f2bfcbd43197ccd07643140121d8a9cfaa91eadbc24e',commit:'3d5a9e355a837f2f5ee78d7484c118cc1342ad1a',runId:'43283669119',attempt:'1'}),/not approved|identity differs/);
});
test('changed DSSE payload, signature, certificate and signed log timestamp refuse',()=>{
    const mutate=(bytes:string)=>Buffer.from('tampered '+bytes).toString('base64');
    for(const change of [
        (value:any)=>{value.dsseEnvelope.payload=mutate(value.dsseEnvelope.payload);},
        (value:any)=>{value.dsseEnvelope.signatures[0].sig=mutate(value.dsseEnvelope.signatures[0].sig);},
        (value:any)=>{value.verificationMaterial.certificate.rawBytes=mutate(value.verificationMaterial.certificate.rawBytes);},
        (value:any)=>{value.verificationMaterial.tlogEntries[0].integratedTime='1';},
    ]){const value=clone(fixture);change(value);assert.throws(()=>verifyBundleCryptography(value));}
});
function policyFixture(){
    const expected={name:'airk-1.8.35-x86_64-unknown-linux-gnu-installer.tar.gz',sha256:'ab'.repeat(32),commit:'cd'.repeat(20),runId:'123456',attempt:'1'};
    const invocation='https://github.com/iokaio/airk/actions/runs/123456/attempts/1';
    const statement={_type:'https://in-toto.io/Statement/v1',predicateType:'https://slsa.dev/provenance/v1',subject:[{name:expected.name,digest:{sha256:expected.sha256}}],predicate:{
        buildDefinition:{buildType:'https://actions.github.io/buildtypes/workflow/v1',externalParameters:{workflow:{ref:'refs/heads/feat/advanced-split',repository:'https://github.com/iokaio/airk',path:'.github/workflows/github-release.yml'}},internalParameters:{github:{runner_environment:'github-hosted',event_name:'workflow_dispatch',repository_id:'1418150478',repository_owner_id:'152147161'}},resolvedDependencies:[{uri:'git+https://github.com/iokaio/airk@refs/heads/feat/advanced-split',digest:{gitCommit:expected.commit}}]},runDetails:{builder:{id:'https://github.com/iokaio/airk/.github/workflows/github-release.yml@refs/heads/feat/advanced-split'},metadata:{invocationId:invocation}}}};
    // Synthetic already-verified identity tests semantic policy only; it cannot pass crypto.
    const values:[[number,string],...[number,string][]]=[[11,'github-hosted'],[12,'https://github.com/iokaio/airk'],[13,expected.commit],[15,'1418150478'],[17,'152147161'],[20,'workflow_dispatch'],[21,invocation]];
    const signer={identity:{subjectAlternativeName:'https://github.com/iokaio/airk/.github/workflows/github-release.yml@refs/heads/feat/advanced-split',extensions:{issuer:'https://token.actions.githubusercontent.com'},oids:values.map(([id,text])=>({oid:{id:[1,3,6,1,4,1,57264,1,id]},value:Buffer.concat([Buffer.from([12,Buffer.byteLength(text)]),Buffer.from(text)])}))}} as Signer;
    return{expected,statement,signer};
}
test('AIRK release policy binds exact subject, immutable repository, runner, source and run',()=>{
    const valid=policyFixture();validateAirkProvenance(valid.statement,valid.signer,valid.expected);
    for(const change of [
        (value:any)=>{value.statement.subject[0].digest.sha256='00'.repeat(32);},
        (value:any)=>{value.statement.subject.push(value.statement.subject[0]);},
        (value:any)=>{value.statement.predicate.buildDefinition.internalParameters.github.runner_environment='self-hosted';},
        (value:any)=>{value.statement.predicate.buildDefinition.externalParameters.workflow.path='.github/workflows/ci.yml';},
        (value:any)=>{value.statement.predicate.buildDefinition.internalParameters.github.repository_id='999';},
        (value:any)=>{value.statement.predicate.buildDefinition.internalParameters.github.event_name='pull_request_target';},
        (value:any)=>{value.statement.predicate.runDetails.builder.id='https://evil.invalid/builder';},
        (value:any)=>{value.statement.predicate.buildDefinition.resolvedDependencies[0].digest.gitCommit='00'.repeat(20);},
        (value:any)=>{value.statement.predicate.runDetails.metadata.invocationId+='2';},
        (value:any)=>{value.signer.identity.extensions.issuer='https://evil.invalid';},
        (value:any)=>{value.signer.identity.oids=[];},
    ]){const value=policyFixture();change(value);assert.throws(()=>validateAirkProvenance(value.statement,value.signer,value.expected));}
});
test('attestation storage and decompression are independently bounded',()=>{
    const valid='https://tmaproduction.blob.core.windows.net/attestations/1418150478/2026/11/13/123.json.sn?sig=opaque';
    assert.equal(approvedAttestationBundleUrl(valid),true);assert.equal(approvedRedirect(valid,valid),true);
    for(const wrong of [valid.replace('1418150478','999'),valid.replace('https:','http:'),valid.replace('tmaproduction','tmastaging'),valid.replace('/attestations/','/other/')]){assert.equal(approvedAttestationBundleUrl(wrong),false);}
    const {compress}=require('snappyjs') as {compress(input:Buffer):Buffer};
    assert.deepEqual(decodeAttestationBundle(compress(Buffer.from('{"fixture":true}'))),{fixture:true});
    assert.throws(()=>decodeAttestationBundle(compress(Buffer.alloc(2*1024*1024+1))));
    assert.throws(()=>decodeAttestationBundle(Buffer.alloc(2*1024*1024+1)));
});
