// SPDX-License-Identifier: Apache-2.0
// Execute actual bundled Workbench components; hooks retain state across renders.
const {test}=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const requireExtension=createRequire(path.resolve(__dirname,'../../packages/vscode/package.json'));
const {buildSync}=requireExtension('esbuild');
const {fieldExpressions,catalogExpression,selectionExpressions}=require('../native-editor-workbench.cjs');
const {assertInstalledInputCatalog}=requireExtension('./out/test/installedCompletion.js');
const file=path.resolve(__dirname,'../../packages/vscode/src/workbenchWebview/main.tsx');
const source=fs.readFileSync(file,'utf8');
const anchor="render(<App/>,document.getElementById('root')!);";
assert.equal(source.split(anchor).length,2);
const bundled=buildSync({stdin:{contents:source.replace(anchor,'export {ToolForm,Turn,App};'),resolveDir:path.dirname(file),loader:'tsx'},bundle:true,write:false,platform:'node',format:'cjs',jsx:'automatic',jsxImportSource:'preact',external:['preact','preact/hooks','preact/jsx-runtime'],loader:{'.css':'empty'}}).outputFiles[0].text;
function screen(){
    const slots=[],sent=[];let cursor=0;
    const hooks={useState(initial){const at=cursor++;if(!(at in slots))slots[at]=typeof initial==='function'?initial():initial;return [slots[at],next=>{slots[at]=typeof next==='function'?next(slots[at]):next;}];},useRef:initial=>({current:initial}),useEffect(){},useLayoutEffect(){},useMemo:read=>read()};
    const module={exports:{}};
    vm.runInNewContext(bundled,{module,exports:module.exports,require:name=>name==='preact/hooks'?hooks:requireExtension(name),acquireVsCodeApi:()=>({postMessage:value=>sent.push(value)}),TextEncoder,TextDecoder,console});
    return {sent,slots,render(name,props){cursor=0;return module.exports[name](props);}};
}
function nodes(node){if(Array.isArray(node))return node.flatMap(nodes);return !node||typeof node!=='object'?[]:[node,...nodes(node.props?.children)];}
function text(node){return node==null||typeof node==='boolean'?'':Array.isArray(node)?node.map(text).join(''):typeof node==='object'?text(node.props?.children):String(node);}
function button(tree,label){const found=nodes(tree).filter(node=>node.type==='button'&&text(node)===label);assert.ok(found.length>0,label);return found[0];}
function fieldDom(tree){
    class Input{
        constructor(vnode){this.vnode=vnode;this.tagName=vnode.type.toUpperCase();this.current=String(vnode.props.value??'');this.type=vnode.props.type??'text';this.required=!!vnode.props.required;this.disabled=!!vnode.props.disabled;}
        get value(){return this.current;}set value(value){this.current=value;}
        getAttribute(name){const key={minlength:'minLength',maxlength:'maxLength'}[name]??name;const value=this.vnode.props[key];return value===undefined?null:String(value);}
        dispatchEvent(event){(event.type==='change'?this.vnode.props.onChange:this.vnode.props.onInput)?.({currentTarget:this,target:this});}
    }
    class Select extends Input{
        constructor(vnode){super(vnode);this.type='select-one';this.options=nodes(vnode.props.children).filter(node=>node.type==='option').map(node=>({value:String(node.props.value??'')}));}
        get value(){return this.current;}set value(value){this.current=value;}
    }
    const controls=nodes(tree).filter(node=>['input','select'].includes(node.type)).map(node=>node.type==='select'?new Select(node):new Input(node));
    const operation=controls.find(node=>node.getAttribute('aria-label')==='Operation'),fields=controls.filter(node=>node!==operation);
    const context=vm.createContext({document:{querySelector:selector=>selector.includes('Operation')?operation:undefined,querySelectorAll:()=>fields},HTMLInputElement:Input,HTMLSelectElement:Select,Event:class{constructor(type){this.type=type;}}});
    return {operation,fields,run:expression=>vm.runInContext(expression,context)};
}
test('audited form selectors/catalog and setters execute against actual generated ToolForm fields',()=>{
    const f=screen(),project='ab'.repeat(32);
    const operations={search:{type:'object',required:['query'],properties:{query:{type:'string',minLength:1,maxLength:123},mode:{type:'string',enum:['lexical','vector']},limit:{type:'integer',minimum:1,maximum:20}}},capabilities:{type:'object',properties:{}}};
    const draft={activeTool:'search',revision:0,scope:'',recipe:'',values:Object.create(null)};
    const state={session:{id:'session',project,formDraft:draft},sessionEpoch:1,project,busy:false,operations};
    const props={state,issue:'',currentDraft:()=>draft,onSelect:tool=>{draft.activeTool=tool;},onField:(tool,name,value)=>{draft.values[tool]=Object.assign(Object.create(null),draft.values[tool],{[name]:value});},onReset:tool=>{delete draft.values[tool];},onIssue:issue=>assert.equal(issue,'')};
    let tree=f.render('ToolForm',props),dom=fieldDom(tree);
    const catalog=JSON.parse(JSON.stringify(dom.run(catalogExpression)));
    assert.deepEqual(catalog.names,['search','capabilities']);
    assert.deepEqual(catalog.fields.map(field=>field.label),['query','mode','limit']);
    assert.equal(catalog.fields[0].minLength,1);assert.equal(catalog.fields[0].maxLength,123);assert.equal(catalog.fields[0].required,true);
    assert.deepEqual(catalog.fields[1].options,['','lexical','vector']);assert.equal(catalog.fields[2].min,'1');assert.equal(catalog.fields[2].max,'20');
    for(const [label,value]of [['query','LegacyOriginalEvidence'],['mode','lexical'],['limit','5']]){
        dom=fieldDom(f.render('ToolForm',props));const expressions=fieldExpressions(label,value);
        assert.equal(dom.run(expressions.ready),true);assert.equal(dom.run(expressions.apply),true);assert.equal(dom.run(expressions.readback),true);
    }
    tree=f.render('ToolForm',props);
    button(tree,'Run form').props.onClick({currentTarget:{form:{reportValidity:()=>true}}});
    assert.equal(f.sent.length,1);assert.equal(f.sent[0].type,'run');assert.equal(f.sent[0].prepare,false);
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent[0].plan.steps)),[{id:'form',tool:'search',args:{query:'LegacyOriginalEvidence',mode:'lexical',limit:5}}]);
});
test('actual generated DOM catalog passes the compiled installed journey catalog assertion for every field kind',()=>{
    const f=screen(),project='ab'.repeat(32);
    const operations={search:{type:'object',required:['query','compact'],properties:{
        query:{type:'string',minLength:2,maxLength:120,default:'original'},
        compact:{type:'boolean',default:false},
        mode:{type:'string',enum:['lexical','vector']},
        limit:{type:'integer',minimum:1,maximum:40},
        names:{type:'array',items:{type:'string'}},
        metadata:{type:'object'},
        member_availability:{anyOf:[{type:'null'},{type:'string',enum:['require_all','allow_partial']}]}
    }},capabilities:{type:'object',properties:{response_mode:{type:'string',enum:['terse','full']}}}};
    const draft={activeTool:'search',revision:0,scope:'',recipe:'',values:Object.create(null)},state={session:{id:'session',project,formDraft:draft},sessionEpoch:1,project,busy:false,operations};
    const props={state,issue:'',currentDraft:()=>draft,onSelect:tool=>{draft.activeTool=tool;},onField(){},onReset(){},onIssue(){}};
    const forms={};
    for(const tool of Object.keys(operations)){
        draft.activeTool=tool;
        const catalog=JSON.parse(JSON.stringify(fieldDom(f.render('ToolForm',props)).run(catalogExpression)));
        forms[tool]=catalog.fields;
    }
    draft.activeTool='search';
    const displayed={...JSON.parse(JSON.stringify(fieldDom(f.render('ToolForm',props)).run(catalogExpression))),forms};
    assertInstalledInputCatalog(operations,displayed);
    const selects=displayed.fields.filter(field=>field.tag==='SELECT');
    assert.ok(selects.length>=3);assert.ok(selects.every(field=>field.minLength===0&&field.maxLength===4096));
    assert.equal(displayed.fields.find(field=>field.label==='Compact results').required,true);
});

test('actual Turn output buttons retain full envelope and separate subset-copy messages from full exports',()=>{
    const f=screen(),turn={id:'turn-original',at:1,input:'search form',format:'structured',envelope:{status:'complete',future:{full:true},evidence:[]}};
    const card={id:'record-0',path:'fixture.ts',start:1,end:1,text:'original',hash:'a'.repeat(64)};
    const state={busy:false,cards:{[turn.id]:[card]}};
    let tree=f.render('Turn',{turn,state});
    const complete=nodes(tree).find(node=>node.type==='details'&&node.props.class==='complete-response');
    assert.deepEqual(JSON.parse(text(nodes(complete).find(node=>node.type==='pre'))),turn.envelope);
    const checkbox=nodes(tree).find(node=>node.type==='input'&&node.props['aria-label']==='Select fixture.ts:1');assert.ok(checkbox);
    checkbox.props.onChange({currentTarget:{checked:true}});
    tree=f.render('Turn',{turn,state});button(tree,'json').props.onClick();tree=f.render('Turn',{turn,state});
    button(tree,'Copy selected (1)').props.onClick();
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent.at(-1))),{type:'copy',turn:turn.id,format:'json',cards:['record-0']});
    button(tree,'Save export\u2026').props.onClick();assert.deepEqual(JSON.parse(JSON.stringify(f.sent.at(-1))),{type:'export',turn:turn.id,format:'json'});
    button(tree,'markdown').props.onClick();tree=f.render('Turn',{turn,state});button(tree,'Copy selected (1)').props.onClick();
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent.at(-1))),{type:'copy',turn:turn.id,format:'markdown',cards:['record-0']});
});
test('actual session Settings controls send whole-session copy/export messages without a subset',()=>{
    const f=screen(),project='ab'.repeat(32);
    f.slots[0]={project,projects:[],recent:[],cards:{},busy:false,session:{id:'original',project,title:'session',turns:[],formDraft:{activeTool:'search',revision:0,scope:'',recipe:'',values:Object.create(null)}}};
    f.slots[2]='settings';let tree=f.render('App');
    for(const [label,format]of [['Copy session as JSON','json'],['Copy session as Markdown','markdown']]){
        f.slots[2]='settings';tree=f.render('App');button(tree,label).props.onClick();
        assert.deepEqual(JSON.parse(JSON.stringify(f.sent.at(-1))),{type:'copy',format});
    }
    f.slots[2]='settings';tree=f.render('App');button(tree,'Export session\u2026').props.onClick();
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent.at(-1))),{type:'export',format:'json'});
});


test('actual selected-card click handlers must render selected copy state before the helper accepts it',()=>{
    const f=screen(),turn={id:'selected-turn',at:1,input:'search form',format:'structured',envelope:{status:'complete',evidence:[]}};
    const cards=[{id:'record-0',path:'fixture.ts',start:1,end:1,text:'first',hash:'a'.repeat(64)},{id:'record-1',path:'other.ts',start:1,end:1,text:'second',hash:'b'.repeat(64)}];
    const state={busy:false,cards:{[turn.id]:cards}};let rendered=f.render('Turn',{turn,state}),clicks=0;
    function run(expression,{suppressHandlers=false}={}){
        const boxes=nodes(rendered).filter(node=>node.type==='input'&&node.props['aria-label']?.startsWith('Select ')).map(vnode=>({
            checked:!!vnode.props.checked,disabled:!!vnode.props.disabled,
            click(){clicks++;this.checked=!this.checked;if(!suppressHandlers)vnode.props.onChange({currentTarget:this});}
        }));
        const buttons=nodes(rendered).filter(node=>node.type==='button').map(vnode=>({textContent:text(vnode),disabled:!!vnode.props.disabled,getAttribute:name=>vnode.props[name]??null}));
        const root={querySelectorAll:selector=>selector==='button'?buttons:boxes};
        return vm.runInNewContext(expression,{document:{querySelectorAll:()=>[root]}});
    }
    const selection=selectionExpressions();
    assert.equal(run(selection.ready),true);assert.equal(run(selection.apply),true);assert.equal(clicks,1);
    rendered=f.render('Turn',{turn,state});assert.equal(run(selection.readback),true);
    button(rendered,'json').props.onClick();rendered=f.render('Turn',{turn,state});button(rendered,'Copy selected (1)').props.onClick();
    assert.deepEqual(JSON.parse(JSON.stringify(f.sent.at(-1))),{type:'copy',turn:turn.id,format:'json',cards:['record-0']});
    const stale=screen();rendered=stale.render('Turn',{turn,state});
    assert.equal(run(selection.apply,{suppressHandlers:true}),true);
    rendered=stale.render('Turn',{turn,state});assert.equal(run(selection.readback),false,'Native checked mutation without component state must not qualify selection');
});
