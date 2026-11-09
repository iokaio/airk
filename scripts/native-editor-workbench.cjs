// SPDX-License-Identifier: Apache-2.0
// Actual Workbench DOM controls only; no injected VS Code or extension API.
const fieldNodes="[...document.querySelectorAll('.tool-form form input[aria-label],.tool-form form select[aria-label]')]";
function fieldExpressions(label,value){
    if(typeof label!=='string'||!label||label.length>256||typeof value!=='string'||value.length>4096)throw Error('Invalid exact Workbench field');
    const lookup=`const rows=${fieldNodes}.filter(n=>n.getAttribute('aria-label')===${JSON.stringify(label)});const node=rows.length===1?rows[0]:null;`;
    return {ready:`(()=>{${lookup}return !!node&&!node.disabled&&(node.tagName!=='SELECT'||[...node.options].some(o=>o.value===${JSON.stringify(value)}));})()`,
        apply:`(()=>{${lookup}if(!node||node.disabled||(node.tagName==='SELECT'&&![...node.options].some(o=>o.value===${JSON.stringify(value)})))return false;Object.getOwnPropertyDescriptor(node.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype,'value').set.call(node,${JSON.stringify(value)});node.dispatchEvent(new Event(node.tagName==='SELECT'?'change':'input',{bubbles:true}));return true;})()`,
        readback:`(()=>{${lookup}return !!node&&node.value===${JSON.stringify(value)};})()`};
}
const catalogExpression=`(()=>{const select=document.querySelector('.tool-form select[aria-label="Operation"]');if(!select)return;return {names:[...select.options].map(o=>o.value),tool:select.value,fields:${fieldNodes}.map(n=>({label:n.getAttribute('aria-label'),tag:n.tagName,type:n.type,required:n.required,min:n.getAttribute('min')??'',max:n.getAttribute('max')??'',minLength:Number(n.getAttribute('minlength')??0),maxLength:Number(n.getAttribute('maxlength')??4096),options:n.tagName==='SELECT'?[...n.options].map(o=>o.value):[]}))};})()`;
function buttonExpressions(label,scope='document'){
    const lookup=`const root=${scope};const rows=root?[...root.querySelectorAll('button')].filter(b=>b.textContent.trim()===${JSON.stringify(label)}):[];const node=rows.length===1?rows[0]:null;`;
    return {ready:`(()=>{${lookup}return !!node&&!node.disabled;})()`,apply:`(()=>{${lookup}if(!node||node.disabled)return false;node.click();return true;})()`};
}
function selectorExpressions(selector){
    const lookup=`const rows=[...document.querySelectorAll(${JSON.stringify(selector)})];const node=rows.length===1?rows[0]:null;`;
    return {ready:`(()=>{${lookup}return !!node&&!node.disabled;})()`,apply:`(()=>{${lookup}if(!node||node.disabled)return false;node.click();return true;})()`};
}
const latest="[...document.querySelectorAll('article.turn')].at(-1)";
function selectionExpressions(){
    const boxes=`[...(${latest})?.querySelectorAll('.card-title input[type="checkbox"]')??[]]`;
    return {ready:`${boxes}.length>0&&${boxes}.every(n=>!n.disabled)`,
        apply:`(()=>{const boxes=${boxes};if(!boxes.length||boxes.some(n=>n.disabled))return false;for(const [index,node]of boxes.entries()){if(node.checked!==(index===0))node.click();}return true;})()`,
        readback:`(()=>{const turn=${latest},boxes=${boxes},copies=turn?[...turn.querySelectorAll('button')].filter(n=>n.textContent.trim()==='Copy selected (1)'):[];return boxes.length>0&&boxes[0].checked&&boxes.filter(n=>n.checked).length===1&&copies.length>0&&copies.every(n=>!n.disabled&&n.getAttribute('aria-label')?.startsWith('Copy 1 selected results as '));})()`};
}
const observationExpression=`(()=>{const turns=[...document.querySelectorAll('article.turn')],turn=turns.at(-1);const text=turn?.querySelector('.complete-response pre')?.textContent;return {count:turns.length,input:turn?.querySelector('.input')?.textContent,error:turn?.querySelector('p.error[role="alert"]')?.textContent,envelope:text?JSON.parse(text):undefined};})()`;
async function workbenchAction(request,actions){
    const mutate=expressions=>actions.mutate(expressions.ready,expressions.apply);
    const click=(label,scope)=>mutate(buttonExpressions(label,scope));
    if(request.action==='workbenchRead'){
        if(typeof request.tool!=='string'||!request.tool||request.tool.length>64||!request.fields||Array.isArray(request.fields)||typeof request.fields!=='object'||Object.keys(request.fields).length>32)throw Error('Invalid Workbench read request');
        for(const [label,value]of Object.entries(request.fields))fieldExpressions(label,value);
        if(!await actions.observe("!!document.querySelector('#evidence-options')"))await mutate(selectorExpressions('button[aria-controls="evidence-options"]'));
        if(!await actions.observe("!!document.querySelector('.tool-form')"))await click('Tool forms',"document.querySelector('#evidence-options')");
        await actions.wait("!!document.querySelector('.tool-form select[aria-label=\"Operation\"]')",'actual Workbench operation catalog');
        const select=`document.querySelector('.tool-form select[aria-label="Operation"]')`;
        const selectOperation=async name=>{
            const tool=JSON.stringify(name);
            await actions.mutate(`!!${select}&&[...${select}.options].some(o=>o.value===${tool})`,`(()=>{const node=${select};if(!node||![...node.options].some(o=>o.value===${tool}))return false;Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(node,${tool});node.dispatchEvent(new Event('change',{bubbles:true}));return true;})()`);
            await actions.settle();
            await actions.wait(`(${catalogExpression})?.tool===${tool}`,'selected actual Workbench operation');
        };
        const initial=await actions.observe(catalogExpression);
        if(!initial||!Array.isArray(initial.names)||initial.names.length>64||new Set(initial.names).size!==initial.names.length||!initial.names.includes(request.tool))throw Error('Workbench operation catalog unavailable');
        const forms={};
        for(const name of initial.names){await selectOperation(name);forms[name]=(await actions.observe(catalogExpression)).fields;}
        await selectOperation(request.tool);
        await mutate(selectorExpressions('.tool-form button[aria-label="Reset this tool form"]'));
        await actions.settle();
        await actions.wait(`${fieldNodes}.every(n=>n.value==='')`,'reset actual Workbench fields');
        for(const [label,value]of Object.entries(request.fields)){
            const expressions=fieldExpressions(label,value);await mutate(expressions);await actions.settle();await actions.wait(expressions.readback,'exact actual Workbench field');
        }
        const catalog=await actions.observe(catalogExpression);
        if(!catalog||catalog.tool!==request.tool)throw Error('Workbench form changed before submission');
        const before=await actions.observe(observationExpression);
        await click('Run form',"document.querySelector('.tool-form')");
        const result=await actions.waitValue(async()=>{
            const current=await actions.observe(observationExpression);
            if(current?.count>before.count+1)throw Error('Workbench result superseded by another turn');
            if(current?.count!==before.count+1||current.input!==request.tool+' form')return;
            if(current.error){actions.record(current);throw Error('Actual Workbench read failed: '+current.error);}
            return current.envelope?current:undefined;
        },'new complete actual Workbench response');
        return {envelope:result.envelope,catalog:{...catalog,forms},turn:{ordinal:result.count,input:result.input},submissions:1};
    }
    if(request.action==='workbenchCopy'){
        if(!['json','markdown'].includes(request.format)||typeof request.selected!=='boolean')throw Error('Invalid Workbench copy request');
        if(!request.selected){
            if(!await actions.observe("!!document.querySelector('#session-settings')"))await mutate(selectorExpressions('button[aria-controls="session-settings"]'));
            await click('Copy session as '+(request.format==='json'?'JSON':'Markdown'),"document.querySelector('#session-settings')");
        }else{
            const output=`${latest}?.querySelector('[aria-label="Output format"]')`;
            await click('structured',output);await actions.settle();
            const selection=selectionExpressions();
            await mutate(selection);await actions.settle();
            await actions.wait(selection.readback,'exactly one selected Workbench card and rendered copy state');
            await click(request.format,output);await actions.settle();
            await click('Copy selected (1)',latest);
        }
        return {format:request.format,selected:request.selected,copy_dispatches:1};
    }
    throw Error('Unknown Workbench action');
}
module.exports={workbenchAction,fieldExpressions,catalogExpression,buttonExpressions,selectorExpressions,selectionExpressions,observationExpression};
