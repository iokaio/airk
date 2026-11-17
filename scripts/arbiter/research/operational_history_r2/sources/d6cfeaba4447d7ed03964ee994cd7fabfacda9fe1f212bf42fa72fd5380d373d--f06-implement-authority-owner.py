# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('crates/airk-cli/src/status_observation.rs');s=p.read_text();at=s.index('pub(crate) struct StatusOwner {')
t='''#[derive(Clone,Copy)]
pub(super) struct RequestTicket {
    pub(super) scope:Option<CacheScope>,
    pub(super) authority:u64,
    pub(super) scalar_epoch:u64,
    pub(super) receipt:airk_mcp::response::StatusReceipt,
    project:ProjectBinding,
}
#[derive(Default)]
struct WorkerAuthority {
    generation:u64,
    known:Option<(airk_core::identity::Incarnation,airk_core::service::WorkerIdentity)>,
}
#[derive(Debug)]
pub(super) struct RetiredRequest;
impl std::fmt::Display for RetiredRequest {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("frontend observation retired before query submission")}
}
impl std::error::Error for RetiredRequest {}
''';s=s[:at]+t+s[at:]
s=s.replace('''    scope: Mutex<Option<CacheScope>>,''','''    scope: Mutex<Option<CacheScope>>,
    authority:Mutex<WorkerAuthority>,''',1)
s=s.replace('''            scope: Mutex::new(None),''','''            scope: Mutex::new(None),
            authority:Default::default(),''',1)
at=s.index('    pub(crate) fn close(&self)')
helpers='''    fn namespace(scope:Option<CacheScope>)->Option<airk_core::identity::Incarnation>{
        scope.map(|scope|match scope{CacheScope::Pinned(namespace)=>namespace,CacheScope::Source(fence)|CacheScope::Registered(fence)=>fence.namespace})
    }
    fn project(scope:Option<CacheScope>)->Option<ProjectBinding>{
        match scope{Some(CacheScope::Source(fence)|CacheScope::Registered(fence))=>Some(fence.project),_=>None}
    }
    fn minimum(scope:Option<CacheScope>)->u64{
        match scope{Some(CacheScope::Source(fence)|CacheScope::Registered(fence))=>fence.fencing_generation,_=>0}
    }
    fn scope_domain_same(left:Option<CacheScope>,right:Option<CacheScope>)->bool{
        Self::namespace(left)==Self::namespace(right)&&Self::project(left)==Self::project(right)
    }
    pub(super) fn request_ticket(&self,project:ProjectBinding)->RequestTicket{
        let scope=self.scope.lock().unwrap_or_else(|e|e.into_inner());
        let authority=self.authority.lock().unwrap_or_else(|e|e.into_inner());
        RequestTicket{scope:*scope,authority:authority.generation,scalar_epoch:self.epoch.load(Ordering::Acquire),receipt:self.receipt(),project}
    }
    pub(super) fn ticket_scope_current(&self,ticket:&RequestTicket)->bool{
        !self.closed.load(Ordering::Acquire)&&*self.scope.lock().unwrap_or_else(|e|e.into_inner())==ticket.scope
    }
    pub(super) fn ticket_current(&self,ticket:&RequestTicket)->bool{
        let scope=self.scope.lock().unwrap_or_else(|e|e.into_inner());
        let authority=self.authority.lock().unwrap_or_else(|e|e.into_inner());
        !self.closed.load(Ordering::Acquire)&&*scope==ticket.scope&&authority.generation==ticket.authority
    }
    pub(super) fn admit_ticket(&self,ticket:&RequestTicket)->Result<OperationPermit<'_>>{
        let scope=self.scope.lock().unwrap_or_else(|e|e.into_inner());
        let authority=self.authority.lock().unwrap_or_else(|e|e.into_inner());
        if *scope!=ticket.scope||authority.generation!=ticket.authority{return Err(RetiredRequest.into());}
        self.closed.compare_exchange(false,false,Ordering::AcqRel,Ordering::Acquire).map_err(|_|RetiredRequest)?;
        Ok(OperationPermit{_owner:std::marker::PhantomData})
    }
    pub(super) fn transport_reusable(&self,ticket:&RequestTicket,bound:Option<CacheScope>,connection:&Connection)->bool{
        let scope=self.scope.lock().unwrap_or_else(|e|e.into_inner());
        let authority=self.authority.lock().unwrap_or_else(|e|e.into_inner());
        if self.closed.load(Ordering::Acquire)||*scope!=ticket.scope||authority.generation!=ticket.authority
            ||!Self::scope_domain_same(bound,*scope)||connection.identity.project!=ticket.project
            ||!connection.reusable(Instant::now())||connection.identity.fencing_generation<Self::minimum(*scope){return false;}
        authority.known.is_none_or(|(namespace,identity)|Some(namespace)==Self::namespace(*scope)&&identity==connection.identity)
    }
    pub(super) fn authenticated_worker(&self,ticket:RequestTicket,identity:airk_core::service::WorkerIdentity)->Result<RequestTicket>{
        let scope=self.scope.lock().unwrap_or_else(|e|e.into_inner());
        let mut authority=self.authority.lock().unwrap_or_else(|e|e.into_inner());
        if self.closed.load(Ordering::Acquire)||*scope!=ticket.scope||identity.project!=ticket.project
            ||Self::project(*scope).is_some_and(|project|project!=identity.project)
            ||identity.fencing_generation<Self::minimum(*scope){return Err(RetiredRequest.into());}
        let Some(namespace)=Self::namespace(*scope) else{return Ok(ticket);};
        let changed=match authority.known{
            None=>true,
            Some((known_namespace,known))=>{
                if known_namespace!=namespace||known.project!=identity.project
                    ||identity.fencing_generation<known.fencing_generation
                    ||identity.fencing_generation==known.fencing_generation&&identity!=known{return Err(RetiredRequest.into());}
                identity!=known
            }
        };
        if changed{
            authority.generation=authority.generation.checked_add(1).ok_or(RetiredRequest)?;
            authority.known=Some((namespace,identity));
            self.invalidate();
            *self.receipt.lock().unwrap_or_else(|e|e.into_inner())=Default::default();
        }
        if changed||authority.generation!=ticket.authority{
            Ok(RequestTicket{scope:*scope,authority:authority.generation,scalar_epoch:self.epoch.load(Ordering::Acquire),receipt:Default::default(),project:ticket.project})
        }else{Ok(ticket)}
    }
    fn retire_domain(&self,old:Option<CacheScope>,new:Option<CacheScope>){
        if !Self::scope_domain_same(old,new){
            let mut authority=self.authority.lock().unwrap_or_else(|e|e.into_inner());
            authority.known=None;
            if let Some(next)=authority.generation.checked_add(1){authority.generation=next;}else{self.closed.store(true,Ordering::Release);}
        }
    }
''';s=s[:at]+helpers+s[at:]
s=s.replace('''        self.invalidate();
        *self.scope.lock().unwrap_or_else(|e| e.into_inner()) = None;''','''        let mut scope=self.scope.lock().unwrap_or_else(|e|e.into_inner());
        self.retire_domain(*scope,None);
        self.invalidate();
        *scope = None;''',1)
s=s.replace('''        if *current != Some(scope) {
            self.invalidate();''','''        if *current != Some(scope) {
            self.retire_domain(*current,Some(scope));
            self.invalidate();''',1)
s=s.replace('''        self.invalidate();
        *scope = Some(new);''','''        self.retire_domain(*scope,Some(new));
        self.invalidate();
        *scope = Some(new);''',1)
p.write_text(s,newline='\n')
