// SPDX-License-Identifier: Apache-2.0
//! Installation renames the historical default index root after owned processes drain.
use super::*;
use airk_store::service::{ServiceLease, registry::data_relocation};

pub(super) fn run(root: &Path) -> Result<()> {
    let (_, channel) = layout::retained_parameters(root)?;
    let Some(paths) = default_paths(root, channel)? else {
        return Ok(());
    };
    run_for(root, channel, &paths)
}

fn default_paths(
    root: &Path,
    channel: RuntimeChannel,
) -> Result<Option<crate::user_paths::UserPaths>> {
    let mut paths = crate::user_paths::UserPaths::historical(channel)?;
    let current = paths.install.with_file_name(match channel {
        RuntimeChannel::Stable => "runtime",
        RuntimeChannel::Preview => "runtime-preview",
    });
    let root = root.canonicalize()?;
    if root != crate::user_paths::canonical_location(&paths.install)?
        && root != crate::user_paths::canonical_location(&current)?
    {
        return Ok(None);
    }
    paths.install = root;
    Ok(Some(paths))
}

pub(super) fn needed(root: &Path) -> Result<bool> {
    let (_, channel) = layout::retained_parameters(root)?;
    let Some(paths) = default_paths(root, channel)? else {
        return Ok(false);
    };
    let control = layout::control(root)?;
    if !control.join("service-1.4.sqlite3").try_exists()? {
        return Ok(false);
    }
    if data_relocation::require_settled(&control).is_err() {
        return Ok(true);
    }
    let source = paths.data.with_file_name(match channel {
        RuntimeChannel::Stable => "service-1.4",
        RuntimeChannel::Preview => "service-1.4-preview",
    });
    data_relocation::needed(&control, &crate::user_paths::canonical_location(&source)?)
}

fn run_for(
    root: &Path,
    channel: RuntimeChannel,
    paths: &crate::user_paths::UserPaths,
) -> Result<()> {
    // Custom installation/storage locations are not inferred from a leaf name.
    if crate::user_paths::canonical_location(&paths.install)? != root.canonicalize()? {
        return Ok(());
    }
    let leaf = match channel {
        RuntimeChannel::Stable => "service-1.4",
        RuntimeChannel::Preview => "service-1.4-preview",
    };
    let source = paths.data.with_file_name(leaf);
    let destination = paths.data.with_file_name(match channel {
        RuntimeChannel::Stable => "index-v1",
        RuntimeChannel::Preview => "index-v1-preview",
    });
    let control = layout::control(root)?;
    if !control.join("service-1.4.sqlite3").try_exists()? {
        return Ok(());
    }
    if data_relocation::require_settled(&control).is_ok()
        && data_relocation::selected(
            &control,
            &crate::user_paths::canonical_location(&source)?,
            &crate::user_paths::canonical_location(&destination)?,
        )?
    {
        return Ok(());
    }
    if data_relocation::require_settled(&control).is_ok()
        && !data_relocation::needed(&control, &crate::user_paths::canonical_location(&source)?)?
    {
        return Ok(());
    }
    let source = crate::user_paths::canonical_location(&source)?;
    let destination = crate::user_paths::canonical_location(&destination)?;
    let service = ServiceLease::acquire(&control)?;
    data_relocation::relocate(&service, &source, &destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_renames_existing_default_and_discovery_uses_its_receipt() -> Result<()> {
        use airk_core::{identity::Incarnation, service::ProjectRegistration};
        use airk_store::service::{data_scope, registry::ServiceRegistry};
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let root = base.join("runtime/service-1.4");
        let state = base.join("state/service-1.4");
        local_ipc::prepare_root(&root)?;
        layout::initialize(&root, Some(&state), RuntimeChannel::Stable)?;
        let control = layout::control(&root)?;
        local_ipc::prepare_root(&control)?;
        let lease = ServiceLease::acquire(&control)?;
        layout::record_control(&lease)?;
        let namespace = Incarnation::from_bytes([1; 16]);
        let mut registry = ServiceRegistry::open(&control, namespace)?;
        layout::record_registry(&lease, namespace)?;
        let data = base.join("data/service-1.4");
        let source = base.join("source");
        fs::create_dir_all(&data)?;
        fs::create_dir(&source)?;
        let registration = ProjectRegistration::new(
            source.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "fixture".into(),
        )?;
        registry.register(&registration)?;
        data_scope::bind(&data, &control, namespace)?;
        drop((registry, lease));
        let mut paths = crate::user_paths::UserPaths {
            channel: RuntimeChannel::Stable,
            legacy_locations: true,
            install: root.clone(),
            data: data.clone(),
            state,
            launcher: base.join("launcher"),
            configuration: base.join("configuration"),
            cache: base.join("cache"),
            logs: base.join("logs"),
        };
        run_for(&root, RuntimeChannel::Stable, &paths)?;
        paths.select_migrated_data()?;
        assert_eq!(paths.data, base.join("data/index-v1"));
        assert!(!data.exists());
        run_for(&root, RuntimeChannel::Stable, &paths)?;
        assert!(paths.data.exists());
        Ok(())
    }
}
