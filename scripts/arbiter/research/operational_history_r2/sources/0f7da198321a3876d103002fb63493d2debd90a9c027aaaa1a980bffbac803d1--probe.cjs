// SPDX-License-Identifier: Apache-2.0
// Read-only replay of the complete retained actual native Answer packet.
'use strict';const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=process.cwd(),source='C:/Users/tdotn/AppData/Local/Temp/airk-service-clients-40vhidk9/workbench-answer-export.json';
const raw=fs.readFileSync(source),turn=JSON.parse(raw).turns[0],packet=turn.envelope;
const {answerEvidenceRows}=require(path.join(root,'packages/vscode/out/workbenchAnswerNavigation'));
const {evidenceCards}=require(path.join(root,'packages/vscode/out/workbenchRender'));
assert.equal(packet.schema,'airk.arbiter.context/2');assert.equal(packet.source_verification.status,'verified');
assert.equal(packet.source_verification.references.length,1);
assert.equal(packet.source_verification.references[0].result_pointer,'/records/0/canonical_result/hits/0/citation');
assert.deepEqual(packet.records[0].scope,packet.assessment.scope);
assert.equal(packet.records[0].canonical_result.source_verification,undefined);
const before=JSON.stringify(packet),rows=answerEvidenceRows(packet),cards=evidenceCards(packet),expected=Number(process.argv[2]);
assert.equal(rows.length,expected);assert.equal(cards.length,expected);
if(expected){const card=cards[0],record=packet.records[0],hit=record.canonical_result.hits[0],origin=packet.source_verification.origins[0],sourceRow=packet.source_verification.sources[0];
 assert.equal(card.step,record.id);assert.equal(card.resultPointer,'/hits/0');assert.equal(card.path,hit.citation.path);assert.equal(card.hash,hit.citation.content_hash);
 assert.equal(card.indexId,record.scope.index_id);assert.deepEqual(card.projectIds,origin.project_ids);assert.equal(card.publication,origin.publication);assert.equal(card.sourceRootId,sourceRow.source_root_id);
 for(const tamper of [p=>p.source_verification.references[0].result_pointer='/records/1/canonical_result/hits/0/citation',p=>p.source_verification.references[0].source_id='foreign',p=>p.records[0].scope.authorization='00'.repeat(32),p=>p.source_verification.origins[0].index_id='idx_'+'00'.repeat(16)]){const crossed=structuredClone(packet);tamper(crossed);assert.equal(answerEvidenceRows(crossed).length,0);}
}
assert.equal(JSON.stringify(packet),before);assert.deepEqual(fs.readFileSync(source),raw);
const receipt={actual_retained_native_packet:true,expected,rows:rows.length,cards:cards.length,original_sha256:crypto.createHash('sha256').update(raw).digest('hex'),source_unchanged:true,navigation_sha256:crypto.createHash('sha256').update(fs.readFileSync(path.join(root,'packages/vscode/out/workbenchAnswerNavigation.js'))).digest('hex')};
fs.writeFileSync(path.join(__dirname,'receipt-'+expected+'.json'),JSON.stringify(receipt,null,2)+'\n');console.log(JSON.stringify(receipt));
