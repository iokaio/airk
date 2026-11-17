# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/packages/vscode/src')
f=p/'serviceExtension.ts';s=f.read_text(encoding='utf-8');needle="        const selected=folder&&await this.workspace.selectedObservation(binding,folder.uri.fsPath,async(root,collection)=>{const observed=await host.sourceScope(binding,root,collection);valid();return observed;});valid();"
s=s.replace(needle,needle+"\n        if(selected?.kind==='ambiguous'){this.selectedEligible=undefined;throw new ServiceError('workspace_binding_review_required');}")
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'test/serviceExtension.test.ts';s=f.read_text(encoding='utf-8');start=s.index("test('selected Explorer project")
pre=s[:start];t=s[start:]
t=t.replace("    context.mock.method(RuntimeHost.prototype,'client',()=>({projects:async(_after:unknown,_revision:unknown,_limit:unknown,inactive:boolean)=>({revision:7,cached:true,observed_unix_ms:1,after:null,projects:inactive?[]:registrations.map(registration=>({project:{registration}}))}),dispose(){}}) as any);", "    let pageReads=0;\n    context.mock.method(RuntimeHost.prototype,'client',()=>({projects:async(_after:unknown,_revision:unknown,_limit:unknown,inactive:boolean)=>{pageReads++;await held;return {revision:7,cached:true,observed_unix_ms:1,after:null,projects:inactive?[]:registrations.map(registration=>({project:{registration}}))};},dispose(){}}) as any);")
t=t.replace("        await selected;assert.equal(host.specs()[0].service.project,registrations[0].binding);", "        await selected;assert.equal(host.specs()[0].service.project,registrations[0].binding);\n        assert.equal(pageReads,0,'selected UI must publish without requesting unrelated console pages');")
f.write_text(pre+t,encoding='utf-8',newline='\n')
