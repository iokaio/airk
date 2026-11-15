// SPDX-License-Identifier: Apache-2.0
//! The I4 installation reader (`docs/split/contracts.md` §5.2–§5.4; split step
//! 3b.1c): the per-user install pointer, the install record it names and the
//! pair of executables the record lists. Every check fails closed. Executable
//! signatures (§5.4.3; step 3b.1c-sig) are checked by the one implementation
//! in `airk_process::signature` against the C10 identity: Windows
//! Authenticode, macOS `codesign`, Linux digest only (documented weaker).
use airk_core::install_record::Signing;
#[cfg(test)]
use airk_core::install_record::{InstallPointer, InstallRecord, MAX_POINTER_BYTES};
pub(crate) use airk_ipc::install_reader::owned_bytes;
pub use airk_ipc::install_reader::{Discovery, Verified, read};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The fixed per-user pointer path (§5.2): native folders, never inherited
/// `HOME`, `XDG_*` or `LOCALAPPDATA` overrides.
pub fn pointer_path() -> anyhow::Result<PathBuf> {
    airk_process::user_folders::install_pointer()
}

/// The signature check of the platform this build runs on (§5.4.3; step
/// 3b.1c-sig) for the verified executables. A Windows or macOS record must
/// name the C10 identity and both images must carry it; a Linux record
/// carries no signature and rests on the record digest. There is no unsigned
/// or development acceptance here: the record's `signing` has none.
pub fn platform_signature(
    signing: &Signing,
    executables: &[&Path],
) -> Result<(), (String, String)> {
    airk_process::signature::verify(claim(signing), executables)
        .map(|_| ())
        .map_err(|refusal| (refusal.code.to_owned(), refusal.message))
}

/// The record's signing as the shared check reads it.
fn claim(signing: &Signing) -> airk_process::signature::Claim<'_> {
    use airk_process::signature::Claim;
    match signing {
        Signing::Windows {
            publisher,
            identity_eku,
        } => Claim::Windows {
            publisher,
            identity_eku,
        },
        Signing::Macos { team_id } => Claim::Macos { team_id },
        Signing::Linux { signed, integrity } => Claim::Linux {
            signed: *signed,
            integrity,
        },
    }
}

/// Discover this user's installation.
pub fn discover() -> Discovery {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(observation) =
        FIXTURE_DISCOVERY.with_borrow_mut(|factory| factory.as_mut().map(|factory| factory()))
    {
        return observation;
    }
    match pointer_path() {
        Ok(path) => read(&path, platform_signature),
        Err(error) => Discovery::Unverified {
            code: "installation_unverified".into(),
            message: format!("{error:#}"),
        },
    }
}

#[cfg(any(test, feature = "test-support"))]
type FixtureDiscovery = Box<dyn FnMut() -> Discovery>;
#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static FIXTURE_DISCOVERY: std::cell::RefCell<Option<FixtureDiscovery>> = const { std::cell::RefCell::new(None) };
}

/// Scoped in-process fixture only. No CLI flag/environment/runtime bypass exists.
#[cfg(any(test, feature = "test-support"))]
pub fn with_fixture_discovery<R>(
    factory: impl FnMut() -> Discovery + 'static,
    run: impl FnOnce() -> R,
) -> R {
    struct Restore(Option<FixtureDiscovery>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FIXTURE_DISCOVERY.with_borrow_mut(|current| *current = self.0.take());
        }
    }
    let previous = FIXTURE_DISCOVERY.with_borrow_mut(|current| current.replace(Box::new(factory)));
    let _restore = Restore(previous);
    run()
}

#[cfg(test)]
mod tests;
