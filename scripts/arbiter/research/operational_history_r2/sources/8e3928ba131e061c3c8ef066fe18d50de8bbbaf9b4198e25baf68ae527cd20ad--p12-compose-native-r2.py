# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,shutil,hashlib,re
root=Path(r'D:/code/Github/airk');a=root/'artifacts/v190';old=a/'p12-native-composed-p11b-r1';out=a/'p12-native-composed-p11b-r2';out.mkdir(exist_ok=True)
for folder in ['base','tree']:shutil.copytree(old/folder,out/folder)
# Compose peer publication binding onto its EXACT reviewed native26 ancestor.
peer=a/'p12-publication-binding-proposed';m=json.loads((peer/'manifest-r3.json').read_text(encoding='utf-8'))
for rel,row in m.items():
 p=out/'tree'/rel;assert hashlib.sha256(p.read_bytes()).hexdigest()==row['base_sha256'],rel
 src=peer/'tree'/rel;assert hashlib.sha256(src.read_bytes()).hexdigest()==row['proposed_sha256'],rel
 p.write_bytes(src.read_bytes())
# Native Core identity golden is an additive test-only delta, exact existing base.
gold=a/'p12-ui-composed-p11b-r2/selection-identity-native-golden.patch';assert hashlib.sha256(gold.read_bytes()).hexdigest()=='ac5b1fc7ef9ce25abde6126576b73658835c9cc885033c4620a65f6bb6f7e439'
lines=gold.read_text(encoding='utf-8').splitlines(True);rel='crates/airk-core/src/catalog_refresh.rs';p=out/'tree'/rel;src=p.read_text(encoding='utf-8').splitlines(True);result=[];cursor=0;i=0
while i<len(lines):
 if not lines[i].startswith('@@ '):i+=1;continue
 h=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',lines[i]);start=max(0,int(h[1])-1);i+=1;body=[]
 while i<len(lines) and not lines[i].startswith(('@@ ','diff --git','--- ')):
  if lines[i].startswith((' ','+','-')):body.append(lines[i])
  elif lines[i].startswith('\\'):pass
  else:assert False,lines[i]
  i+=1
 expected=[v[1:] for v in body if v[0] in ' -'];new=[v[1:] for v in body if v[0] in ' +'];assert src[start:start+len(expected)]==expected,start;result.extend(src[cursor:start]);result.extend(new);cursor=start+len(expected)
result.extend(src[cursor:]);p.write_text(''.join(result),encoding='utf-8',newline='\n')
# Exact canonical registry-owned root; not a caller supplied runtime path.
rel='crates/airk-store/src/service_registry.rs';raw=(a/'p10-worktree'/rel).read_bytes();p=out/'base'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw);s=raw.decode('utf-8');pos=s.index('\n#[derive(serde::Serialize, serde::Deserialize)]',s.index('pub struct ServiceRegistry'))
s=s[:pos]+'''
impl ServiceRegistry {
    /// Canonical root of this already opened native registry. Metadata only;
    /// this path is not an authorization credential or a model provisioning request.
    pub fn control_root(&self) -> &Path { &self.root }
}
'''+s[pos:];p=out/'tree'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s,encoding='utf-8',newline='\n')
rel='crates/airk-cli/src/reference_refresh_control.rs';p=out/'tree'/rel;s=p.read_text(encoding='utf-8');needle='''    let kernel = airk_kernel::Kernel::open_existing_service_observer(&index.backing)?;''';assert needle in s;s=s.replace(needle,'''    // Match the ordinary native worker's read-only model context. This binds
    // immutable descriptors/runtime only: no provisioning, model weights,
    // inference, source scan or Current mutation is performed by qualification.
    let kernel = airk_kernel::Kernel::open_existing_service_observer(&index.backing)?
        .with_service_models(registry.control_root(), crate::bundled_assets::default_plan()?)?;''',1);p.write_text(s,encoding='utf-8',newline='\n')
# Known-control drift is distinct from I/O/permission/malformed/unknown state.
rel='crates/airk-core/src/catalog_refresh.rs';p=out/'tree'/rel;s=p.read_text(encoding='utf-8');s=s.replace('    ControlInput,','    ControlInput,\n    ControlInputsChanged,\n    ControlContextChanged,',1);s=s.replace('unique_bounded(&self.reasons, 9)','unique_bounded(&self.reasons, 11)').replace('unique_bounded(reasons, 9)','unique_bounded(reasons, 11)');p.write_text(s,encoding='utf-8',newline='\n')
rel='crates/airk-ingest/src/control_snapshot.rs';p=out/'tree'/rel;s=p.read_text(encoding='utf-8');pos=s.index('#[derive(Default)]');s=s[:pos]+'''/// A positively observed known-state inequality, never a classifier for I/O.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlDriftKind { Inputs, Context }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlDrift { pub kind: ControlDriftKind }
impl std::fmt::Display for ControlDrift {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self.kind { ControlDriftKind::Inputs => "control_inputs_changed", ControlDriftKind::Context => "control_context_changed" })
    }
}
impl std::error::Error for ControlDrift {}
fn changed(kind: ControlDriftKind) -> anyhow::Error { ControlDrift { kind }.into() }
/// Preserve uncertainty. Only actual NotFound is an absence observation.
fn require_absence(observed: std::io::Result<std::fs::Metadata>) -> Result<()> {
    match observed {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(changed(ControlDriftKind::Inputs)),
    }
}

'''+s[pos:]
replacements=[
('''                    let bytes = crate::read_protected_path(path, 64 << 10, false)?;''','''                    let bytes = match crate::read_protected_path(path, 64 << 10, false) {
                        Ok(bytes) => bytes,
                        Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) => {
                            if crate::source_read::missing_protected_path(path)? { return Err(changed(ControlDriftKind::Inputs)); }
                            return Err(error);
                        }
                        Err(error) => return Err(error),
                    };'''),
('''                    ensure!(
                        matches!(std::fs::symlink_metadata(path), Err(error) if error.kind()==std::io::ErrorKind::NotFound),
                        "control absence changed"
                    );''','''                    require_absence(std::fs::symlink_metadata(path))?;'''),
('''                    expected.inputs.get(path) == Some(&value),
                    "control input changed or was not in the authenticated baseline"''','''                    expected.inputs.get(path) == Some(&value),
                    ControlDrift { kind: ControlDriftKind::Inputs }'''),
('''ensure!(*prior == value, "control input changed within snapshot");''','''ensure!(*prior == value, ControlDrift { kind: ControlDriftKind::Inputs });'''),
('''                    expected.contexts.get(root) == Some(&digest),
                    "control locator/environment context changed"''','''                    expected.contexts.get(root) == Some(&digest),
                    ControlDrift { kind: ControlDriftKind::Context }'''),
('''ensure!(*prior == digest, "control context changed within scan");''','''ensure!(*prior == digest, ControlDrift { kind: ControlDriftKind::Context });'''),
('''                baseline == *expected,
                "control revalidation did not account for every saved input/context"''','''                baseline == *expected,
                ControlDrift { kind: ControlDriftKind::Inputs }''')]
for old,new in replacements:
 assert old in s,old;s=s.replace(old,new,1)
p.write_text(s,encoding='utf-8',newline='\n')
rel='crates/airk-kernel/src/service_reference_refresh.rs';p=out/'tree'/rel;s=p.read_text(encoding='utf-8');needle='''    pub fn refresh_service_references(
        &self,
        project: ProjectBinding,
        request: &ConfigurationRequest,
    ) -> Result<airk_ingest::IndexReport> {''';assert needle in s
s=s.replace(needle,needle+'''
        self.refresh_service_references_owned(project, request).map_err(control_drift_review)
    }
    fn refresh_service_references_owned(
        &self, project: ProjectBinding, request: &ConfigurationRequest,
    ) -> Result<airk_ingest::IndexReport> {''',1)
pos=s.index('\nimpl Kernel {');s=s[:pos]+'''
fn control_drift_review(error: anyhow::Error) -> anyhow::Error {
    match error.downcast_ref::<airk_ingest::control_snapshot::ControlDrift>() {
        Some(drift) => RefreshReviewRequired { reasons: vec![match drift.kind {
            airk_ingest::control_snapshot::ControlDriftKind::Inputs => RefreshFallbackReason::ControlInputsChanged,
            airk_ingest::control_snapshot::ControlDriftKind::Context => RefreshFallbackReason::ControlContextChanged,
        }] }.into(),
        None => error,
    }
}
'''+s[pos:];p.write_text(s,encoding='utf-8',newline='\n')
print('Native coherent r2 source authoring done; no formatter/CPU-heavy command during parent replays')
