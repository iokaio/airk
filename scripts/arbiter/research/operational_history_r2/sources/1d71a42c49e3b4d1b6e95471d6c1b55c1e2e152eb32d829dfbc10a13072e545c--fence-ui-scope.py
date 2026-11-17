# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/packages/vscode/src')
f=p/'serviceWorkspace.ts';s=f.read_text(encoding='utf-8')
s=s.replace("export type SelectedRootObservation =\n    {kind:","export type SelectedRootObservation = (\n    {kind:")
s=s.replace("|{kind:'removed'|'missing'|'ambiguous';revision:number|null;namespace:string|null};","|{kind:'removed'|'missing'|'ambiguous';revision:number|null;namespace:string|null}) & {root:string;collection:string|null};")
s=s.replace("return {kind:observed.outcome,revision:observed.revision,namespace:observed.namespace};","return {kind:observed.outcome,revision:observed.revision,namespace:observed.namespace,root:observed.root,collection};")
s=s.replace("return {kind:'registered',revision:observed.revision!,namespace:observed.namespace!,fencing_generation:observed.fencing_generation!,","return {kind:'registered',revision:observed.revision!,namespace:observed.namespace!,fencing_generation:observed.fencing_generation!,root:observed.root,collection,")
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'serviceExtension.ts';s=f.read_text(encoding='utf-8')
needle='    private selectedEligible?:{spec:LaunchSpec;opened:string;generation:number;folders:string;host:RuntimeHost};'
s=s.replace(needle,needle+'\n    private selectedScope?:{root:string;collection:string|null;generation:number;folders:string;host:RuntimeHost};')
start=s.index('    private uiSpecs():LaunchSpec[]');end=s.index('    private isUiCurrent',start)
s=s[:start]+'''    private uiSpecs():LaunchSpec[]{
        const early=this.selectedEligible,scope=this.selectedScope;
        // Exact native absence/replacement suppresses stale UI receipts only.
        // Complete configured workspace MCP declarations still use this.specs.
        const full=scope&&this.selectedFence(scope)?this.specs.filter(spec=>!(same(spec.root,scope.root)&&(scope.collection===null||spec.collection===scope.collection))):this.specs;
        if(!early||!this.selectedFence(early))return full;
        return [early.spec,...(this.currentBinding()?full:[]).filter(spec=>spec.service?.project!==early.spec.service?.project)];
    }
'''+s[end:]
s=s.replace("private selectedFence(value:NonNullable<ServiceExtension['selectedEligible']>)", "private selectedFence(value:Pick<NonNullable<ServiceExtension['selectedEligible']>,'generation'|'folders'|'host'>)")
s=s.replace("        if(selected?.kind==='ambiguous'){this.selectedEligible=undefined;throw new ServiceError('workspace_binding_review_required');}","        this.selectedScope=selected?{root:selected.root,collection:selected.collection,generation,folders:identity,host}:undefined;")
s=s.replace('        this.treeChanges.fire(); // Explorer/Workbench only: never definitions.fire here.', "        this.treeChanges.fire(); // Explorer/Workbench only: never definitions.fire here.\n        if(selected?.kind==='ambiguous')throw new ServiceError('workspace_binding_review_required');")
s=s.replace('this.specs=[];this.roots=[];this.binding=undefined;this.selectedEligible=undefined;', 'this.specs=[];this.roots=[];this.binding=undefined;this.selectedEligible=undefined;this.selectedScope=undefined;')
s=s.replace('this.host=undefined;this.selectedEligible=undefined;this.specs=[];', 'this.host=undefined;this.selectedEligible=undefined;this.selectedScope=undefined;this.specs=[];')
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'test/serviceExtension.test.ts';s=f.read_text(encoding='utf-8');start=s.index("test('selected Explorer project");pre=s[:start];t=s[start:]
t=t.replace("    context.mock.method(RuntimeHost.prototype as any,'invoke'", "    let selectedOutcome='registered',selectedProject=registrations[0].binding;\n    context.mock.method(RuntimeHost.prototype as any,'invoke'",1)
t=t.replace("namespace:'12'.repeat(16),revision:7,observed_unix_ms:1,outcome:'registered',registration:row,fencing_generation:1", "namespace:'12'.repeat(16),revision:7,observed_unix_ms:1,outcome:selectedOutcome,registration:selectedOutcome==='registered'?{...row,binding:selectedProject}:null,fencing_generation:selectedOutcome==='registered'?1:null",1)
t=t.replace('let release!:()=>void,published!:()=>void,host:any,definitionsChanged=0;', 'let release!:()=>void,published!:()=>void,host:any,controller:any,definitionsChanged=0;')
t=t.replace('createTreeView:()=>disposable,', 'createTreeView:(_name:string,options:any)=>{controller=options.treeDataProvider;return disposable;},',1)
needle="        assert.ok(result[0].args.includes('--additional-root'));assert.ok(result[0].args.includes(otherCanonical));"
t=t.replace(needle,needle+'''
        const configured=JSON.stringify(controller.specs),events=definitionsChanged;
        selectedOutcome='removed';await controller.observeSelected(binding);
        assert.ok(!host.specs().some((spec:any)=>spec.root===canonical),'native removed scope cannot remain visible through stale complete UI specs');
        assert.equal(JSON.stringify(controller.specs),configured,'UI removal does not rewrite configured MCP scopes');
        selectedOutcome='registered';selectedProject='56'.repeat(32);await controller.observeSelected(binding);
        assert.deepEqual(host.specs().filter((spec:any)=>spec.root===canonical).map((spec:any)=>spec.service.project),[selectedProject]);
        assert.equal(JSON.stringify(controller.specs),configured);assert.equal(definitionsChanged,events);
''')
f.write_text(pre+t,encoding='utf-8',newline='\n')
