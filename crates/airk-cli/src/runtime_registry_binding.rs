// SPDX-License-Identifier: Apache-2.0
//! Independent state-level namespace pin, completed before accepting service work.
use super::*;
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use airk_store::service::{
    ServiceLease,
    registry::{ServiceRegistry, retained_namespace},
};

pub(super) const PIN: &str = "airk-registry-1.4.owner.json";
pub(super) const PART: &str = "airk-registry-1.4.owner.part";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema: u32,
    layout: Layout,
    ready: bool,
    namespace: Option<Incarnation>,
    checksum: Sha256Digest,
}
impl Binding {
    fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk/installed-registry-binding/1",
            &[&serde_json::to_vec(&(
                self.schema,
                &self.layout,
                self.ready,
                self.namespace,
            ))?],
        ))
    }
    fn new(layout: &Layout, namespace: Option<Incarnation>) -> Result<Self> {
        let mut value = Self {
            schema: 1,
            layout: layout.clone(),
            ready: namespace.is_some(),
            namespace,
            checksum: Sha256Digest::from_bytes([0; 32]),
        };
        value.checksum = value.digest()?;
        Ok(value)
    }
    fn validate(&self, layout: &Layout) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.layout == *layout
                && self.ready == self.namespace.is_some()
                && self.checksum == self.digest()?,
            "installed registry binding is foreign or damaged"
        );
        let control = layout.state.join("control");
        require_control(&control)?;
        if let Some(namespace) = self.namespace {
            ensure!(
                retained_namespace(&control)? == namespace,
                "installed registry namespace differs; restore the original metadata"
            );
        }
        Ok(())
    }
}
fn read(layout: &Layout) -> Result<Option<Binding>> {
    let path = layout.state.join(PIN);
    if !exists(&path)? {
        return Ok(None);
    }
    let value: Binding = serde_json::from_slice(&read_bounded(&path, MAX_LAYOUT)?)?;
    value.validate(layout)?;
    Ok(Some(value))
}
fn write(layout: &Layout, previous: Option<&Binding>, next: &Binding) -> Result<()> {
    let bytes = serde_json::to_vec(next)?;
    ensure!(
        bytes.len() as u64 <= MAX_LAYOUT,
        "installed registry binding exceeds budget"
    );
    let path = layout.state.join(PIN);
    let partial = layout.state.join(PART);
    let old = previous.map(serde_json::to_vec).transpose()?;
    match (&old, exists(&path)?) {
        (Some(old), true) => ensure!(
            read_bounded(&path, MAX_LAYOUT)? == *old,
            "installed registry binding changed"
        ),
        (None, false) => {}
        _ => bail!("installed registry binding changed"),
    }
    let mut options = OpenOptions::new();
    options.write(true);
    if exists(&partial)? {
        let prefix = read_bounded(&partial, MAX_LAYOUT)?;
        ensure!(
            bytes.starts_with(&prefix) || old.as_ref().is_some_and(|old| old.starts_with(&prefix)),
            "unknown registry binding partial preserved"
        );
        options.truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&partial)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    if previous.is_none() {
        publish_new(&partial, &path)?;
    } else {
        #[cfg(unix)]
        fs::rename(&partial, &path)?;
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let from = partial
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let to = path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            // SAFETY: terminated owned paths; replace the validated preceding receipt.
            ensure!(
                unsafe {
                    windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                        from.as_ptr(),
                        to.as_ptr(),
                        1 | 8,
                    )
                } != 0,
                "registry binding publication failed: {}",
                std::io::Error::last_os_error()
            );
        }
    }
    sync_directory(&layout.state)
}
pub(super) fn validate(layout: &Layout, completed: bool) -> Result<()> {
    if read(layout)?.is_none() && completed {
        // Earlier development receipts do not distinguish lost metadata from a
        // first crash before DB creation. Preserve that ambiguity; never reset it.
        let control = layout.state.join("control");
        ensure!(
            exists(&control.join("service-1.4.sqlite3"))?
                && exists(&control.join("service-1.4.owner.json"))?,
            "earlier initialized control metadata is missing; restore its original registry"
        );
    }
    Ok(())
}
pub(super) fn begin(lease: &ServiceLease, layout: &Layout, completed: bool) -> Result<()> {
    ensure!(
        lease.root() == layout.state.join("control"),
        "registry binding lease differs"
    );
    if read(layout)?.is_some() {
        return Ok(());
    }
    let control = lease.root();
    let existing = completed
        || exists(&control.join("service-1.4.sqlite3"))?
        || exists(&control.join("service-1.4.owner.json"))?;
    let namespace = if existing {
        let registry =
            ServiceRegistry::open(control, airk_kernel::service_broker::new_instance_id()?)?;
        Some(registry.incarnation()?)
    } else {
        None
    };
    write(layout, None, &Binding::new(layout, namespace)?)
}
pub(super) fn finish(lease: &ServiceLease, layout: &Layout, namespace: Incarnation) -> Result<()> {
    ensure!(
        lease.root() == layout.state.join("control")
            && retained_namespace(lease.root())? == namespace,
        "registry binding completion identity differs"
    );
    let previous = read(layout)?.context("registry binding initialization missing")?;
    if previous.ready {
        ensure!(
            previous.namespace == Some(namespace),
            "registry binding namespace differs"
        );
        return Ok(());
    }
    write(
        layout,
        Some(&previous),
        &Binding::new(layout, Some(namespace))?,
    )
}
