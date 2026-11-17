# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('crates/airk-cli/src/project_client.rs');s=p.read_text().replace('pub(crate) use status_observation::StatusOwner;','pub(crate) use status_observation::StatusOwner;\nuse status_observation::{RequestTicket,RetiredRequest};',1)
s=s.replace('''        owner: Option<&StatusOwner>,
    ) -> Result<Self> {
        if let Some(owner) = owner {
            owner.accepting()?;
        }
        let ControlResponse::Health { observation } =
            service_runtime::request(root, ControlMethod::Health).await?''','''        owner: Option<(&StatusOwner,&RequestTicket)>,
    ) -> Result<Self> {
        let response={
            let _health_request=owner.map(|(owner,ticket)|owner.admit_ticket(ticket)).transpose()?;
            service_runtime::request(root,ControlMethod::Health).await?
        };
        let ControlResponse::Health { observation } = response''',1)
s=s.replace('''let _broker_request = owner.map(StatusOwner::admit_request).transpose()?;''','''let _broker_request = owner.map(|(owner,ticket)|owner.admit_ticket(ticket)).transpose()?;''',1)
at=s.index('    async fn connect_once(\n')
method='''    async fn connect_frontend(root:&Path,project:ProjectBinding,client:airk_core::usage::ClientFamily,
        owner:&StatusOwner,ticket:RequestTicket,once:bool)->Result<Self>{
        if once{return Self::connect_once_checked(root,project,client,Some((owner,&ticket))).await;}
        retry_setup(||Self::connect_once_checked(root,project,client,Some((owner,&ticket))),
            Duration::from_secs(30),8,Duration::from_millis(100)).await
    }
''';s=s[:at]+method+s[at:]
s=s.replace('''struct ClientState {
    connection: Option<Connection>,''','''struct ClientState {
    connection: Option<Connection>,
    connection_scope:Option<CacheScope>,''',1)
a=s.index('    fn call_bound(\n');b=s.index('\n}\n\npub fn tools',a)
new='''    fn call_bound(&self,tool:String,arguments:serde_json::Map<String,serde_json::Value>,
        observed:Option<SourceFence>,source_expires:Option<Instant>)->airk_mcp::remote::RemoteFuture<'_>{
        Box::pin(async move{
            if tool=="airk_help"{return Ok(serde_json::to_value(airk_mcp::help::call(serde_json::Value::Object(arguments)))?);}
            if matches!(tool.as_str(),"airk_status"|"airk_diagnostics")
                &&serde_json::from_value::<airk_mcp::StatusArgs>(serde_json::Value::Object(arguments.clone())).is_err(){
                return Ok(airk_mcp::unified::failure("INVALID_SCHEMA","Invalid status arguments."));
            }
            if let Some(fence)=observed && !self.accept_source_fence(fence){return Ok(source_scope_superseded());}
            if source_expires.is_some_and(|until|Instant::now()>=until){return Ok(source_scope_superseded());}
            if tool=="airk_status"{
                let client=self.client.lock().map(|value|*value).unwrap_or(airk_core::usage::ClientFamily::Unknown);
                return self.status.status(self.root.clone(),self.project,arguments,observed,source_expires,client).await;
            }
            if tool=="airk_diagnostics"{self.status.invalidate();crate::runtime_metadata::observe_process();}
            if observed.is_none(){self.status.bind_initial_pinned_scope(self.root.clone(),self.project).await?;}
            // Capture logical receipt and actual-authority generation before any
            // evidence-channel wait. A later grant returns its own fresh ticket.
            let mut ticket=self.status.request_ticket(self.project);
            if observed.is_some_and(|expected|ticket.scope!=Some(CacheScope::Source(expected))){return Ok(source_scope_superseded());}
            let mut state=self.state.lock().await;
            if source_expires.is_some_and(|until|Instant::now()>=until)
                ||observed.is_some_and(|fence|!self.source_fence_current(CacheScope::Source(fence)))
                ||!self.status.ticket_current(&ticket){return Ok(source_scope_superseded());}
            if state.connection.as_ref().is_none_or(|connection|!self.status.transport_reusable(&ticket,state.connection_scope,connection)){
                state.connection=None;state.connection_scope=None;
                let client=self.client.lock().map(|value|*value).unwrap_or(airk_core::usage::ClientFamily::Unknown);
                let connected=Connection::connect_frontend(&self.root,self.project,client,&self.status,ticket,tool=="airk_diagnostics").await;
                if source_expires.is_some_and(|until|Instant::now()>=until)
                    ||observed.is_some_and(|fence|!self.source_fence_current(CacheScope::Source(fence)))
                    ||!self.status.ticket_scope_current(&ticket){return Ok(source_scope_superseded());}
                let connection=match connected{
                    Ok(connection)=>connection,
                    Err(error) if error.is::<RetiredRequest>()=>return Ok(source_scope_superseded()),
                    Err(_) if tool=="airk_diagnostics"=>return Ok(crate::runtime_metadata::unavailable(&self.root,self.project).await),
                    Err(error)=>return setup_rejection(&error).ok_or(error),
                };
                ticket=match self.status.authenticated_worker(ticket,connection.identity){
                    Ok(ticket)=>ticket,Err(error) if error.is::<RetiredRequest>()=>return Ok(source_scope_superseded()),Err(error)=>return Err(error)};
                state.connection_scope=ticket.scope;state.connection=Some(connection);
            }else{
                ticket=match self.status.authenticated_worker(ticket,state.connection.as_ref().expect("checked evidence connection").identity){
                    Ok(ticket)=>ticket,Err(error) if error.is::<RetiredRequest>()=>return Ok(source_scope_superseded()),Err(error)=>return Err(error)};
            }
            if source_expires.is_some_and(|until|Instant::now()>=until)
                ||observed.is_some_and(|fence|!self.source_fence_current(CacheScope::Source(fence)))
                ||!self.status.ticket_current(&ticket){return Ok(source_scope_superseded());}
            let receipt=if ticket.scope.is_some(){ticket.receipt}else{state.status_receipt};
            let connection=state.connection.as_mut().ok_or_else(||anyhow::anyhow!("project connection unavailable"))?;
            connection.restore_status_receipt(receipt);
            let _query=match self.status.admit_ticket(&ticket){Ok(permit)=>permit,Err(_)=>return Ok(source_scope_superseded())};
            let result=connection.call(tool,arguments).await;
            if result.is_ok(){
                let receipt=connection.status_receipt();state.status_receipt=receipt;
                if ticket.scope.is_some(){self.status.adopt_receipt((ticket.scope,ticket.scalar_epoch,ticket.receipt),receipt);}
            }else{state.connection=None;state.connection_scope=None;}
            // Logical invalidation does not retire authenticated transport authority.
            self.status.invalidate();result
        })
    }
''';s=s[:a]+new+s[b:]
# Deliberately absent broker: one fresh diagnostic setup is enough to prove that
# the retired old channel gets no write, without the ordinary30s retry budget.
s=s.replace('''let _result=if pinned{airk_mcp::remote::RemoteTools::call(&*backend,"airk_read_file".into(),Default::default()).await}''','''let _result=if pinned{airk_mcp::remote::RemoteTools::call(&*backend,"airk_diagnostics".into(),Default::default()).await}''',1)
p.write_text(s,newline='\n')
