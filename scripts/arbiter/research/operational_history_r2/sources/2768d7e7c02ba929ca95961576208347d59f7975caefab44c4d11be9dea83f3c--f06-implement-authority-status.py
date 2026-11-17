# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('crates/airk-cli/src/status_observation.rs');s=p.read_text()
# Capture the caller's receipt/authority before owner admission or metadata waits.
s=s.replace('''        let key = Key {''','''        let original_ticket=self.request_ticket(project,expires);
        let key = Key {''',1)
s=s.replace('''.materialize(root, project, args, source, expires, client)''','''.materialize(root, project, args, source, expires, client,original_ticket)''',1)
a=s.index('    fn channel_reusable(');b=s.index('    async fn materialize(',a);s=s[:a]+s[b:]
a=s.index('    async fn materialize(');b=s.index('    #[cfg(test)]',a);t=s[a:b]
t=t.replace('''        client: airk_core::usage::ClientFamily,
    ) -> Result<Snapshot>''','''        client: airk_core::usage::ClientFamily,
        original_ticket:RequestTicket,
    ) -> Result<Snapshot>''',1)
t=t.replace('''        let before = self.receipt_fence();''','''        let before=(original_ticket.scope,original_ticket.scalar_epoch,original_ticket.receipt);
        let mut ticket=original_ticket;''',1)
t=t.replace('''        let epoch = self.epoch.load(Ordering::Acquire);''','''        if source.is_none()&&ticket.scope!=Some(scope){
            ticket=self.request_ticket(project,expires);
            ticket.receipt=Default::default(); // freshly authenticated metadata, no unrelated acknowledgment
        }
        if ticket.scope!=Some(scope){return Err(RetiredRequest.into());}
        let mut epoch=ticket.scalar_epoch;''',1)
t=t.replace('''            .filter(|(bound, connection)| Self::channel_reusable(*bound, scope, connection));''','''            .filter(|(bound, connection)| self.transport_reusable(&ticket,Some(*bound),connection));''',1)
t=t.replace('''Connection::connect_once_checked(&root, project, client, Some(self))''','''Connection::connect_once_checked(&root,project,client,Some((self,&ticket)))''',1)
t=t.replace('''                    Err(error) => {
                        self.accepting()?;''','''                    Err(error) => {
                        if error.is::<RetiredRequest>()||!self.ticket_current(&ticket){return Err(RetiredRequest.into());}
                        self.accepting()?;''',1)
# Authenticate the actual worker (including a legitimate newer cold grant) only
# after the original metadata/expiry fence; receive a new default-receipt ticket
# for an authority advance, never a post-await read of another operation's receipt.
needle='''        self.accepting()?;
        if self.epoch.load(Ordering::Acquire) != epoch'''
replacement='''        self.accepting()?;
        ticket=self.authenticated_worker(ticket,connection.identity)?;
        epoch=ticket.scalar_epoch;
        if self.epoch.load(Ordering::Acquire) != epoch'''
assert needle in t;t=t.replace(needle,replacement,1)
# Scalar-only invalidation can refuse this observation without throwing away
# a still-current, healthy authenticated transport.
needle='''        {
            return Ok(Snapshot {
                options: request_args,
                scope,
                epoch,
                result: source_scope_superseded(),
                cached: None,
                submitted: false,
                reused: false,
                receipt: self.receipt(),
            });
        }
        connection.restore_status_receipt(self.receipt());
        let _request = self.admit_request()?;'''
replacement='''        {
            if self.transport_reusable(&ticket,Some(scope),&connection){*channel=Some((scope,connection));}
            return Ok(Snapshot {
                options: request_args,scope,epoch,result:source_scope_superseded(),cached:None,
                submitted:false,reused:false,receipt:ticket.receipt,
            });
        }
        connection.restore_status_receipt(ticket.receipt);
        let _request = self.admit_ticket(&ticket)?;'''
assert needle in t;t=t.replace(needle,replacement,1)
old='''        if !self.closed.load(Ordering::Acquire)
            && self
                .scope
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some_and(|current| Self::channel_reusable(scope, current, &connection))
        {'''
new='''        if self.transport_reusable(&ticket,Some(scope),&connection){''';assert old in t;t=t.replace(old,new,1)
s=s[:a]+t+s[b:]
s=s.replace('''Err(error) if airk_store::service::registry::source_scope_retryable(&error) =>''','''Err(error) if error.is::<RetiredRequest>()||airk_store::service::registry::source_scope_retryable(&error) =>''',1)
p.write_text(s,newline='\n')
