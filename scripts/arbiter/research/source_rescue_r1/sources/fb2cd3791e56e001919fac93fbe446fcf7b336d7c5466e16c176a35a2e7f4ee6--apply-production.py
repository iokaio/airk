# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import os,json,difflib
root=Path.cwd();out=root/'artifacts/v190/final-plan/exec-20261116T052942Z/ui-workbench-composer-r1';p=root/'packages/vscode/src/workbenchWebview/main.tsx';raw=p.read_bytes();text=raw.decode().replace(chr(13)+chr(10),chr(10));original=text
text=text.replace("type Menu = 'history' | 'settings' | 'add' | 'selection';", "type Menu = 'history' | 'settings' | 'add' | 'selection' | 'context' | 'options' | 'forms';")
anchor='    const latest=state.session?.turns.at(-1);'
new='''    const plusOpen=menu==='add'||menu==='context'||menu==='options'||menu==='forms';
    const closeMenu=(restore=false)=>{setMenu(undefined);if(restore)requestAnimationFrame(()=>document.querySelector<HTMLButtonElement>(`button[aria-controls="${menuTrigger.current}"]`)?.focus());};
    const showContext=()=>{setContextQuestion(question=>question||input.trim());setContextIssue('');setMenu('context');};
    const gatherContext=(custom=false)=>{
        if(!state.project||state.busy)return;
        try{
            const message=parseWorkbenchMessage({type:'answer',question:contextQuestion,...(custom?{budget:JSON.parse(contextBudget)}:{})});
            if(!message)throw new Error('Provide a nonempty question within 16,384 UTF-8 bytes and every integer native budget field.');
            send(message);setContextIssue('');setContextQuestion('');setInput('');closeMenu(true);
        }catch(error){setContextIssue(error instanceof Error?error.message:String(error));}
    };
    const menuKeys=(event:KeyboardEvent)=>{
        const items=[...document.querySelectorAll<HTMLButtonElement>('#evidence-options [role="menuitem"]')].filter(button=>!button.disabled),position=items.indexOf(document.activeElement as HTMLButtonElement);
        if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)&&items.length){event.preventDefault();const next=event.key==='Home'?0:event.key==='End'?items.length-1:event.key==='ArrowDown'?(position+1)%items.length:(position-1+items.length)%items.length;items[next]?.focus();}
        else if(event.key==='Tab'){event.preventDefault();setMenu(undefined);requestAnimationFrame(()=>{const next=event.shiftKey?document.querySelector<HTMLButtonElement>('button[aria-controls="evidence-options"]'):document.querySelector<HTMLButtonElement>('.send-button:not(:disabled)');(next??composer.current)?.focus();});}
    };
    useLayoutEffect(()=>{
        if(!plusOpen)return;
        const panel=document.getElementById('evidence-options'),selector=menu==='add'?'[role="menuitem"]:not(:disabled)':menu==='context'?'[aria-label="Context question"]':menu==='options'?'input':'[aria-label="Operation"]';
        (panel?.querySelector<HTMLElement>(selector)??panel?.querySelector<HTMLButtonElement>('button'))?.focus();
    },[menu]);
    const latest=state.session?.turns.at(-1);'''
assert anchor in text;text=text.replace(anchor,new).replace("else if(event.data?.type==='prefillForm'){setMenu('add');}","else if(event.data?.type==='prefillForm'){menuTrigger.current='evidence-options';setMenu('forms');}")
start=text.index('            <button type="button" disabled={!state.project || state.busy || !input.trim()}');end=text.index("            {input.startsWith('/')",start);text=text[:start]+text[end:]
start=text.index('            <div class="compose-actions"><div class="menu-anchor add-anchor">');end=text.index('{state.busy?<button class="icon-button send-button"',start)
new='''            <div class="compose-actions"><div class="menu-anchor add-anchor"><button class="icon-button add-button" title="Evidence options" aria-label="Evidence options" aria-haspopup={menu==='context'||menu==='options'||menu==='forms'?'dialog':'menu'} aria-expanded={plusOpen} aria-controls="evidence-options" onClick={e=>plusOpen?closeMenu():toggle('add',e)}><Icon name="plus"/></button>
                {menu==='add'&&<section class="popover above evidence-menu" id="evidence-options" role="menu" aria-label="Evidence options" onKeyDown={menuKeys}>
                    <h2 role="presentation">Evidence options</h2>
                    <div role="group" aria-label="Collect"><h3 role="presentation">Collect</h3>
                        <button type="button" role="menuitem" disabled={!state.project||state.busy} onClick={showContext}>Gather cited context…</button>
                        <button type="button" role="menuitem" disabled={!state.project||state.busy} onClick={()=>{setMenu('forms');send({type:'forms'});}}>Tool forms…</button></div>
                    <div role="group" aria-label="Request options"><h3 role="presentation">Request options</h3>
                        <button type="button" role="menuitem" disabled={!state.session} onClick={()=>setMenu('options')}>Scope and recipe…</button>
                        <button type="button" role="menuitem" disabled={!state.project||state.busy||!input.trim()&&!recipe} onClick={()=>run(true)}>Prepare plan before running</button></div>
                    <div role="group" aria-label="Help"><button type="button" role="menuitem" onClick={()=>action({type:'help'})}>Commands and help</button></div>
                </section>}
                {plusOpen&&menu!=='add'&&<section class="popover above evidence-panel" id="evidence-options" role="dialog" aria-label={menu==='context'?'Gather cited context':menu==='options'?'Scope and recipe':'Tool forms'}>
                    <div class="popover-heading"><button type="button" class="icon-button" aria-label="Back to evidence options" title="Back to evidence options" onClick={()=>setMenu('add')}>←</button><h2>{menu==='context'?'Gather cited context':menu==='options'?'Scope and recipe':'Tool forms'}</h2><button type="button" class="icon-button" aria-label="Close evidence options" title="Close" onClick={()=>closeMenu(true)}>×</button></div>
                    {menu==='context'?<>
                        <label>Question<textarea aria-label="Context question" maxLength={16384} value={contextQuestion} onInput={e=>setContextQuestion(e.currentTarget.value)}/></label>
                        <p class="note">Collects cited evidence with bounded canonical reads and reports gaps. Optional Arbiter models require explicit Service enrollment.</p>
                        <button type="button" class="context-submit" disabled={!state.project||state.busy||!contextQuestion.trim()} onClick={()=>gatherContext()}>{state.busy?'Collecting evidence…':'Gather cited context'}</button>
                        <details><summary>Adjust native budget</summary><label>Native context budget (JSON)<textarea aria-label="Native context budget" maxLength={2048} value={contextBudget} onInput={e=>setContextBudget(e.currentTarget.value)}/></label>
                            <p class="note">The native service validates budget limits. A smaller budget can return gaps or omissions.</p>
                            <button type="button" disabled={!state.project||state.busy||!contextQuestion.trim()} onClick={()=>gatherContext(true)}>Gather with this budget</button></details>
                        {contextIssue&&<p role="alert">{contextIssue}</p>}
                    </>:menu==='options'?<>
                        <label>Scope<input aria-label="Literal collection-relative path prefix" placeholder="Project root, or src/component/" maxLength={1024} value={scope} onInput={e=>setScope(e.currentTarget.value)}/></label>
                        <label>Recipe<select aria-label="Recipe" value={recipe} onChange={e=>setRecipe(e.currentTarget.value)}><option value="">Command or supported request</option>{RECIPES.map(([id,label])=><option value={id}>{label}</option>)}</select></label>
                    </>:state.operations?<ToolForm state={state} issue={formIssue} currentDraft={currentDraft} onSelect={tool=>changeForm({tool})} onField={(tool,name,value)=>changeForm({tool,field:{name,value}})} onReset={tool=>changeForm({tool},true)} onIssue={setFormIssue}/>:<><p role="status">Loading tool forms…</p><button type="button" disabled={!state.project||state.busy} onClick={()=>send({type:'forms'})}>Retry tool forms</button></>}
                </section>}
            </div><small title="Requests read the local index. Refresh requires review.">Sources read-only</small>'''
text=text[:start]+new+text[end:];assert 'Cited context question and budget' not in text
newline=chr(13)+chr(10) if raw.count(bytes([13,10])) else chr(10);after=text.replace(chr(10),newline).encode();stat=p.stat();(out/'main-before.tsx').write_bytes(raw);(out/'main-after.tsx').write_bytes(after)
with p.open('r+b') as f:f.write(after);f.truncate();f.flush();os.fsync(f.fileno())
assert p.stat().st_ino==stat.st_ino
css=root/'packages/vscode/src/workbenchWebview/style.css';rawcss=css.read_bytes();(out/'style-before.css').write_bytes(rawcss);addition='''
.evidence-menu [role=group]+[role=group]{border-top:1px solid var(--vscode-panel-border);margin-top:4px;padding-top:4px}
.evidence-menu h3{font-size:.85em;color:var(--vscode-descriptionForeground);font-weight:500;margin:4px 8px}
.evidence-menu [role=menuitem]{display:block;width:100%;border:0;background:transparent;text-align:left;padding:7px 8px;border-radius:4px}
.evidence-menu [role=menuitem]:focus{background:var(--vscode-list-hoverBackground,var(--vscode-button-secondaryHoverBackground))}
.evidence-panel{width:340px}.popover-heading{display:flex;align-items:center;gap:4px}.popover-heading h2{flex:1;margin:4px 0}
.evidence-panel>.context-submit{margin:6px}.evidence-panel textarea{min-height:80px;max-height:24vh;border:1px solid var(--vscode-input-border,var(--vscode-panel-border));background:var(--vscode-input-background)}
.evidence-panel [role=alert],.evidence-panel [role=status]{margin:8px 6px}
''';nl=chr(13)+chr(10) if rawcss.count(bytes([13,10])) else chr(10);nextcss=rawcss+addition.replace(chr(10),nl).encode();statcss=css.stat()
with css.open('r+b') as f:f.write(nextcss);f.truncate();f.flush();os.fsync(f.fileno())
assert css.stat().st_ino==statcss.st_ino
(out/'production.patch').write_text(''.join(difflib.unified_diff(original.splitlines(True),text.splitlines(True),fromfile='a/packages/vscode/src/workbenchWebview/main.tsx',tofile='b/packages/vscode/src/workbenchWebview/main.tsx')),encoding='utf-8');print(json.dumps({'main_bytes':len(after),'style_bytes':len(nextcss),'file_ids_preserved':True,'old_persistent_context_removed':True},indent=2))
