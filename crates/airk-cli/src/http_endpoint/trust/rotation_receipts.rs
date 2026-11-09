// SPDX-License-Identifier: Apache-2.0
//! Exact trust receipt transfer for the explicit stopped-owner issuer rotation.
use super::*;
use std::fs::File;

const PREIMAGE: &str = "rotation-user-trust-preimage.json";
const PREIMAGE_LIMIT: usize = 64 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preimage {
    schema: u32,
    binding: InstallationBinding,
    ca_sha256: Sha256Digest,
    receipt: Option<String>,
}
fn receipt_lock(root: &Path) -> Result<File> {
    if read_private(root, "user-trust-1.lock", 0)?.is_none()
        && let Err(error) = write_private(root, "user-trust-1.lock", b"", 0, false)
        && read_private(root, "user-trust-1.lock", 0)?.is_none()
    {
        return Err(error);
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("user-trust-1.lock"))?;
    lock.try_lock()
        .context("user trust setup is already active")?;
    Ok(lock)
}
fn checked_receipt(
    bytes: &[u8],
    binding: &InstallationBinding,
    ca: Sha256Digest,
    host: Host,
) -> Result<Receipt> {
    let receipt: Receipt = serde_json::from_slice(bytes)?;
    let store = match host {
        Host::Windows => PathBuf::from("CurrentUser/Root"),
        Host::Macos => crate::user_paths::home()?.join("Library/Keychains/login.keychain-db"),
        Host::Unsupported => bail!("owned user trust receipt is unsupported on this platform"),
    };
    ensure!(
        receipt.schema == 1
            && receipt.binding == *binding
            && receipt.ca_sha256 == ca
            && receipt.store == store,
        "changed rotation trust ownership receipt"
    );
    ensure!(
        matches!(receipt.phase, Phase::Installed | Phase::Removed),
        "user trust outcome unknown; preserve rotation receipt for review"
    );
    Ok(receipt)
}
fn immutable(root: &Path, name: &str, bytes: &[u8], limit: usize) -> Result<()> {
    if let Some(saved) = read_private(root, name, limit)? {
        ensure!(saved == bytes, "rotation trust preimage changed");
        return Ok(());
    }
    write_private(root, name, bytes, limit, false)
}
fn preimage(
    old: &Path,
    binding: &InstallationBinding,
    ca: Sha256Digest,
    host: Host,
) -> Result<Preimage> {
    public_ca(old, ca)?;
    let bytes = read_private(old, PREIMAGE, PREIMAGE_LIMIT)?
        .context("rotation trust preimage is unavailable")?;
    let saved: Preimage = serde_json::from_slice(&bytes)?;
    ensure!(
        saved.schema == 1 && saved.binding == *binding && saved.ca_sha256 == ca,
        "rotation trust preimage identity changed"
    );
    if let Some(receipt) = &saved.receipt {
        checked_receipt(receipt.as_bytes(), binding, ca, host)?;
    }
    Ok(saved)
}

fn archived_receipt(old: &Path, saved: &Preimage, host: Host) -> Result<Option<Vec<u8>>> {
    let _lock = receipt_lock(old)?;
    let current = read_private(old, RECEIPT, LIMIT)?;
    match (&saved.receipt, current) {
        (None, None) => Ok(None),
        (None, Some(_)) => bail!("unowned archived issuer gained a trust receipt"),
        (Some(original), None) => {
            // Complete a crash between immutable preimage publication and its
            // mutable archive copy. Never recapture ownership from active state.
            write_private(old, RECEIPT, original.as_bytes(), LIMIT, false)?;
            Ok(Some(original.as_bytes().to_vec()))
        }
        (Some(original), Some(current)) => {
            let receipt = checked_receipt(&current, &saved.binding, saved.ca_sha256, host)?;
            ensure!(
                current == original.as_bytes() || receipt.phase == Phase::Removed,
                "archived owned trust receipt changed after prepare"
            );
            Ok(Some(current))
        }
    }
}

pub(crate) fn snapshot_rotation_receipt(
    active: &Path,
    old: &Path,
    binding: InstallationBinding,
    ca: Sha256Digest,
) -> Result<()> {
    snapshot_with(active, old, binding, ca, Host::current())
}
fn snapshot_with(
    active: &Path,
    old: &Path,
    binding: InstallationBinding,
    ca: Sha256Digest,
    host: Host,
) -> Result<()> {
    binding.validate()?;
    public_ca(active, ca)?;
    public_ca(old, ca)?;
    let _lock = receipt_lock(active)?;
    let current = read_private(active, RECEIPT, LIMIT)?;
    if let Some(bytes) = &current {
        checked_receipt(bytes, &binding, ca, host)?;
    }
    let candidate = Preimage {
        schema: 1,
        binding: binding.clone(),
        ca_sha256: ca,
        receipt: current
            .as_ref()
            .map(|bytes| String::from_utf8(bytes.clone()))
            .transpose()?,
    };
    // Persist absence as well as presence before copying the mutable archived
    // receipt. A resumed prepare can never adopt a changed active old receipt.
    immutable(
        old,
        PREIMAGE,
        &serde_json::to_vec(&candidate)?,
        PREIMAGE_LIMIT,
    )?;
    let saved = preimage(old, &binding, ca, host)?;
    ensure!(
        saved.receipt.as_deref().map(str::as_bytes) == current.as_deref(),
        "active trust receipt changed since rotation prepare"
    );
    if let Some(bytes) = &current {
        immutable(old, RECEIPT, bytes, LIMIT)?;
    } else {
        ensure!(
            read_private(old, RECEIPT, LIMIT)?.is_none(),
            "unowned archived issuer gained a trust receipt"
        );
    }
    Ok(())
}

pub(crate) fn commit_rotation_receipt(
    active: &Path,
    old: &Path,
    new: &Path,
    binding: InstallationBinding,
    old_ca: Sha256Digest,
    new_ca: Sha256Digest,
) -> Result<()> {
    commit_with(active, old, new, binding, old_ca, new_ca, Host::current())
}
fn commit_with(
    active: &Path,
    old: &Path,
    new: &Path,
    binding: InstallationBinding,
    old_ca: Sha256Digest,
    new_ca: Sha256Digest,
    host: Host,
) -> Result<()> {
    binding.validate()?;
    ensure!(old_ca != new_ca, "rotation issuer did not change");
    let saved = preimage(old, &binding, old_ca, host)?;
    public_ca(new, new_ca)?;
    public_ca(active, new_ca)?;
    let replacement = read_private(new, RECEIPT, LIMIT)?;
    if let Some(bytes) = &replacement {
        let receipt = checked_receipt(bytes, &binding, new_ca, host)?;
        ensure!(
            receipt.phase == Phase::Installed,
            "staged new issuer has no installed owned trust"
        );
    }
    archived_receipt(old, &saved, host)?;
    let _lock = receipt_lock(active)?;
    let current = read_private(active, RECEIPT, LIMIT)?;
    if current == replacement {
        return Ok(());
    }
    ensure!(
        current.as_deref() == saved.receipt.as_deref().map(str::as_bytes),
        "active trust receipt differs from prepared old or committed new image"
    );
    match replacement {
        Some(bytes) => write_private(active, RECEIPT, &bytes, LIMIT, current.is_some())?,
        None => {
            // A preexisting new CA has no ownership receipt. Preserve that
            // absence instead of adopting it while retaining the old archive.
            if current.is_some() {
                std::fs::remove_file(active.join(RECEIPT))?;
            }
        }
    }
    Ok(())
}

pub(crate) fn finalize_rotation_old(
    old: &Path,
    binding: InstallationBinding,
    ca: Sha256Digest,
) -> Result<Option<TrustStatus>> {
    finalize_with(
        old,
        binding,
        ca,
        Host::current(),
        &mut |root, binding, sha| manage(root, binding, sha, Action::Remove),
    )
}
fn finalize_with(
    old: &Path,
    binding: InstallationBinding,
    ca: Sha256Digest,
    host: Host,
    remove: &mut dyn FnMut(&Path, InstallationBinding, Sha256Digest) -> Result<TrustStatus>,
) -> Result<Option<TrustStatus>> {
    binding.validate()?;
    let saved = preimage(old, &binding, ca, host)?;
    let current = archived_receipt(old, &saved, host)?;
    let Some(original) = saved.receipt else {
        ensure!(
            current.is_none(),
            "unowned archived issuer gained a trust receipt"
        );
        return Ok(None);
    };
    let current = current.context("archived owned trust receipt disappeared")?;
    let receipt = checked_receipt(&current, &binding, ca, host)?;
    if receipt.phase == Phase::Removed {
        return Ok(None);
    }
    ensure!(
        current == original.as_bytes(),
        "archived owned trust receipt changed after prepare"
    );
    let status = remove(old, binding, ca)?;
    ensure!(
        matches!(status.outcome, "removed" | "already_absent"),
        "old issuer removal outcome unknown; preserve rotation state for review"
    );
    Ok(Some(status))
}

pub(crate) fn abort_rotation_new(
    new: &Path,
    binding: InstallationBinding,
    ca: Sha256Digest,
) -> Result<Option<TrustStatus>> {
    abort_with(
        new,
        binding,
        ca,
        Host::current(),
        &mut |root, binding, sha| manage(root, binding, sha, Action::Remove),
    )
}
fn abort_with(
    new: &Path,
    binding: InstallationBinding,
    ca: Sha256Digest,
    host: Host,
    remove: &mut dyn FnMut(&Path, InstallationBinding, Sha256Digest) -> Result<TrustStatus>,
) -> Result<Option<TrustStatus>> {
    binding.validate()?;
    public_ca(new, ca)?;
    let Some(bytes) = read_private(new, RECEIPT, LIMIT)? else {
        return Ok(None);
    };
    let receipt = checked_receipt(&bytes, &binding, ca, host)?;
    if receipt.phase == Phase::Removed {
        return Ok(None);
    }
    let status = remove(new, binding, ca)?;
    ensure!(
        matches!(status.outcome, "removed" | "already_absent"),
        "candidate issuer removal outcome unknown; preserve rotation state for review"
    );
    Ok(Some(status))
}

#[cfg(test)]
mod tests;
