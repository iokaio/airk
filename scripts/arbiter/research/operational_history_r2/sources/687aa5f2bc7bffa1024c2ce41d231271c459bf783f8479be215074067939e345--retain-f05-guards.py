# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f06-draft/rebased-f05/source/crates/airk-cli/src/project_client.rs');s=p.read_text()
s=s.replace('if state.connection.as_ref().is_none_or(|connection|!connection.reusable(Instant::now())){','''if state.connection.as_ref().is_none_or(|connection| observed.map_or_else(
                || !connection.reusable(Instant::now()),
                |fence| !connection.reusable_for_source(fence, Instant::now()))){''')
s=s.replace('''                let connection=match connected{''','''                // F05: a newer accepted observation while brokerage awaited means
                // this old request is still unsubmitted and must not retain the channel.
                if source_expires.is_some_and(|until|Instant::now()>=until)
                    || observed.is_some_and(|fence|!self.source_fence_current(CacheScope::Source(fence))){
                    return Ok(source_scope_superseded());
                }
                let connection=match connected{''',1)
s=s.replace('''            if source_expires.is_some_and(|until|Instant::now()>=until){return Ok(source_scope_superseded());}
            let connection=state.connection''','''            if source_expires.is_some_and(|until|Instant::now()>=until)
                || observed.is_some_and(|fence|!self.source_fence_current(CacheScope::Source(fence))){return Ok(source_scope_superseded());}
            let connection=state.connection''',1)
p.write_text(s,newline='\n')
