# SPDX-License-Identifier: Apache-2.0
"""Build private proposals only; never writes native/public source."""
from pathlib import Path
import hashlib,json,difflib
ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
rows=[]
def propose(target,text):
    source=ROOT/target
    before=source.read_text(encoding='utf-8') if source.exists() else ''
    output=HERE/'proposed'/target
    output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(text,encoding='utf-8',newline='\n')
    rows.append({'target':target,'base_sha256':hashlib.sha256(source.read_bytes()).hexdigest() if source.exists() else None,'proposal':str(output),'proposal_sha256':hashlib.sha256(output.read_bytes()).hexdigest()})
    (HERE/(target.replace('/','_')+'.diff')).write_text(''.join(difflib.unified_diff(before.splitlines(True),text.splitlines(True),fromfile=target,tofile=target)),encoding='utf-8')
base='crates/airk-service/src/service_runtime/'
p=base+'arbiter.rs';s=(ROOT/p).read_text(encoding='utf-8');s=s.replace('mod lifecycle;','mod lifecycle;\n#[cfg(all(feature = "local-inference", feature = "test-support"))]\npub(super) mod warm_observer;');propose(p,s)
p=base+'arbiter/lifecycle.rs';s=(ROOT/p).read_text(encoding='utf-8')
s=s.replace('    pending: Option<PreparedLaunch>,','    pending: Option<PreparedLaunch>,\n    #[cfg(feature = "test-support")]\n    warm_observer: Option<super::warm_observer::Observer>,',1)
s=s.replace('            pending: None,','            pending: None,\n            #[cfg(feature = "test-support")]\n            warm_observer: None,',1)
start=s.index('    let WarmContext {',s.index('async fn warm('));end=s.index('\n}\n\nfn cached_intent_eligible',start)
s=s[:start]+'''    #[cfg(feature = "test-support")]
    let mut observation = WarmCapture::capture(&context, profile.description().manifest);
    let result = async {
'''+s[start:end]+'''
    }.await;
    #[cfg(feature = "test-support")]
    if let Some(observation) = observation { observation.finish(&result); }
    result'''+s[end:]
s=s.replace('if handle.observation_for(stage)?.active_profile == Some(manifest)', 'let retained_observation = handle.observation_for(stage)?;\n        if retained_observation.active_profile == Some(manifest)',1)
s=s.replace('            return phase(\n', '            #[cfg(feature = "test-support")]\n            if let Some(observation) = observation.as_mut() {\n                observation.activation_path = super::warm_observer::ActivationPath::RetainedResident;\n                observation.native_profile = retained_observation.active_profile;\n                observation.native_revision = retained_observation.configuration_revision;\n            }\n            return phase(\n',1)
s=s.replace('    let receiver = handle.warm(profile, source, reservation)?;', '    let receiver = handle.warm(profile, source, reservation)?;\n    #[cfg(feature = "test-support")]\n    if let Some(observation) = observation.as_mut() { observation.activation_path = super::warm_observer::ActivationPath::NativeWarmAdmitted; }',1)
s=s.replace('let observed=handle.observation_for(stage)?;', 'let observed=handle.observation_for(stage)?;\n        #[cfg(feature = "test-support")]\n        if let Some(observation) = observation.as_mut() {\n            observation.native_profile=observed.active_profile;\n            observation.native_revision=observed.configuration_revision;\n        }',1)
s+='\n'+(HERE/'lifecycle-capture.rs.fragment').read_text(encoding='utf-8');propose(p,s)
p=base+'arbiter/evaluation.rs';s=(ROOT/p).read_text(encoding='utf-8');at=s.index('    pub fn configuration(')
s=s[:at]+'''    pub(in crate::service_runtime) fn observe_warm(self, observer: super::warm_observer::Observer) -> Result<Self> {
        super::lifecycle::observe_warm(&self.owner.focus.lifecycle, observer.clone())?;
        super::lifecycle::observe_warm(&self.owner.context.lifecycle, observer)?;
        Ok(self)
    }
'''+s[at:];propose(p,s)
p=base+'evaluation_fixture_tests.rs';s=(ROOT/p).read_text(encoding='utf-8')
s=s.replace('mod execution_observer;','mod execution_observer;\nmod warm_journal;',1)
s=s.replace('    health: airk_core::service::ServiceHealth,','    health: airk_core::service::ServiceHealth,\n    #[serde(default, skip_serializing_if = "Option::is_none")]\n    warm_observer_id: Option<airk_core::identity::Incarnation>,',1)
s=s.replace('    let source = prepared.clone();','    let mut warm_recorder = warm_journal::Recorder::from_env(&work)?;\n    let warm_observer = warm_recorder.as_ref().map(|recorder| recorder.observer());\n    let warm_observer_id = warm_recorder.as_ref().map(|recorder| recorder.id());\n    let source = prepared.clone();',1)
s=s.replace('        seeded\n','        if let Some(observer) = warm_observer { owner = owner.observe_warm(observer)?; }\n        seeded\n',1)
s=s.replace('            health: *observation,','            health: *observation,\n            warm_observer_id,',1)
s=s.replace('                let _ = recorder.finish().await;\n            }\n            return Err','                let _ = recorder.finish().await;\n            }\n            if let Some(recorder) = warm_recorder.take() { let _ = recorder.finish().await; }\n            return Err',1)
s=s.replace('        return Err(error.context("private fixture failed; no successful closure was claimed"));','        if let Some(recorder) = warm_recorder.take() { let _ = recorder.finish().await; }\n        return Err(error.context("private fixture failed; no successful closure was claimed"));',1)
s=s.replace('    // Actual lease reacquisition','    if let Some(recorder) = warm_recorder.take() { recorder.finish().await?; }\n    // Actual lease reacquisition',1);propose(p,s)
for name,target in [('warm_observer.rs',base+'arbiter/warm_observer.rs'),('warm_journal.rs',base+'evaluation_fixture_tests/warm_journal.rs'),('warm_observer_tests.rs',base+'arbiter/warm_observer/tests.rs')]:
    propose(target,(HERE/name).read_text(encoding='utf-8'))
p=base+'arbiter/CONTRACT.md';propose(p,(ROOT/p).read_text(encoding='utf-8')+'\n'+(HERE/'CONTRACT.append.md').read_text(encoding='utf-8'))
(HERE/'manifest.proposed.json').write_text(json.dumps({'schema':'airk.private.warm-journal-proposal/1','public_sources_mutated':False,'runtime_executed':False,'targets':rows},indent=2)+'\n',encoding='utf-8')
