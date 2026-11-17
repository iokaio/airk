# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-store/src')
f=p/'service_data_scope.rs';s=f.read_text(encoding='utf-8').replace('pub struct BoundDataRoot(Scope);','''pub struct BoundDataRoot(Scope,LiveIntegrity);
#[derive(Clone,Debug)]
pub(crate) struct LiveIntegrity(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl LiveIntegrity {pub(crate) fn revoked(&self)->bool{self.0.load(std::sync::atomic::Ordering::Acquire)}}''')
s=s.replace('BoundDataRoot(scope)','BoundDataRoot(scope,LiveIntegrity(Default::default()))')
s=s.replace('impl BoundDataRoot {','''impl BoundDataRoot {
    pub(crate) fn live_integrity(&self)->LiveIntegrity{self.1.clone()}
    pub(crate) fn native_integrity_revoked(&self)->bool{self.1.revoked()}
    pub(crate) fn native_control_root(&self)->&Path{&self.0.control}
    pub(crate) fn revoke_native_integrity(&self){self.1.0.store(true,std::sync::atomic::Ordering::Release);}
''',1);f.write_text(s,encoding='utf-8',newline='\n')
f=p/'service_collection_integrity.rs';s=f.read_text(encoding='utf-8').replace('pub mod census;','pub mod census;\n#[path="service_collection_verify.rs"]\npub mod verify;',1)
s=s.replace('    pub known_damage: Vec<KnownDamage>,','    pub known_damage: Vec<KnownDamage>,\n    #[serde(default,skip_serializing_if="Option::is_none")]\n    pub verify:Option<verify::VerifyProgress>,',1)
needle='        let mut known_damage = self.load(namespace)?.map_or_else(Vec::new, |value| value.known_damage);'
if needle not in s:
 start=s.index('        let mut known_damage =');end=s.index('        if let Some(damage)',start)
 s=s[:start]+'''        let previous=self.load(namespace)?;
        let verify=previous.as_ref().and_then(|value|value.verify.clone());
        let mut known_damage=previous.map_or_else(Vec::new,|value|value.known_damage);
'''+s[end:]
else:s=s.replace(needle,'''        let previous=self.load(namespace)?;
        let verify=previous.as_ref().and_then(|value|value.verify.clone());
        let mut known_damage=previous.map_or_else(Vec::new,|value|value.known_damage);''',1)
s=s.replace('            privacy,\n        };','            privacy,\n            verify,\n        };',1)
start=s.index('    pub fn record_damage(');end=s.index('    fn publish(',start)
s=s[:start]+'''    pub fn record_damage(&self,read:&CurrentRead,component:Sha256Digest,observed_unix_ms:u64)->Result<IntegrityObservation>{
        self.bound.revoke_native_integrity();
        let control=self.registry.persist_integrity_damage(self.lease,read.head().scope.incarnation,component,observed_unix_ms);
        let local=self.publish(read,Some(KnownDamage{incarnation:read.head().scope.incarnation,component,observed_unix_ms}),PrivacyEligibility::PlatformEvidenceUnavailable);
        match(local,control){
            (Ok(value),Ok(()))=>Ok(value),
            (Err(error),Ok(()))=>Err(error.context("collection damage receipt failed; control veto was persisted")),
            (Ok(_),Err(error))=>Err(error.context("control veto failed; collection damage receipt was persisted")),
            (Err(error),Err(control))=>Err(error.context(format!("neither damage store confirmed persistence; control failure: {control:#}"))),
        }
    }
'''+s[end:];assert '            verify,' in s;f.write_text(s,encoding='utf-8',newline='\n')
f=p/'service_collection_tests.rs';s=f.read_text(encoding='utf-8')+Path('artifacts/optimizations/f10-f11-draft/verify-tests.rs').read_text(encoding='utf-8');f.write_text(s,encoding='utf-8',newline='\n')
