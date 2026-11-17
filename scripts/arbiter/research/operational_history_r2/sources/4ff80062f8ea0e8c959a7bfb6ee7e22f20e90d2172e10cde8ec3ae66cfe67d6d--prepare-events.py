# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/packages/vscode/src/serviceConsoleController.ts')
s=p.read_text(encoding='utf-8');start=s.index("        const epoch=await this.rpc('request_epoch'");end=s.index('        if(this.state.plan)await this.plan(false);',start)
s=s[:start]+'''        // Events already occurs in this poll and carries the registry namespace.
        // It is an observation, never a request-key epoch or mutation admission.
        try{await this.events();}catch(error){
            this.state.message=code(error);
            if(!(error instanceof ServiceError)||!['event_cursor_expired','event_scope_changed'].includes(error.code))throw error;
            if(error.code==='event_scope_changed')this.clearRegistryObservation(true);
            this.eventCursor=null;this.state.historyOmitted=true;
            try{await this.events(true);}catch(resetError){
                // Preserve the original refusal; the bounded fresh read cannot
                // erase it or replay an uncertain mutation.
                void this.host.diagnosticFailure?.('event_cursor_reset',resetError,this.bootstrap).catch(()=>{});
                throw error;
            }
        }
        this.pending();
        try{await this.page(false);}catch(error){this.state.message??=code(error);}
'''+s[end:]
start=s.index('    private async events():');end=s.index('    private async plan(',start)
s=s[:start]+'''    private async events(historyReset=false):Promise<void>{
        const client=this.client,binding=this.binding,instance=this.observedInstance;
        const page=record((await this.rpc({events:{project:null,cursor:this.eventCursor,limit:32}},'events')).page);
        this.authorized();
        if(this.client!==client||this.binding!==binding||this.observedInstance!==instance)throw new ServiceError('stale_event_observation');
        const namespace=identifier(page.namespace,16);
        if(!Array.isArray(page.events)||page.events.length>32)throw new ServiceError('invalid_event_page');
        const cursor=(value:unknown)=>{const c=record(value);if(identifier(c.namespace,16)!==namespace||c.project!==null)throw new ServiceError('invalid_event_page');count(c.after);if(c.through!==null)count(c.through);return c;};
        const next=page.next===null?null:cursor(page.next),tail=cursor(page.tail);
        const events=page.events.map(event=>{const row=record(event);count(row.sequence);return row;});
        // Namespace confirmation clears old page/selection/plan/history BEFORE
        // rows merge, including a replacement behind the same live service.
        this.restoreJournal(namespace);
        const merged=new Map(this.state.events.map(event=>[event.sequence,event]));
        events.forEach(event=>merged.set(event.sequence,event));
        this.state.historyOmitted||=historyReset||page.retained_prefix_lost===true||merged.size>128;
        this.state.events=[...merged.values()].slice(-128);
        // Keep an unfinished snapshot cursor; never advance straight to its tail.
        this.eventCursor=next??tail;
    }
'''+s[end:]
p.write_text(s,encoding='utf-8',newline='\n')
