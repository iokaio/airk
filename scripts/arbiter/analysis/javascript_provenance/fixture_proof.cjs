// SPDX-License-Identifier: Apache-2.0
// Parse maintained fixture generators as data; never execute them or their outputs.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process');
const ROOT=path.resolve(__dirname,'../../../..');
const sha=raw=>crypto.createHash('sha256').update(raw).digest('hex');
function extract(ts,text,name){
 const source=ts.createSourceFile(name,text,ts.ScriptTarget.Latest,true,name.endsWith('.ts')?ts.ScriptKind.TS:ts.ScriptKind.JS),rows=[];
 if(source.parseDiagnostics.length)throw Error('Generator syntax invalid');
 function visit(node){
  if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text==='service'&&node.initializer&&ts.isNoSubstitutionTemplateLiteral(node.initializer))
   rows.push({kind:'service_static_template',value:node.initializer.text,start:node.initializer.pos,end:node.initializer.end});
  if(ts.isPropertyAssignment(node)&&ts.isStringLiteral(node.name)&&node.name.text.startsWith('out/')&&ts.isStringLiteral(node.initializer))
   rows.push({kind:'literal_output_property',output:node.name.text,value:node.initializer.text,start:node.initializer.pos,end:node.initializer.end});
  ts.forEachChild(node,visit);
 }
 visit(source);return rows;
}
function main(){
 const [input,expected]=process.argv.slice(2);if(!input||!expected)throw Error('Expected role report path and SHA256');
 const started=performance.now(),deadline=started+120000,snapshots=new Map();
 function check(){if(performance.now()>=deadline)throw Error('Original fixture proof deadline expired');}
 function read(file){check();const full=path.resolve(ROOT,file),raw=fs.readFileSync(full);snapshots.set(full,sha(raw));check();return raw;}
 function pin(file){const raw=read(file);return {path:path.relative(ROOT,path.resolve(ROOT,file)).split(path.sep).join('/'),sha256:sha(raw),bytes:raw.length};}
 function git(args){check();const raw=cp.execFileSync('git',args,{cwd:ROOT,timeout:Math.max(1,Math.floor(deadline-performance.now())),maxBuffer:32<<20});check();return raw;}
 const raw=read(input);if(sha(raw)!==expected)throw Error('Role report identity differs');
 const report=JSON.parse(raw);if(report.schema!=='airk.private.javascript-role-linkage/1')throw Error('Role report schema');
 const selected=report.groups.filter(g=>['unresolved_generation_role','exact_maintained_fixture_literal'].includes(g.role));
 const owner=pin(__filename),compiler=pin('packages/vscode/node_modules/typescript/lib/typescript.js');
 const nodeRaw=read(process.execPath),node={path:process.execPath,sha256:sha(nodeRaw),bytes:nodeRaw.length,version:process.version};
 const ts=require(path.join(ROOT,compiler.path)),refs=git(['show-ref']);
 if(git(['rev-parse','--show-object-format']).toString().trim()!=='sha1')throw Error('SHA1 Git object format required');
 const candidates=[];
 for(const name of ['scripts/check-extension-update-editor.cjs','packages/vscode/src/test/extensionUpdateLoader.test.ts']){
  const bytes=read(name);candidates.push({name,bytes,source:{path:name,sha256:sha(bytes),bytes:bytes.length}});
 }
 const old=git(['rev-list','--objects','--all','--','scripts/check-extension-update-editor.cjs']).toString().trim().split('\n');
 for(const line of old){
  const [oid,...parts]=line.split(' '),name=parts.join(' ');if(name!=='scripts/check-extension-update-editor.cjs')continue;
  const bytes=git(['cat-file','blob',oid]);
  if(crypto.createHash('sha1').update(Buffer.from('blob '+bytes.length+'\0')).update(bytes).digest('hex')!==oid)throw Error('Git blob differs');
  candidates.push({name,bytes,source:{git_blob_oid:oid,path_label:name,sha256:sha(bytes),bytes:bytes.length}});
 }
 const reproduced=new Map();
 for(const candidate of candidates){
  for(const literal of extract(ts,new TextDecoder('utf-8',{fatal:true}).decode(candidate.bytes),candidate.name)){
   const variants=literal.kind==='service_static_template'?['initial','updated','rebuilt','closed'].map(tag=>({tag,text:'const TAG='+JSON.stringify(tag)+';'+literal.value})):[{text:literal.value}];
   for(const variant of variants){
    const bytes=Buffer.from(variant.text),digest=sha(bytes);
    if(!reproduced.has(digest))reproduced.set(digest,{bytes:bytes.length,source:candidate.source,literal:{kind:literal.kind,output:literal.output,start_utf16:literal.start,end_utf16:literal.end},tag:variant.tag??null,method:'Static AST literal decoding; optional exact reviewed const TAG prefix; no generator execution'});
   }
  }
 }
 const rows=[],unresolved=[];
 for(const group of selected){
  for(const file of group.paths){const bytes=read(file);if(sha(bytes)!==group.sha256||bytes.length!==group.bytes)throw Error('Observed fixture bytes differ');}
  const proof=reproduced.get(group.sha256);
  if(proof&&proof.bytes===group.bytes)rows.push({...group,exact_fixture_reproduction:true,proof});else unresolved.push(group);
 }
 if(!git(['show-ref']).equals(refs))throw Error('Git refs changed');
 for(const [file,digest]of snapshots){check();if(sha(fs.readFileSync(file))!==digest)throw Error('Fixture proof input drift');}
 check();console.log(JSON.stringify({schema:'airk.private.javascript-fixture-proof/1',input:{path:input,sha256:expected},owner,compiler,node,git_refs:refs.toString().trim().split('\n'),qualification:false,generator_executed:false,selected_hashes:selected.length,proved_hashes:rows.length,proved_paths:rows.reduce((n,r)=>n+r.paths.length,0),unresolved,rows,elapsed_seconds:(performance.now()-started)/1000},null,2));
}
module.exports={extract};
if(require.main===module)main();
