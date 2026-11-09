// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm');
const {fieldExpressions,catalogExpression,observationExpression,workbenchAction}=require('../native-editor-workbench.cjs');
function fixture(){
    const writes=[],events=[];
    class Input{
        constructor(label,value=''){this.tagName='INPUT';this.label=label;this.current=value;this.disabled=false;this.required=true;this.type='text';}
        get value(){return this.current;}
        set value(value){this.current=value;writes.push(value);}
        getAttribute(name){return name==='aria-label'?this.label:null;}
        dispatchEvent(event){events.push(event.type);}
    }
    class Select extends Input{constructor(label,options){super(label);this.tagName='SELECT';this.type='select-one';this.options=options.map(value=>({value}));}get value(){return this.current;}set value(value){this.current=value;writes.push(value);}}
    const query=new Input('Query'),mode=new Select('Mode',['','lexical','vector']),operation=new Select('Operation',['search','status']);operation.current='search';
    const fields=[query,mode];
    const context=vm.createContext({document:{querySelectorAll:()=>fields,querySelector:()=>operation},HTMLInputElement:Input,HTMLSelectElement:Select,Event:class{constructor(type){this.type=type;}}});
    return {run:source=>vm.runInContext(source,context),query,mode,fields,writes,events};
}
test('actual field expressions set exact accessible inputs and enum values without a submit',()=>{
    const f=fixture();
    for(const [label,value]of [['Query','original symbol'],['Mode','lexical']]){
        const e=fieldExpressions(label,value);assert.equal(f.run(e.ready),true);assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);
    }
    assert.deepEqual(f.writes,['original symbol','lexical']);assert.deepEqual(f.events,['input','change']);
    const invalid=fieldExpressions('Mode','unsupported');assert.equal(f.run(invalid.ready),false);assert.equal(f.run(invalid.apply),false);
    const missing=fieldExpressions('Missing','value');assert.equal(f.run(missing.ready),false);assert.equal(f.run(missing.apply),false);
});
test('duplicate accessible labels fail before any field mutation',()=>{
    const f=fixture();f.fields.push(f.query);assert.equal(f.run(fieldExpressions('Query','value').apply),false);assert.deepEqual(f.writes,[]);
});
test('rendered operation catalog retains full field constraints and default enum option',()=>{
    const f=fixture(),catalog=JSON.parse(JSON.stringify(f.run(catalogExpression)));
    assert.deepEqual(catalog.names,['search','status']);assert.equal(catalog.tool,'search');
    assert.deepEqual(catalog.fields[0],{label:'Query',tag:'INPUT',type:'text',required:true,min:'',max:'',minLength:0,maxLength:4096,options:[]});
    assert.deepEqual(catalog.fields[1].options,['','lexical','vector']);
});
function actionsFixture({failure,newError=false,superseded=false}={}){
    const mutations=[],waits=[];let observations=0,submitted=false,tool='search';
    const actions={
        async observe(expression){
            new Function('return '+expression);
            if(expression===catalogExpression)return {tool,names:['search','capabilities'],fields:[]};
            if(expression===observationExpression)return submitted?{count:superseded?2:1,input:'search form',envelope:{status:'complete',future:{preserved:true}},...(newError?{error:'native refused'}:{})}:{count:0};
            return false;
        },
        async mutate(ready,apply){new Function('return '+ready);new Function('return '+apply);mutations.push(apply);if(apply.includes("HTMLSelectElement.prototype,'value'")){tool=apply.includes('\"capabilities\"')?'capabilities':'search';}if(apply.includes('Run form')){submitted=true;if(failure)throw failure;}},
        async settle(){waits.push('animation frame');},
        async wait(expression,label){new Function('return '+expression);waits.push(label);return true;},
        async waitValue(action){observations++;return action();}
    };
    return {actions,mutations,waits,get observations(){return observations;}};
}
test('Workbench read submits one Run form and retains raw complete response/catalog',async()=>{
    const f=actionsFixture(),result=await workbenchAction({action:'workbenchRead',tool:'search',fields:{Query:'symbol',Mode:'lexical'}},f.actions);
    assert.equal(result.envelope.future.preserved,true);assert.equal(result.submissions,1);assert.equal(result.catalog.tool,'search');
    assert.equal(f.mutations.filter(source=>source.includes('Run form')).length,1);
    assert.equal(f.waits.filter(label=>label==='exact actual Workbench field').length,2);
});
test('unknown Run dispatch is never replayed',async()=>{
    const failure=Error('CDP vanished'),f=actionsFixture({failure});
    await assert.rejects(workbenchAction({action:'workbenchRead',tool:'search',fields:{}},f.actions),error=>error===failure);
    assert.equal(f.mutations.filter(source=>source.includes('Run form')).length,1);assert.equal(f.observations,0);
});
test('rendered native error and superseded turn fail instead of adopting another response',async()=>{
    for(const options of [{newError:true},{superseded:true}]){
        const f=actionsFixture(options);await assert.rejects(workbenchAction({action:'workbenchRead',tool:'search',fields:{}},f.actions),/failed|superseded/);
    }
});
test('complete copies use session controls and selected copies use exactly one selected card',async()=>{
    for(const selected of [false,true])for(const format of ['json','markdown']){
        const f=actionsFixture();const result=await workbenchAction({action:'workbenchCopy',format,selected},f.actions);
        assert.equal(result.copy_dispatches,1);
        const copies=f.mutations.filter(source=>source.includes(selected?'Copy selected (1)':'Copy session as '+(format==='json'?'JSON':'Markdown')));
        assert.equal(copies.length,1);
        if(selected)assert.ok(f.mutations.some(source=>source.includes("index===0")));
    }
});
test('actual Workbench source still contains the audited accessible DOM controls',()=>{
    const fs=require('node:fs'),path=require('node:path');
    const source=fs.readFileSync(path.join(__dirname,'../../packages/vscode/src/workbenchWebview/main.tsx'),'utf8');
    for(const control of ['aria-label="Operation"','Reset this tool form','Run form','class="turn"','class="complete-response"','Copy session as JSON','Copy session as Markdown','Copy selected (','class="card-title"','aria-label="Request indexed evidence"'])assert.ok(source.includes(control),control);
});
