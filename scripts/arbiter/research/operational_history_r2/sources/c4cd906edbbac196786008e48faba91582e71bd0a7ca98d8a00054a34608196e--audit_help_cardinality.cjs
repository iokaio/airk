// SPDX-License-Identifier: Apache-2.0
// Pure authored-test AST and retained-result audit; does not execute the tests.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),ts=require(path.join(root,'packages/vscode/node_modules/typescript'));
const dir=path.join(root,'artifacts/advanced-split/receipts/final-help-resume-r2');
const old=path.join(root,'artifacts/advanced-split/receipts/final-help-r1');
const hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const expected=new Map([['packages/ui/test/helpManifest.test.mjs',2],['packages/ui/test/helpSearch.test.mjs',1],
 ['packages/ui/test/helpRoute.test.mjs',3],['packages/ui/test/helpPresentation.test.mjs',4],
 ['packages/ui/test/help.dom.test.mjs',3],['packages/ui/test/generatedHelp.dom.test.mjs',2],
 ['packages/admin/test/helpSemantics.test.mjs',5]]);
const result=JSON.parse(fs.readFileSync(path.join(dir,'shared-help-tests.json'))),prior=JSON.parse(fs.readFileSync(path.join(old,'shared-help-tests.json')));
const plan=JSON.parse(fs.readFileSync(path.join(dir,'plan.json'))),rows=[],names=[];
assert.deepEqual(result.command,prior.command);assert.deepEqual(result.command.slice(2),[...expected.keys()]);
for(const [file,count]of expected){
 const full=path.join(root,file),tree=ts.createSourceFile(file,fs.readFileSync(full,'utf8'),ts.ScriptTarget.Latest,true,ts.ScriptKind.JS),declared=[];
 assert.equal(tree.parseDiagnostics.length,0);assert.equal(hash(full),plan.source_files[file]);
 function visit(node){
  if(ts.isCallExpression(node)&&ts.isPropertyAccessExpression(node.expression)&&ts.isIdentifier(node.expression.expression)&&node.expression.expression.text==='test')throw Error('Unexpected test modifier '+file);
  if(ts.isCallExpression(node)&&ts.isIdentifier(node.expression)&&node.expression.text==='test'){
   const title=node.arguments[0];
   if(ts.isStringLiteral(title)||ts.isNoSubstitutionTemplateLiteral(title))declared.push(title.text);
   else{
    assert.ok(ts.isTemplateExpression(title));assert.equal(title.templateSpans.length,1);
    const span=title.templateSpans[0];assert.ok(ts.isIdentifier(span.expression));assert.equal(span.expression.text,'host');
    let loop=node.parent;while(loop&&!ts.isForOfStatement(loop))loop=loop.parent;
    assert.ok(loop&&ts.isArrayLiteralExpression(loop.expression));
    assert.deepEqual(loop.expression.elements.map(x=>x.text),['browser','editor']);
    for(const host of ['browser','editor'])declared.push(title.head.text+host+span.literal.text);
   }
  }ts.forEachChild(node,visit);
 }visit(tree);assert.equal(declared.length,count,file);rows.push({file,sha256:hash(full),declared_names:declared,count});names.push(...declared);
}
assert.equal(names.length,20);assert.equal(new Set(names).size,20);
const log=fs.readFileSync(path.join(dir,'shared-help-tests.log'),'utf8');
const passed=[...log.matchAll(/^✔ (.+) \([0-9.]+ms\)$/gm)].map(x=>x[1]);
assert.deepEqual([...passed].sort(),[...names].sort());
for(const [label,n]of [['tests',20],['pass',20],['fail',0],['cancelled',0],['skipped',0]])assert.ok(log.includes(`ℹ ${label} ${n}`));
assert.equal(result.exit_code,0);assert.equal(result.selected_tests,20);assert.equal(result.skipped_tests,0);
const oldlog=fs.readFileSync(path.join(old,'shared-help-tests.log'),'utf8');
assert.ok(oldlog.includes('ℹ tests 21'));assert.ok(oldlog.includes('✖ D:\\code\\Github\\airk\\packages\\ui\\test\\help.dom.test.mjs ('));
assert.ok(oldlog.includes('test at packages\\ui\\test\\help.dom.test.mjs:16:1'));
const report={schema:'airk.private-help-authored-cardinality/1',files:rows,authored_count:20,all_authored_names_passed_in_r2:true,
 exact_r1_r2_command_same:true,filters_or_subset:false,r2_skipped:0,r2_failures:0,r1_reported_count:21,
 diagnosis:'R1 adds the implicit help.dom.test.mjs file/after-hook failure event to the 20 authored tests; line16 is the after hook. R2 has all20 authored names passed without that failure event.',
 evidence:{r1_log_sha256:hash(path.join(old,'shared-help-tests.log')),r2_log_sha256:hash(path.join(dir,'shared-help-tests.log')),r2_receipt_sha256:hash(path.join(dir,'shared-help-tests.json'))},tests_reexecuted:false};
fs.writeFileSync(path.join(__dirname,'help-cardinality-audit.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({authored_tests:20,actual_r2_passed_names:passed.length,r1_extra:'implicit file/after-hook failure',tests_reexecuted:false}));
