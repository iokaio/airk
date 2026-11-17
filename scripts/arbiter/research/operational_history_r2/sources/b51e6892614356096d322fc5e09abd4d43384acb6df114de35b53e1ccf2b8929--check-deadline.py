# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/crates/airk-cli/src/service_frontend.rs')
s=p.read_text(encoding='utf-8');needle='        let observed = self.observe_uncached(control, Some(deadline))?;'
assert needle in s;s=s.replace(needle,'''        if std::time::Instant::now() >= deadline {
            return Err(airk_store::service::registry::SavedScopeExpired.into());
        }
        let observed = self.observe_uncached(control, Some(deadline))?;
        if std::time::Instant::now() >= deadline {
            return Err(airk_store::service::registry::SavedScopeExpired.into());
        }''',1)
p.write_text(s,encoding='utf-8',newline='\n')
