# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path('artifacts/optimizations/f09-draft/exact-scope')
files={
'crates/airk-cli/src/runtime_install.rs':Path('crates/airk-cli/src/runtime_install.rs'),
'crates/airk-cli/src/runtime_install_commands.rs':Path('artifacts/optimizations/f09-draft/bootstrap-source/crates/airk-cli/src/runtime_install_commands.rs'),
'crates/airk-cli/src/service_frontend.rs':Path('crates/airk-cli/src/service_frontend.rs'),
'packages/vscode/src/serviceBootstrap.ts':Path('artifacts/optimizations/f09-draft/bootstrap-source/packages/vscode/src/serviceBootstrap.ts'),
'packages/vscode/src/serviceWorkspace.ts':Path('artifacts/optimizations/f09-draft/source/packages/vscode/src/serviceWorkspace.ts'),
'packages/vscode/src/serviceExtension.ts':Path('artifacts/optimizations/f09-draft/source/packages/vscode/src/serviceExtension.ts')}
record={}
for name,f in files.items():
 data=f.read_bytes();record[name]={'sha256':hashlib.sha256(data).hexdigest(),'layer':f.as_posix()}
 for layer in ['base','source']:
  target=p/layer/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
(p/'BASE.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
# native port is deliberately frontend metadata, no project grant/service call
f=p/'source/crates/airk-cli/src/service_frontend.rs';s=f.read_text(encoding='utf-8')
pos=s.index('/// Resolve only installation/control metadata;')
s=s[:pos]+'''/// Exact saved-scope metadata. Not a connection grant or execution proof.
#[derive(serde::Serialize)]
pub(crate) struct SourceScopeMetadata {
    schema: u32,
    root: PathBuf,
    collection: Option<String>,
    namespace: Option<airk_core::identity::Incarnation>,
    revision: Option<u64>,
    observed_unix_ms: Option<u64>,
    outcome: &'static str,
    registration: Option<ProjectRegistration>,
    fencing_generation: Option<u64>,
}
impl RootScope {
    pub(crate) fn metadata_before(&self, control: &Path, deadline: std::time::Instant) -> Result<SourceScopeMetadata> {
        let observed = self.observe_uncached(control, Some(deadline))?;
        let (outcome, registration) = match observed.resolution {
            RootResolution::Registered(project) => ("registered", Some(project)),
            RootResolution::Missing => ("missing", None),
            RootResolution::Removed => ("removed", None),
            RootResolution::Ambiguous => ("ambiguous", None),
        };
        Ok(SourceScopeMetadata {
            schema: 1, root: self.root.clone(), collection: self.collection.clone(),
            namespace: observed.stamp.map(|stamp| stamp.namespace),
            revision: observed.stamp.map(|stamp| stamp.revision),
            observed_unix_ms: observed.observed_unix_ms, outcome, registration,
            fencing_generation: observed.fence.map(|fence| fence.fencing_generation),
        })
    }
}

'''+s[pos:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-cli/src/runtime_install.rs';s=f.read_text(encoding='utf-8');needle='pub enum RuntimeCommand {'
s=s.replace(needle,needle+'''
    /// Read one exact saved source scope, without electing a service or opening an index.
    SourceScope {
        #[arg(long)] install_dir: PathBuf,
        #[arg(long)] expected_control: PathBuf,
        #[arg(long)] expected_runtime: String,
        #[arg(long)] root: PathBuf,
        #[arg(long)] collection: Option<String>,
    },
''',1);f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-cli/src/runtime_install_commands.rs';s=f.read_text(encoding='utf-8');needle='    let output = match command {'
s=s.replace(needle,needle+'''
        RuntimeCommand::SourceScope { install_dir, expected_control, expected_runtime, root, collection } => {
            // The lock binds selection/policy/control metadata throughout the one
            // registry transaction. No payload proof, lifecycle settlement or start.
            let scope = crate::service_frontend::RootScope::new(root, collection.clone())?;
            let _lock = install_lock(install_dir)?;
            let approval = match managed::read(install_dir)? {
                Some(policy) => policy.metadata_approval()?,
                None => selected_preflight(install_dir)?.approval,
            };
            ensure!(approval.version == env!("CARGO_PKG_VERSION") && approval.manifest_sha256 == *expected_runtime,
                "source scope runtime changed");
            let control = layout::control(install_dir)?;
            ensure!(control == *expected_control, "source scope control changed");
            let observed = scope.metadata_before(&control, std::time::Instant::now() + std::time::Duration::from_secs(2))?;
            serde_json::json!({"install_dir":install_dir,"control_dir":control,"runtime_sha256":approval.manifest_sha256,"scope":observed})
        },
''',1);f.write_text(s,encoding='utf-8',newline='\n')
