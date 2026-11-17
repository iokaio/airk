# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
base=Path(__file__).parent;root=Path.cwd();patch=[]
for name in ['service_frontend.rs','unified_mcp.rs','project_client.rs']:
 path='crates/airk-cli/src/'+name;old=(root/path).read_bytes().decode();ending='\r\n' if '\r\n' in old else '\n';new=old.replace('\r\n','\n')
 if name=='service_frontend.rs':
  new=new.replace('impl RootScope {',base.joinpath('root-observation.rs').read_text()+'impl RootScope {',1)
  start=new.index('    pub fn resolve(&self, control: &Path)')
  end=new.index('\n}\n\n/// Resolve only installation',start)
  new=new[:start]+base.joinpath('observe-scope.rs').read_text()+new[end:]
 elif name=='unified_mcp.rs':
  start=new.index('struct RootNative {');end=new.index('fn root_failure(',start)
  new=new[:start]+base.joinpath('root-native.rs').read_text()+new[end:]
  new=new.replace('backend: tokio::sync::Mutex::new(None),','sequence: std::sync::atomic::AtomicU64::new(0),\n                backend: tokio::sync::Mutex::new(SourceBackendCache::default()),')
 else:
  new=new.replace('struct Backend {','pub(crate) struct Backend {',1)
  new=new.replace('    state: tokio::sync::Mutex<ClientState>,','    state: tokio::sync::Mutex<ClientState>,\n    latest_source: std::sync::Mutex<Option<SourceFence>>,',1)
  new=new.replace('struct CachedStatus {',base.joinpath('cache-scope.rs').read_text()+'struct CachedStatus {',1)
  new=new.replace('    scope: airk_core::identity::Incarnation,','    scope: CacheScope,',1)
  new=new.replace('fn new(scope: airk_core::identity::Incarnation, result:', 'fn new(scope: impl Into<CacheScope>, result:')
  new=new.replace('            scope,\n            captured:', '            scope: scope.into(),\n            captured:',1)
  new=new.replace('        scope: airk_core::identity::Incarnation,\n        now:', '        scope: impl Into<CacheScope>,\n        now:',1)
  new=new.replace('        if scope != self.scope || age >=', '        if scope.into() != self.scope || age >=',1)
  new=new.replace(') -> Result<Option<airk_core::identity::Incarnation>> {', ') -> Result<Option<CacheScope>> {',1)
  new=new.replace('Ok(Some(registry.incarnation()?))','Ok(Some(CacheScope::Pinned(registry.incarnation()?)))',1)
  impl_start=new.index('impl airk_mcp::remote::RemoteTools for Backend {')
  call_start=new.index('    fn call(',impl_start)
  impl_end=new.index('\n}\n\n/// One direct client session',call_start)
  call=new[call_start:impl_end]
  delegate="""    fn call(&self, tool: String, arguments: serde_json::Map<String, serde_json::Value>)
        -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, None)
    }"""
  common=call.replace('    fn call(', '    fn call_bound(',1).replace('        arguments: serde_json::Map<String, serde_json::Value>,','        arguments: serde_json::Map<String, serde_json::Value>,\n        observed: Option<SourceFence>,',1)
  common=common.replace('            let scope = if tool == "airk_status" {', '''            if let Some(fence) = observed {
                if !self.accept_source_fence(fence) { return Ok(source_scope_superseded()); }
            }
            let scope = if tool == "airk_status" {
                if let Some(fence) = observed { Some(CacheScope::Source(fence)) } else {''',1)
  common=common.replace('status_scope(&root, project)).await??\n            } else', 'status_scope(&root, project)).await??\n                }\n            } else',1)
  common=common.replace('            let mut state = self.state.lock().await;', '''            let mut state = self.state.lock().await;
            if observed.is_some_and(|fence| !self.source_fence_current(CacheScope::Source(fence))) {
                return Ok(source_scope_superseded());
            }''',1)
  common=common.replace('            if is_status {\n                state.cached_status', '            if is_status {\n                state.cached_status',1)
  common=common.replace('state.cached_status = scope.and_then(|scope| {', '''state.cached_status = scope.filter(|scope| self.source_fence_current(*scope)).and_then(|scope| {''',1)
  new=new[:call_start]+delegate+'\n}\n\n'+base.joinpath('backend-methods.rs').read_text()+common+'\n'+new[impl_end:]
  new=new.replace('pub fn tools(root: PathBuf, project: ProjectBinding) -> Arc<dyn airk_mcp::remote::RemoteTools> {','''pub fn tools(root: PathBuf, project: ProjectBinding) -> Arc<dyn airk_mcp::remote::RemoteTools> {
    source_tools(root, project)
}
pub(crate) fn source_tools(root: PathBuf, project: ProjectBinding) -> Arc<Backend> {''',1)
  new=new.replace('        state: tokio::sync::Mutex::new(Default::default()),','        state: tokio::sync::Mutex::new(Default::default()),\n        latest_source: std::sync::Mutex::new(None),',1)
 if name=='project_client.rs': new+='\n'+base.joinpath('frontend-tests.rs').read_text()
 if name=='unified_mcp.rs': new+='\n'+base.joinpath('root-tests.rs').read_text()
 new=new.replace('\n',ending);out=base/'frontend-source'/path;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(new.encode())
 patch.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+path,tofile='b/'+path))
base.joinpath('frontend-scope.patch').write_bytes(''.join(patch).encode())
