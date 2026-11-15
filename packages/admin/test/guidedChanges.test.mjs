// SPDX-License-Identifier: Apache-2.0
// Actual component callbacks over synthetic native packets; not transport parity.
import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const bundle=await build({absWorkingDir:root,entryPoints:['src/EvidenceToolsScreen.tsx'],bundle:true,write:false,format:'esm',platform:'node',jsx:'automatic',jsxImportSource:'preact',plugins:[{name:'hooks',setup(plugin){
 plugin.onResolve({filter:/^preact\/hooks$/},()=>({path:'hooks',namespace:'test'}));
 plugin.onLoad({filter:/^hooks$/,namespace:'test'},()=>({contents:`const host=()=>globalThis.__guidedHooks;export function useState(initial){const h=host(),i=h.cursor++;if(!(i in h.slots))h.slots[i]=initial;return [h.slots[i],next=>h.slots[i]=typeof next==='function'?next(h.slots[i]):next];}export function useRef(initial){const h=host(),i=h.cursor++;return h.slots[i]??={current:initial};}export function useEffect(){host().cursor++;}`}));
}}]});
const {EvidenceToolsScreen}=await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].contents).toString('base64')}`);
const nodes=v=>!v||typeof v!=='object'?[]:Array.isArray(v)?v.flatMap(nodes):[v,...nodes(v.props?.children)];
const text=v=>v==null?'':typeof v!=='object'?String(v):Array.isArray(v)?v.map(text).join(''):text(v.props?.children);
async function settled(){for(let i=0;i<30&&globalThis.__guidedHooks.slots[2];i++)await Promise.resolve();assert.equal(globalThis.__guidedHooks.slots[2],false,'callback completed');}
const scope='56'.repeat(32),index='idx_'+'23'.repeat(16),token=g=>`airk-r1:${'ab'.repeat(16)}:${g}:${'cd'.repeat(32)}`;
const packet=(cursor,generation=1)=>({schema:'airk.combined-read/1',cursor,publication_vector:{authority:'67'.repeat(32),members:[{index_id:index,publication:{collection:'fixture',token:token(generation)}}]},members:[{index_id:index,result:{structuredContent:{status:'ready',changes:[{future_change:{kept:true}}],baseline_status:'verified',coverage:{all:'kept'}}}}]});
function fixture({changed=false,initial}={}){
 globalThis.__guidedHooks={cursor:0,slots:[]};let active=true,currentScope=scope;const calls=[];
 const capture={indexIds:[index],checkCurrent(){if(currentScope!==scope)throw Error('scope_changed');},async read(tool,args){calls.push({tool,args:structuredClone(args)});return {scopeIdentity:scope,request:{tool,args},logical:calls.length===1&&initial?initial:packet(calls.length===1?'original-opaque-cursor':null,changed&&calls.length>1?2:1),raw:{untouched: calls.length,future_outer:['retained']}};}};
 const render=()=>{globalThis.__guidedHooks.cursor=0;return EvidenceToolsScreen({route:'evidence/changes',scopeIdentity:currentScope,disabled:false,capture:()=>capture,active});};
 const select=(tree,predicate)=>nodes(tree).find(predicate);
 const button=(label)=>select(render(),node=>node.type==='button'&&text(node)===label);
 const start=async()=>{let tree=render();select(tree,node=>node.props?.label==='Earlier native publication token').props.onInput(' exact baseline ');select(tree,node=>node.props?.label==='Path prefix (optional)').props.onInput('src/');select(tree,node=>node.props?.label==='Result limit').props.onInput(7);select(tree,node=>node.type==='textarea').props.onInput({currentTarget:{value:'{"max_bytes":1234,"future_filter":{"keep":true}}'}});await select(render(),node=>node.type==='form').props.onSubmit({preventDefault(){}});};
 return {calls,render,button,start,hide(){active=false;render();},rescope(){currentScope='99'.repeat(32);render();},report(){return globalThis.__guidedHooks.slots[4];}};
}
test('Changes continuation retains exact baseline/options after visible edits and all original packets',async()=>{
 const f=fixture();await f.start();assert.ok(f.button('Load more indexed changes'));const first=f.report();
 const input=nodes(f.render()).find(node=>node.props?.label==='Earlier native publication token');input.props.onInput('edited baseline');
 await f.button('Load more indexed changes').props.onClick();await settled();
 assert.deepEqual(f.calls,[{tool:'changes',args:{max_bytes:1234,future_filter:{keep:true},limit:7,path_prefix:'src/',since:' exact baseline '}},{tool:'airk_changes',args:{max_bytes:1234,future_filter:{keep:true},limit:7,path_prefix:'src/',since:' exact baseline ',cursor:'original-opaque-cursor'}}]);
 assert.equal(f.report().inputs.fields.since,' exact baseline ');assert.deepEqual(f.report().reads[0],first.reads[0]);assert.deepEqual(f.report().reads[1].raw,{untouched:2,future_outer:['retained']});assert.equal(f.button('Load more indexed changes'),undefined);
});
test('Changed publication refuses Changes continuation and retains the failed page for export',async()=>{
 const f=fixture({changed:true});await f.start();await f.button('Load more indexed changes').props.onClick();await settled();assert.equal(f.report().historical,true);assert.match(f.report().error,/publication or query scope changed/);assert.equal(f.report().reads.length,2);assert.equal(f.button('Load more indexed changes').props.disabled,true);
});
test('Hidden Changes views send no continuation read',async()=>{const f=fixture();await f.start();const pending=f.button('Load more indexed changes');f.hide();await pending.props.onClick();assert.equal(f.calls.length,1);});
test('Replaced Changes scopes disable continuation and send no read',async()=>{const f=fixture();await f.start();f.rescope();assert.equal(f.button('Load more indexed changes').props.disabled,true);await f.button('Load more indexed changes').props.onClick();assert.equal(f.calls.length,1);});
test('Changes refuses ambiguous or malformed cursors and incomplete combined publication vectors',async()=>{
 for(const [initial,message]of [[{...packet('one'),page:{cursor:'two'}},/ambiguous continuation/],[packet(42),/continuation exceeded/],[{...packet('one'),publication_vector:{}},/no complete member publication vector/]]){
  const f=fixture({initial});await f.start();assert.equal(f.button('Load more indexed changes'),undefined);assert.match(f.report().error,message);assert.equal(f.report().reads.length,1);assert.equal(f.calls.length,1);
 }
});
