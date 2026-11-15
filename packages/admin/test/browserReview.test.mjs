// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {createHash} from 'node:crypto';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const bundle=await build({absWorkingDir:root,stdin:{contents:"export {manualReviewChanges} from './src/browserReview';export {createReviewOperations,DEFAULT_TEST_PATTERN} from '../ui/src/review';export {FactHandles} from '../vscode/src/explorerFacts';export {RefRegistry} from '../vscode/src/explorerSearch';export {EvidenceFailure} from '../ui/src/evidenceCore';",resolveDir:root,sourcefile:'review-owner-test.ts'},bundle:true,write:false,format:'esm',platform:'browser',metafile:true});
assert.equal(Object.keys(bundle.metafile.inputs).some(path=>path.endsWith('/explorerReview.ts')||path.includes('mcpStdioSession')||path.includes('airkctl/runner')),false);
const api=await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const hash=text=>createHash('sha256').update(text).digest('hex');
function fixture(changes,call){const mutable={changes};return {mutable,operations:api.createReviewOperations({sourceKind:'manual',changedPaths:async()=>({git:'unavailable',changes:mutable.changes}),dirty:()=>false,testPattern:()=>new RegExp(api.DEFAULT_TEST_PATTERN),sha256:hash}),context:{project:'ab'.repeat(32),root:'explicit-browser-input',handles:new api.FactHandles(new api.RefRegistry()),session:{call}}};}
test('manual paths preserve literal Unicode and deduplicate without creating Git or staged facts',()=>{const value=api.manualReviewChanges('src/日本語.rs\ntests/session.test.ts\nsrc/日本語.rs','paths');assert.deepEqual(value.map(v=>v.path),['src/日本語.rs','tests/session.test.ts']);assert.ok(value.every(v=>!Object.hasOwn(v,'staged')&&v.change==='modified'));});
test('diff headers preserve rename additions deletions and never treat hunk text as a path header',()=>{
 const input='diff --git a/old.rs b/new.rs\n--- a/old.rs\n+++ b/new.rs\n@@ -1 +1 @@\n--- fake.rs\n+++ invented.rs\ndiff --git a/deleted.rs b/deleted.rs\n--- a/deleted.rs\n+++ /dev/null\ndiff --git a/new.rs b/added.rs\n--- /dev/null\n+++ b/added.rs';
 assert.deepEqual(api.manualReviewChanges(input,'patch').map(({path,change,originalPath})=>({path,change,...(originalPath?{originalPath}:{})})),[{path:'added.rs',change:'added'},{path:'deleted.rs',change:'deleted'},{path:'new.rs',change:'renamed',originalPath:'old.rs'}]);
 for(const value of ['../outside.rs','/absolute.rs','C:/outside.rs','src/../escape.rs',' src/space.rs'])assert.throws(()=>api.manualReviewChanges(value,'paths'),/relative indexed paths/);
 assert.throws(()=>api.manualReviewChanges('diff --git "a/file name.rs" "b/file name.rs"','patch'),/Quoted/);assert.throws(()=>api.manualReviewChanges('ordinary narrative','patch'),/No supported/);
});
test('same owning Review Changes pages all123 supplied paths and refuses changed manual snapshots',async()=>{
 let calls=0;const changes=api.manualReviewChanges(Array.from({length:123},(_,i)=>'src/file-'+String(i).padStart(3,'0')+'.rs').join('\n'),'paths');
 const f=fixture(changes,async()=>{calls++;return {publication:'airk-r1:original',payload:{read:{status:'unknown',reason:'path is absent from the current index'}},rawPayload:{future_native:{all:true}}};});
 const first=await f.operations.reviewChanges(f.context),second=await f.operations.reviewChanges(f.context,{offset:50,snapshot:first.snapshot,publication:first.publication}),third=await f.operations.reviewChanges(f.context,{offset:100,snapshot:first.snapshot,publication:first.publication});
 assert.deepEqual([first.files.length,second.files.length,third.files.length],[50,50,23]);assert.equal(new Set([...first.files,...second.files,...third.files].map(row=>row.path)).size,123);assert.equal(calls,123);assert.equal(third.truncated,false);assert.ok(first.notes.some(note=>note.includes('explicitly supplied')));assert.ok(first.files.every(row=>row.live==='unchecked'&&row.nativeResult.future_native.all===true));
 f.mutable.changes=[...changes,{path:'another.rs',file:'another.rs',change:'modified',staged:false}];await assert.rejects(f.operations.reviewChanges(f.context,{offset:50,snapshot:first.snapshot,publication:first.publication}),/supplied change list changed/);assert.equal(calls,123);
});
test('test candidates retain manual provenance and scope refusal never becomes a successful fallback',async()=>{
 const changes=api.manualReviewChanges('src/a.rs\ntests/a.test.ts','paths');const f=fixture(changes,async()=>({publication:'airk-r1:original',payload:{items:[],paths:[]}}));
 const result=await f.operations.reviewTests(f.context);assert.ok(result.candidates.some(row=>row.path==='tests/a.test.ts'&&row.reason==='changed_test_file'));assert.ok(result.notes.some(note=>note.includes('Manual path entries')));assert.ok(result.notes.every(note=>!note.includes('come from Git')));
 const refused=fixture(changes,async()=>{throw new api.EvidenceFailure({code:'STALE_BINDING',message:'actual captured scope was revoked'});});await assert.rejects(refused.operations.reviewTests(refused.context),/revoked/);
});

test('mixed text and binary Git diffs refuse the whole input rather than silently omitting a changed file',()=>{
 const patch='diff --git a/text.rs b/text.rs\n--- a/text.rs\n+++ b/text.rs\n@@ -1 +1 @@\n-old\n+new\ndiff --git a/image.png b/image.png\nBinary files a/image.png and b/image.png differ';
 assert.throws(()=>api.manualReviewChanges(patch,'patch'),/unsupported|binary|Paths/i);
});
test('multi-file standalone unified patches without Git section separators refuse rather than return only the first file',()=>{
 const patch='--- a/first.rs\n+++ b/first.rs\n@@ -1 +1 @@\n-old\n+new\n--- a/second.rs\n+++ b/second.rs\n@@ -1 +1 @@\n-old\n+new';
 assert.throws(()=>api.manualReviewChanges(patch,'patch'),/Git|section|Paths/i);
});

test('separate valid Git hunks retain every file while truncated or mismatched sections refuse as a whole',()=>{
 const patch='diff --git a/first.rs b/first.rs\n--- a/first.rs\n+++ b/first.rs\n@@ -0,0 +1 @@\n+first\n\\ No newline at end of file\ndiff --git a/second.rs b/second.rs\n--- a/second.rs\n+++ b/second.rs\n@@ -1 +1 @@\n-old\n+new';
 assert.deepEqual(api.manualReviewChanges(patch,'patch').map(row=>row.path),['first.rs','second.rs']);
 assert.throws(()=>api.manualReviewChanges(patch.slice(0,patch.lastIndexOf('\n+new')),'patch'),/incomplete/);
 assert.throws(()=>api.manualReviewChanges(patch.replace('+++ b/second.rs','+++ b/foreign.rs'),'patch'),/disagree/);
});

test('concatenated duplicate Git destinations refuse instead of dropping an earlier rename source',()=>{
 const patch='diff --git a/original.rs b/current.rs\nrename from original.rs\nrename to current.rs\ndiff --git a/current.rs b/current.rs\n--- a/current.rs\n+++ b/current.rs\n@@ -1 +1 @@\n-old\n+new';
 assert.throws(()=>api.manualReviewChanges(patch,'patch'),/Repeated|duplicate|Paths/);
});

test('browser manual changes never manufacture an observed staged or working-tree boolean',()=>{
 const changes=api.manualReviewChanges('src/unknown-state.rs','paths');assert.equal(Object.hasOwn(changes[0],'staged'),false);assert.equal(Object.hasOwn(changes[0],'workingTree'),false);
});
