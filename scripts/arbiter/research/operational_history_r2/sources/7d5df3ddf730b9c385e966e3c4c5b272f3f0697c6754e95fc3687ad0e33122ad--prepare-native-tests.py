# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/crates/airk-cli/src/service_frontend.rs')
s=p.read_text(encoding='utf-8')
s+='''
#[cfg(test)]
mod exact_scope_metadata_tests {
    use super::*;
    use airk_core::identity::Incarnation;
    use airk_store::service::registry::ServiceRegistry;
    fn observe(scope: &RootScope, control: &Path) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(scope.metadata_before(control, std::time::Instant::now()+std::time::Duration::from_secs(2))?)?)
    }
    #[test]
    fn actual_registry_scope_metadata_tracks_removed_reregistered_ambiguity_and_namespace() -> Result<()> {
        let owner=tempfile::tempdir()?;let base=owner.path().canonicalize()?;
        let source=base.join("source");let control=base.join("control");let data=base.join("data");
        for path in [&source,&control,&data] { std::fs::create_dir(path)?; }
        let scope=RootScope::new(&source,None)?;
        let absent=observe(&scope,&control)?;
        assert_eq!(absent["outcome"],"missing");assert!(absent["namespace"].is_null());
        assert!(!control.join("service-1.4.sqlite3").exists());
        let mut registry=ServiceRegistry::open(&control,Incarnation::from_bytes([81;16]))?;
        let first=ProjectRegistration::new(source.to_str().unwrap().into(),data.to_str().unwrap().into(),"first".into())?;
        registry.register(&first)?;
        let registered=observe(&scope,&control)?;
        assert_eq!(registered["outcome"],"registered");
        assert_eq!(registered["registration"],serde_json::to_value(&first)?);
        assert_eq!(registered["namespace"],serde_json::to_value(registry.incarnation()?)?);
        assert_eq!(registered["fencing_generation"],registry.get(first.binding)?.fencing_generation);
        std::fs::rename(&source,base.join("offline"))?;
        let epoch=registry.get(first.binding)?.fencing_generation;
        assert!(registry.retire_missing_source(first.binding,epoch,0)?);
        assert_eq!(observe(&scope,&control)?["outcome"],"removed");
        assert!(registry.purge_removed_project_turn(first.binding)?);
        assert_eq!(observe(&scope,&control)?["outcome"],"removed");
        registry.register(&first)?;
        assert_eq!(observe(&scope,&control)?["outcome"],"registered");
        let second=ProjectRegistration::new(first.root.clone(),first.data_root.clone(),"second".into())?;
        registry.register(&second)?;
        assert_eq!(observe(&scope,&control)?["outcome"],"ambiguous");
        let chosen=RootScope::new(&source,Some("second".into()))?;
        assert_eq!(observe(&chosen,&control)?["registration"],serde_json::to_value(&second)?);
        let old=observe(&chosen,&control)?;drop(registry);
        std::fs::rename(&control,base.join("old-control"))?;std::fs::create_dir(&control)?;
        let mut replacement=ServiceRegistry::open(&control,Incarnation::from_bytes([82;16]))?;
        replacement.register(&second)?;
        let new=observe(&chosen,&control)?;
        assert_ne!(old["namespace"],new["namespace"]);
        assert_eq!(new["registration"],serde_json::to_value(&second)?);
        assert!(chosen.metadata_before(&control,std::time::Instant::now()-std::time::Duration::from_secs(1)).is_err());
        Ok(())
    }
}
'''
p.write_text(s,encoding='utf-8',newline='\n')
