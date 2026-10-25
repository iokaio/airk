// SPDX-License-Identifier: Apache-2.0
//! Signed-bundle-scoped login ownership; never mutate a raw launchd label.
use super::*;
use crate::runtime_install::{RuntimeChannel, macos_login};
use airk_core::identity::{Sha256Digest, framed_digest};
use std::fs::File;

const RECEIPT: &str = "mac-login-1.4.json";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    root: PathBuf,
    channel: RuntimeChannel,
    target: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    value: Owner,
    sha256: Sha256Digest,
}
fn digest(owner: &Owner) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"mac-login-ownership/1.4",
        &[&serde_json::to_vec(owner)?],
    ))
}
fn decode(bytes: &[u8], root: &Path, channel: RuntimeChannel) -> Result<Owner> {
    ensure!(bytes.len() <= 4096, "Mac login ownership exceeds budget");
    let receipt: Envelope = serde_json::from_slice(bytes)?;
    let owner = &receipt.value;
    ensure!(
        owner.version == 1
            && owner.root == root
            && owner.root.is_absolute()
            && owner.channel == channel
            && owner.target.len() == 64
            && owner
                .target
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && receipt.sha256 == digest(owner)?
            && serde_json::to_vec(&receipt)? == bytes,
        "changed or foreign Mac login ownership receipt"
    );
    Ok(receipt.value)
}
pub(super) fn verify_intent(root: &Path, identity: &str, channel: RuntimeChannel) -> Result<()> {
    let bytes = read_config(&root.join(RECEIPT))?
        .context("Mac login mutation has no captured ownership intent")?;
    let owner = decode(&bytes, &root.canonicalize()?, channel)?;
    ensure!(
        owner.target == identity,
        "Mac login mutation differs from captured runtime intent"
    );
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    NotRegistered,
    Enabled,
    Approval,
    NotFound,
}
pub(super) struct Manager<'a> {
    service: &'a ServiceManager,
}
impl<'a> Manager<'a> {
    pub(super) fn new(service: &'a ServiceManager) -> Self {
        Self { service }
    }
    fn root(&self) -> &Path {
        Path::new(&self.service.data)
    }
    fn path(&self) -> PathBuf {
        self.root().join(RECEIPT)
    }
    fn selected(&self) -> Result<Owner> {
        let (_, approval) = macos_login::candidate(self.root())?;
        Ok(Owner {
            version: 1,
            root: self.root().canonicalize()?,
            channel: macos_login::channel(self.root())?,
            target: approval.manifest_sha256,
        })
    }
    fn read(&self) -> Result<Option<Owner>> {
        let Some(bytes) = read_config(&self.path())? else {
            return Ok(None);
        };
        Ok(Some(decode(
            &bytes,
            &self.root().canonicalize()?,
            macos_login::channel(self.root())?,
        )?))
    }
    fn save(&self, owner: &Owner) -> Result<()> {
        let prior = self.read()?;
        let bytes = serde_json::to_vec(&Envelope {
            value: owner.clone(),
            sha256: digest(owner)?,
        })?;
        ensure!(bytes.len() <= 4096, "Mac login ownership exceeds budget");
        let prior = prior
            .map(|value| -> Result<Vec<u8>> {
                Ok(serde_json::to_vec(&Envelope {
                    sha256: digest(&value)?,
                    value,
                })?)
            })
            .transpose()?;
        for (count, entry) in fs::read_dir(self.root())?.enumerate() {
            ensure!(count < 256, "Mac login metadata entry budget exceeded");
            let entry = entry?;
            if !entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".airk-mac-login-"))
            {
                continue;
            }
            let partial = read_config(&entry.path())?.context("Mac login scratch disappeared")?;
            ensure!(
                partial.len() <= 4096
                    && (bytes.starts_with(&partial)
                        || prior
                            .as_ref()
                            .is_some_and(|original| original.starts_with(&partial))),
                "unknown Mac login partial preserved"
            );
            fs::remove_file(entry.path())?;
        }
        let mut temporary = tempfile::Builder::new()
            .prefix(".airk-mac-login-")
            .tempfile_in(self.root())?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.path())?;
        File::open(self.root())?.sync_all()?;
        Ok(())
    }
    fn invoke(&self, owner: &Owner, action: &str) -> Result<State> {
        ensure!(
            matches!(action, "status" | "register" | "unregister"),
            "invalid Mac login operation"
        );
        let main = macos_login::target_main(self.root(), &owner.target)?;
        let invocation = Invocation::new(
            main,
            &[
                "runtime",
                "mac-login",
                "--install-dir",
                self.root().to_str().context("non-UTF-8 install root")?,
                "--runtime-sha256",
                &owner.target,
                "--channel",
                macos_login::channel_name(owner.channel),
                "--action",
                action,
            ],
        );
        let output = execute(&invocation)?;
        ensure!(output.success, "Mac login {action} failed: {}", output.text);
        let value: serde_json::Value = serde_json::from_str(&output.text)?;
        ensure!(
            value["version"] == 1
                && value["runtime"] == owner.target
                && value["channel"] == serde_json::to_value(owner.channel)?,
            "crossed Mac login observation"
        );
        match value["state"].as_str() {
            Some("not_registered") => Ok(State::NotRegistered),
            Some("enabled") => Ok(State::Enabled),
            Some("requires_approval") => Ok(State::Approval),
            Some("not_found") => Ok(State::NotFound),
            _ => bail!("unsupported Mac login observation"),
        }
    }
    pub(super) fn plan(&self) -> Result<ServicePlan> {
        let owner = self.selected()?;
        let main = macos_login::target_main(self.root(), &owner.target)?;
        Ok(ServicePlan {
            id: self.service.id.clone(),
            platform: "smappservice_user",
            definition_path: self.path(),
            definition: serde_json::to_string(&owner)?,
            executable: main,
            arguments: vec![
                "runtime".into(),
                "login-foreground".into(),
                "--channel".into(),
                macos_login::channel_name(owner.channel).into(),
            ],
            scope: "current-user signed app registration; macOS 13+; subject to Login Items approval",
        })
    }
    pub(super) fn owned(&self) -> Result<bool> {
        Ok(self.read()?.is_some())
    }
    pub(super) fn status(&self) -> Result<ServiceStatus> {
        let saved = self.read()?;
        let owned = saved.is_some();
        let owner = saved.map(Ok).unwrap_or_else(|| self.selected())?;
        let observation = self.invoke(&owner, "status");
        let (loaded, running, matched, diagnostic, output) = match observation {
            Ok(State::NotRegistered) => (Some(false), Some(false), None, None, "not_registered".into()),
            Ok(State::Enabled) => {
                let running = match macos_login::quiet(self.root()) { Ok(quiet) => Some(!quiet), Err(_) => None };
                (Some(true), running, owned.then_some(true), None, "enabled".into())
            }
            Ok(State::Approval) => (Some(true), Some(false), owned.then_some(true), Some("Approve AIRK in System Settings > General > Login Items; user consent is preserved".into()), "requires_approval".into()),
            Ok(State::NotFound) => (None, None, None, Some("Service Management could not locate the captured app service".into()), "not_found".into()),
            Err(error) => { let message: String = error.to_string().chars().take(512).collect(); (None, None, None, Some(message.clone()), message) }
        };
        Ok(ServiceStatus {
            id: self.service.id.clone(),
            platform: "smappservice_user",
            definition_path: self.path(),
            installed: owned,
            loaded,
            running,
            registered_definition_matches: matched,
            registration_verification_error: diagnostic,
            manager_output: output,
        })
    }
    pub(super) fn install(&self) -> Result<ServiceStatus> {
        macos_login::require_start_allowed(self.root())?;
        ensure!(
            !self.owned()?,
            "Mac login intent already exists; use Start or Uninstall"
        );
        let owner = self.selected()?;
        ensure!(
            self.invoke(&owner, "status")? == State::NotRegistered,
            "existing or unverifiable Mac registration is preserved"
        );
        ensure!(
            macos_login::quiet(self.root())?,
            "drain native owners before registering login startup"
        );
        self.save(&owner)?;
        self.start()
    }
    pub(super) fn start(&self) -> Result<ServiceStatus> {
        macos_login::require_start_allowed(self.root())?;
        let mut owner = self.read()?.context("Mac login is not installed")?;
        let next = self.selected()?;
        let state = self.invoke(&owner, "status")?;
        if owner != next {
            ensure!(
                state == State::NotRegistered && macos_login::quiet(self.root())?,
                "stop the captured Mac registration and native owners before changing payload"
            );
            self.save(&next)?;
            owner = next;
        }
        match self.invoke(&owner, "status")? {
            State::NotRegistered => {
                self.invoke(&owner, "register")?;
            }
            State::Enabled => (),
            State::Approval => {
                bail!("Approve AIRK in System Settings > General > Login Items before Start")
            }
            State::NotFound => {
                bail!("Mac service registration is unverifiable; existing state is preserved")
            }
        }
        let status = self.status()?;
        ensure!(
            status.manager_output != "requires_approval",
            "Mac login was registered; approve AIRK in System Settings > General > Login Items, then retry Start"
        );
        ensure!(
            status.loaded == Some(true) && status.registered_definition_matches == Some(true),
            "Mac login registration did not verify"
        );
        Ok(status)
    }
    pub(super) fn stop(&self) -> Result<ServiceStatus> {
        let owner = self.read()?.context("Mac login is not installed")?;
        match self.invoke(&owner, "status")? {
            State::NotRegistered => (),
            State::Enabled | State::Approval => {
                self.invoke(&owner, "unregister")?;
            }
            State::NotFound => bail!("unverifiable Mac registration is preserved"),
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            ensure!(
                self.read()?.as_ref() == Some(&owner),
                "Mac login ownership changed during Stop"
            );
            if self.invoke(&owner, "status")? == State::NotRegistered
                && macos_login::quiet(self.root())?
            {
                return self.status();
            }
            ensure!(
                Instant::now() < deadline,
                "Mac login/native owners have not confirmed shutdown"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }
    pub(super) fn uninstall(&self) -> Result<ServiceStatus> {
        let owner = self.read()?.context("Mac login is not installed")?;
        self.stop()?;
        ensure!(
            self.read()?.as_ref() == Some(&owner),
            "Mac login receipt changed before removal"
        );
        fs::remove_file(self.path())?;
        File::open(self.root())?.sync_all()?;
        self.status()
    }
    pub(super) fn owns_restart(&self) -> Result<bool> {
        if !self.owned()? {
            return Ok(false);
        }
        let status = self.status()?;
        ensure!(
            status.manager_output != "requires_approval",
            "Mac login consent is disabled or pending; preserve it and approve explicitly before runtime activation"
        );
        ensure!(
            status.loaded == Some(false)
                || status.loaded == Some(true)
                    && status.registered_definition_matches == Some(true),
            "Mac registration ownership is unavailable; preserve it for explicit diagnosis"
        );
        Ok(true)
    }
    pub(super) fn require_stopped(&self) -> Result<()> {
        if !self.owned()? {
            return Ok(());
        }
        let owner = self.read()?.context("Mac login receipt disappeared")?;
        ensure!(
            self.invoke(&owner, "status")? == State::NotRegistered
                && macos_login::quiet(self.root())?,
            "stop owned Mac login and native workers before maintenance"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mac_login_receipt_rejects_cross_root_channel_target_and_changed_metadata() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().canonicalize()?;
        let owner = Owner {
            version: 1,
            root: root.clone(),
            channel: RuntimeChannel::Stable,
            target: "ab".repeat(32),
        };
        let original = serde_json::to_vec(&Envelope {
            value: owner.clone(),
            sha256: digest(&owner)?,
        })?;
        assert!(verify_intent(&root, &owner.target, RuntimeChannel::Stable).is_err());
        fs::write(root.join(RECEIPT), &original)?;
        verify_intent(&root, &owner.target, RuntimeChannel::Stable)?;
        assert!(verify_intent(&root, &"cd".repeat(32), RuntimeChannel::Stable).is_err());
        assert!(verify_intent(&root, &owner.target, RuntimeChannel::Preview).is_err());
        assert_eq!(decode(&original, &root, RuntimeChannel::Stable)?, owner);
        assert!(decode(&original, &root.join("foreign"), RuntimeChannel::Stable).is_err());
        assert!(decode(&original, &root, RuntimeChannel::Preview).is_err());
        let mut changed: serde_json::Value = serde_json::from_slice(&original)?;
        changed["value"]["target"] = "cd".repeat(32).into();
        assert!(
            decode(
                &serde_json::to_vec(&changed)?,
                &root,
                RuntimeChannel::Stable
            )
            .is_err()
        );
        let mut incompatible = owner.clone();
        incompatible.version = 2;
        let bytes = serde_json::to_vec(&Envelope {
            sha256: digest(&incompatible)?,
            value: incompatible,
        })?;
        assert!(decode(&bytes, &root, RuntimeChannel::Stable).is_err());
        let mut noncanonical = original.clone();
        noncanonical.push(b'\n');
        assert!(decode(&noncanonical, &root, RuntimeChannel::Stable).is_err());
        assert!(decode(&vec![b' '; 4097], &root, RuntimeChannel::Stable).is_err());
        Ok(())
    }
}
