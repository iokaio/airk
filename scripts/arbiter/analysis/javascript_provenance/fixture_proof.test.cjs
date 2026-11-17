// SPDX-License-Identifier: Apache-2.0
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const {extract}=require('./fixture_proof.cjs');
const ts=require(path.resolve(__dirname,'../../../../packages/vscode/node_modules/typescript/lib/typescript.js'));
test('static template line endings use JavaScript literal semantics',()=>{
 const rows=extract(ts,'const service=`first\r\nsecond`;'.replace('\\r\\n','\r\n'),'fixture.cjs');
 assert.equal(rows.length,1);assert.equal(rows[0].value,'first\nsecond');
});
test('template expressions are never interpreted',()=>{
 assert.deepEqual(extract(ts,'const service=`before ${dangerous()} after`;','fixture.cjs'),[]);
});
test('static output properties decode literals without executing source',()=>{
 const rows=extract(ts,"throw new Error('never execute'); const files={'out/example.js':'exports.ready=true;'};",'fixture.ts');
 assert.equal(rows.length,1);assert.equal(rows[0].output,'out/example.js');assert.equal(rows[0].value,'exports.ready=true;');
});
