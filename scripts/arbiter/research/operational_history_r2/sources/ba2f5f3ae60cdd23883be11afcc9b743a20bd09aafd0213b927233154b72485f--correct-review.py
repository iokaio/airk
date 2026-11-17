# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f16b-draft/source/crates/airk-cli/src/unified_mcp/explorer_adapter.rs')
s=p.read_text(encoding='utf-8')
s=s.replace('    sequence: u64,','    sequence: u64,\n    started_at: Instant,')
s=s.replace('        deadline: Instant,\n        client: String,','        client: String,')
s=s.replace('        ensure!(\n            Instant::now() < deadline,\n            "Explorer establishment deadline expired"\n        );\n','')
s=s.replace('        let mut child = ContainedChild::spawn(&mut command)?;','        let mut child = ContainedChild::spawn(&mut command)?;\n        let started_at = Instant::now();')
s=s.replace('            sequence: 0,','            sequence: 0,\n            started_at,')
s=s.replace('        let deadline = Instant::now() + Duration::from_secs(15);\n        let closed', '        let closed')
s=s.replace('result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),self.worker.lock())=>result.context("Explorer adapter admission timed out")?,','result=self.worker.lock()=>result,')
s=s.replace('result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),previous)=>result.context("Explorer adapter admission timed out")?,','result=previous=>result,')
s=s.replace('!self.admission.closed() && Instant::now() < deadline','!self.admission.closed()')
s=s.replace('!owner.admission.closed() && Instant::now() < deadline','!owner.admission.closed()')
s=s.replace('            if adapter.is_none() {','            let mut execution_deadline = None;\n            if adapter.is_none() {',1)
s=s.replace('                    &owner.admission,\n                    deadline,\n                    client_name,','                    &owner.admission,\n                    client_name,')
s=s.replace('                started.initialize(&owner.admission, deadline)?;','                let deadline = child_work_deadline(started.started_at);\n                started.initialize(&owner.admission, deadline)?;\n                execution_deadline = Some(deadline);')
needle='            let result = adapter.as_mut().context("Explorer adapter missing")?.rpc('
insert='''            // Resolve again after all verification, teardown and initialization waits.
            // A source refusal remains its original typed result; never submit/replay
            // a UI request against the previously resolved binding.
            let current = match resolve() {
                Ok(Ok(scope)) => scope,
                Ok(Err(reply)) => { drop(adapter.take()); return Ok(reply); }
                Err(error) => { drop(adapter.take()); return Err(error); }
            };
            let bound = adapter.as_ref().context("Explorer adapter missing")?;
            if current != (bound.project.clone(), bound.root.clone()) {
                drop(adapter.take());
                anyhow::bail!("Explorer source binding changed during setup");
            }
            let deadline = execution_deadline.unwrap_or_else(|| child_work_deadline(Instant::now()));
'''
assert needle in s;s=s.replace(needle,insert+needle)
# Separate pure settling logic makes ordering testable without a process or clock sleep.
start=s.index('            let reply =\n                match receive.recv_timeout')
end=s.index('            Ok((stdin, stdout, bytes))',start)
s=s[:start]+'''            let (reply, writer_first) =
                match receive.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(IoEvent::Reply(result)) => (result, false),
                    Ok(IoEvent::WriteFailed) => (Err(anyhow::anyhow!("Explorer input failed")), true),
                    Err(error) => (Err(anyhow::Error::new(error)
                        .context("Explorer reply timed out or disconnected")), false),
                };
            if reply.is_err() { drop(self.child.take()); }
            let written = writer.join().map_err(|_| anyhow::anyhow!("Explorer input owner panicked"));
            let read = reader.join().map_err(|_| anyhow::anyhow!("Explorer output owner panicked"));
            // Join both owners before returning. Teardown failures cannot replace
            // the first observed reply/timeout; an input event uses its real cause.
            let (stdin, written) = match written {
                Ok(value) => value,
                Err(error) => { return settle_exchange(reply, Err(error), writer_first).and_then(|_| unreachable!()); }
            };
            let bytes = settle_exchange(reply, written, writer_first)?;
            let stdout = read?;
''' +s[end:]
# helper primary keeps identity; secondary retained in custom wrapper
pos=s.index('#[derive(Default)]')
s=s[:pos]+'''fn child_work_deadline(started: Instant) -> Instant {
    started + Duration::from_secs(15)
}
#[derive(Debug)]
struct ExchangeFailure { primary: anyhow::Error, secondary: anyhow::Error }
impl std::fmt::Display for ExchangeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { std::fmt::Display::fmt(&self.primary, f) }
}
impl std::error::Error for ExchangeFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(self.primary.as_ref()) }
}
fn settle_exchange(reply: Result<Vec<u8>>, written: Result<()>, writer_first: bool) -> Result<Vec<u8>> {
    match (reply, written) {
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(_), Err(primary)) => Err(primary),
        (Err(reply), Err(written)) => {
            let (primary, secondary) = if writer_first { (written, reply) } else { (reply, written) };
            Err(anyhow::Error::new(ExchangeFailure { primary, secondary }))
        }
    }
}

'''+s[pos:]
p.write_text(s,encoding='utf-8',newline='\n')
