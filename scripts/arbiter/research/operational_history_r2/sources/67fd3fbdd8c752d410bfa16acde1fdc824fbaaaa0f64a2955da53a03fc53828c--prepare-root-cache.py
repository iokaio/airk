# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib, subprocess, sys
base=Path(__file__).parent
root=Path.cwd()
subprocess.run([sys.executable,str(base/'prepare-frontend.py')],check=True)
changes={}
for name in ('service_registry.rs','service_scope.rs'):
    path='crates/airk-store/src/'+name
    old=(root/path).read_text(encoding='utf-8') if (root/path).exists() else ''
    new=(base/'source'/path).read_text(encoding='utf-8')
    if name=='service_registry.rs':
        new=new.replace('pub use scope::{SavedScopeObservation, SavedScopeResolution};',
            'pub use scope::{SavedScopeObservation, SavedScopeResolution, SavedScopeStamp, SavedScopeExpired, source_scope_retryable};')
        # Tombstone-only changes are visible to negative scope resolution.
        new=new.replace('        if newly_active && let Some(enabled) = setup_watch {',
            '        if removed && !newly_active { bump(&transaction)?; }\n        if newly_active && let Some(enabled) = setup_watch {',1)
        start=new.index('    pub fn open_saved(root: &Path) -> Result<Self> {')
        end=new.index('\n    /// Native adapter establishes',start)
        block=new[start:end]
        block=block.replace('    pub fn open_saved(root: &Path) -> Result<Self> {','''    pub fn open_saved(root: &Path) -> Result<Self> {
        Self::open_saved_before(root, None)
    }
    pub fn open_saved_before(root: &Path, deadline: Option<std::time::Instant>) -> Result<Self> {
        scope::scope_remaining(deadline)?;''',1)
        block=block.replace('connection.busy_timeout(Duration::from_secs(2))?', 'connection.busy_timeout(scope::scope_remaining(deadline)?)?')
        block=block.replace('        let application: i64 =', '        connection.busy_timeout(scope::scope_remaining(deadline)?)?;\n        let application: i64 =')
        block=block.replace('        let version: i64 =', '        connection.busy_timeout(scope::scope_remaining(deadline)?)?;\n        let version: i64 =')
        block=block.replace('        ensure!(\n            registry.incarnation()?', '        registry.connection.busy_timeout(scope::scope_remaining(deadline)?)?;\n        ensure!(\n            registry.incarnation()?')
        block=block.replace('        Ok(registry)', '        scope::scope_remaining(deadline)?;\n        Ok(registry)')
        new=new[:start]+block+new[end:]
    else:
        new=new.replace('''    ) -> Result<SavedScopeObservation> {
        let registration_state''','''    ) -> Result<SavedScopeObservation> {
        self.observe_source_scope_before(root, collection, None)
    }
    pub fn observe_source_scope_before(&mut self, root: &Path, collection: Option<&str>,
        deadline: Option<std::time::Instant>) -> Result<SavedScopeObservation> {
        self.connection.busy_timeout(scope_remaining(deadline)?)?;
        let registration_state''',1)
        new=new.replace('        for inactive in [false, true] {','        for inactive in [false, true] {\n            scope_remaining(deadline)?;',1)
        new=new.replace('            while let Some(row) = rows.next()? {','            while let Some(row) = rows.next()? {\n                scope_remaining(deadline)?;',1)
        production, tests = new.split('\n#[cfg(test)]',1)
        production=production.replace('                return Ok(SavedScopeObservation',
            '                scope_remaining(deadline)?;\n                return Ok(SavedScopeObservation')
        production=production.replace('        Ok(SavedScopeObservation {',
            '        scope_remaining(deadline)?;\n        Ok(SavedScopeObservation {')
        new=production+'\n#[cfg(test)]'+tests
        new+=base.joinpath('root-cache-stamp.rs').read_text(encoding='utf-8')
        # Place real-API regression inside existing private tests module.
        at=new.index('\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub struct SavedScopeStamp')
        before=new[:at]; after=new[at:]
        close=before.rfind('}')
        new=before[:close]+base.joinpath('root-cache-test.rs').read_text(encoding='utf-8')+'\n'+before[close:]+after
    changes[path]=(old,new)
for name in ('service_frontend.rs','unified_mcp.rs','project_client.rs'):
    path='crates/airk-cli/src/'+name
    old=(root/path).read_text(encoding='utf-8')
    new=(base/'frontend-source'/path).read_text(encoding='utf-8')
    if name=='service_frontend.rs':
        start=new.index('pub(crate) struct RootObservation {');end=new.index('impl RootScope {',start)
        new=new[:start]+base.joinpath('root-cache-observation.rs').read_text(encoding='utf-8')+new[end:]
        start=new.index('    pub fn resolve(&self, control: &Path)');end=new.index('\n}\n\n/// Resolve only installation',start)
        new=new[:start]+base.joinpath('root-cache-observe.rs').read_text(encoding='utf-8')+new[end:]
        new+=base.joinpath('root-cache-frontend-tests.rs').read_text(encoding='utf-8')
    elif name=='unified_mcp.rs':
        new=new.replace('#[derive(Default)]\nstruct SourceBackendCache {',
            base.joinpath('root-lookup-owner.rs').read_text(encoding='utf-8')+'\n#[derive(Default)]\nstruct SourceBackendCache {',1)
        new=new.replace('    sequence: std::sync::atomic::AtomicU64,','    sequence: std::sync::atomic::AtomicU64,\n    observation: std::sync::Mutex<Option<Arc<crate::service_frontend::RootObservation>>>,',1)
        new=new.replace('    observation: std::sync::Mutex<Option<Arc<crate::service_frontend::RootObservation>>>,',
            '    observation: std::sync::Mutex<Option<Arc<crate::service_frontend::RootObservation>>>,\n    lookup: Arc<SourceLookupOwner>,',1)
        new=new.replace('            let observed = control.clone();\n            let observed = tokio::task::spawn_blocking(move || scope.observe(&observed)).await??;', '''            let observed_control = control.clone();
            let admission = Instant::now() + Duration::from_secs(2);
            let candidate = self.observation.lock().unwrap_or_else(|error| error.into_inner()).clone();
            let previous = candidate.clone();
            let observed = match self.lookup.run(admission, move || scope.observe_cached(&observed_control, candidate, admission)).await {
                Ok(observed) => observed,
                Err(error) if airk_store::service::registry::source_scope_retryable(&error)
                    => return Ok(crate::project_client::source_scope_superseded()),
                Err(error) => return Err(error),
            };''',1)
        new=new.replace('            let mut cached = self.backend.lock().await;',
            '            let source_cached = previous.as_ref().is_some_and(|old| Arc::ptr_eq(old, &observed));\n            let mut cached = self.backend.lock().await;',1)
        new=new.replace('"registration_state":state,"source_root":self.scope.root,',
            '"registration_state":state,"source_root":self.scope.root,"observed_unix_ms":observed.observed_unix_ms,"cache_age_ms":observed.age_ms(),"cached":source_cached,',1)
        new=new.replace('            if !cached.admit_sequence(sequence) {','            if Instant::now() >= admission || !observed.fresh_at(Instant::now()) || !cached.admit_sequence(sequence) {',1)
        new=new.replace('            let registration = match observed.resolution {','            *self.observation.lock().unwrap_or_else(|error| error.into_inner()) = Some(observed.clone());\n            let registration = match &observed.resolution {',1)
        new=new.replace('let (code, state, message) = root_failure(other);','let (code, state, message) = root_failure(match other {\n                        crate::service_frontend::RootResolution::Missing => crate::service_frontend::RootResolution::Missing,\n                        crate::service_frontend::RootResolution::Removed => crate::service_frontend::RootResolution::Removed,\n                        crate::service_frontend::RootResolution::Ambiguous => crate::service_frontend::RootResolution::Ambiguous,\n                        _ => unreachable!(),\n                    });',1)
        new=new.replace('sequence: std::sync::atomic::AtomicU64::new(0),','sequence: std::sync::atomic::AtomicU64::new(0),\n                observation: std::sync::Mutex::new(None),',1)
        new=new.replace('backend.call_observed(tool, arguments, fence).await',
            'backend.call_observed_until(tool, arguments, fence, observed.expires_at()).await',1)
        start=new.index('pub async fn serve_roots(')
        before=new[:start]; serving=new[start:]
        serving=serving.replace('    let projects = scopes', '    let mut lookups=Vec::with_capacity(scopes.len());\n    let projects = scopes',1)
        serving=serving.replace('        .map(|scope| Project {','        .map(|scope| {\n            let lookup=Arc::new(SourceLookupOwner::new());\n            lookups.push(lookup.clone());\n            Project {',1)
        serving=serving.replace('                observation: std::sync::Mutex::new(None),','                observation: std::sync::Mutex::new(None),\n                lookup,',1)
        serving=serving.replace('        })\n        .collect();','        }})\n        .collect();',1)
        serving=serving.replace('    let close = ready.clone();','    let close = ready.clone();\n    let lookup_close=lookups.clone();',1)
        serving=serving.replace('Arc::new(move || close.close_admission()),',
            'Arc::new(move || { close.close_admission(); for lookup in &lookup_close { lookup.close(); } }),',1)
        serving=serving.replace('    let cleanup = ready.shutdown().await;', '''    let mut cleanup = ready.shutdown().await;
    for lookup in &lookups {
        if let Err(error)=lookup.shutdown().await {
            cleanup=Err(match cleanup { Ok(())=>error, Err(primary)=>primary.context(format!("source lookup cleanup also failed: {error:#}")) });
        }
    }''',1)
        new=before+serving
        new+=base.joinpath('root-lookup-tests.rs').read_text(encoding='utf-8')
    elif name=='project_client.rs':
        new=new.replace('self.call_bound(tool, arguments, None)', 'self.call_bound(tool, arguments, None, None)',1)
        new=new.replace('self.call_bound(tool, arguments, Some(fence))', 'self.call_bound(tool, arguments, Some(fence), None)',1)
        at=new.index('    fn call_bound(',new.index('impl Backend {'))
        new=new[:at]+'''    pub(crate) fn call_observed_until(&self, tool: String,
        arguments: serde_json::Map<String,serde_json::Value>, fence: SourceFence,
        expires: Instant) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, Some(fence), Some(expires))
    }
'''+new[at:]
        new=new.replace('        observed: Option<SourceFence>,','        observed: Option<SourceFence>,\n        source_expires: Option<Instant>,',1)
        new=new.replace('            let mut state = self.state.lock().await;',
            '            let mut state = self.state.lock().await;\n            if source_expires.is_some_and(|until| Instant::now() >= until) { return Ok(source_scope_superseded()); }',1)
        new=new.replace('            let is_status = tool == "airk_status";',
            '            if source_expires.is_some_and(|until| Instant::now() >= until) { return Ok(source_scope_superseded()); }\n            let is_status = tool == "airk_status";',1)
        new+=base.joinpath('root-cache-backend-tests.rs').read_text(encoding='utf-8')
    changes[path]=(old,new)
patch=[]
for path,(old,new) in changes.items():
    target=base/'root-cache-source'/path;target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text(new,encoding='utf-8',newline='\n')
    patch.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+path if old else '/dev/null',tofile='b/'+path))
base.joinpath('root-cache.patch').write_text(''.join(patch),encoding='utf-8',newline='\n')
