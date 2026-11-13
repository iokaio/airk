// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::http_endpoint::TlsIdentity;
use airk_core::identity::Incarnation;

struct Fixture {
    _owner: tempfile::TempDir,
    active: PathBuf,
    old: PathBuf,
    new: PathBuf,
    binding: InstallationBinding,
    old_ca: Sha256Digest,
    new_ca: Sha256Digest,
}
fn fixture() -> Result<Fixture> {
    let owner = tempfile::tempdir()?;
    let active = owner.path().join("active");
    let old = owner.path().join("old");
    let new = owner.path().join("new");
    let binding = InstallationBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        user: crate::local_ipc::current_user()?,
        namespace: Incarnation::from_bytes([2; 16]),
    };
    let now = 1_800_000_000_000;
    let old_identity = TlsIdentity::load_or_create(&active, binding.clone(), now)?;
    crate::local_ipc::prepare_root(&old)?;
    write_private(
        &old,
        "ca.pem",
        &read_private(&active, "ca.pem", 8192)?.unwrap(),
        8192,
        false,
    )?;
    let new_identity = TlsIdentity::load_or_create(&new, binding.clone(), now)?;
    Ok(Fixture {
        _owner: owner,
        active,
        old,
        new,
        binding,
        old_ca: old_identity.ca_sha256(),
        new_ca: new_identity.ca_sha256(),
    })
}
fn receipt(binding: &InstallationBinding, ca: Sha256Digest, phase: Phase) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&Receipt {
        schema: 1,
        binding: binding.clone(),
        ca_sha256: ca,
        store: PathBuf::from("CurrentUser/Root"),
        phase,
    })?)
}
fn save(root: &Path, bytes: &[u8]) -> Result<()> {
    write_private(
        root,
        RECEIPT,
        bytes,
        LIMIT,
        read_private(root, RECEIPT, LIMIT)?.is_some(),
    )
}
fn publish_public_new(f: &Fixture) -> Result<()> {
    // The TLS coordinator owns complete bundle publication. These isolated
    // receipt tests provide only its exact public-CA postcondition.
    write_private(
        &f.active,
        "ca.pem",
        &read_private(&f.new, "ca.pem", 8192)?.unwrap(),
        8192,
        true,
    )
}

#[test]
fn snapshot_records_immutable_presence_or_absence_and_cannot_adopt_changed_old_receipt()
-> Result<()> {
    for owned in [false, true] {
        let f = fixture()?;
        let original = owned
            .then(|| receipt(&f.binding, f.old_ca, Phase::Installed))
            .transpose()?;
        if let Some(bytes) = &original {
            save(&f.active, bytes)?;
        }
        snapshot_with(
            &f.active,
            &f.old,
            f.binding.clone(),
            f.old_ca,
            Host::Windows,
        )?;
        assert_eq!(read_private(&f.old, RECEIPT, LIMIT)?, original);
        let marker = read_private(&f.old, PREIMAGE, PREIMAGE_LIMIT)?.unwrap();
        snapshot_with(
            &f.active,
            &f.old,
            f.binding.clone(),
            f.old_ca,
            Host::Windows,
        )?;
        save(&f.active, &receipt(&f.binding, f.old_ca, Phase::Removed)?)?;
        assert!(
            snapshot_with(
                &f.active,
                &f.old,
                f.binding.clone(),
                f.old_ca,
                Host::Windows
            )
            .is_err()
        );
        assert_eq!(
            read_private(&f.old, PREIMAGE, PREIMAGE_LIMIT)?,
            Some(marker)
        );
    }
    Ok(())
}

#[test]
fn stopped_commit_transfers_exact_new_ownership_or_absence_and_resumes_without_overwrite()
-> Result<()> {
    for old_owned in [false, true] {
        for new_owned in [false, true] {
            let f = fixture()?;
            if old_owned {
                save(&f.active, &receipt(&f.binding, f.old_ca, Phase::Installed)?)?;
            }
            snapshot_with(
                &f.active,
                &f.old,
                f.binding.clone(),
                f.old_ca,
                Host::Windows,
            )?;
            let archived = read_private(&f.old, PREIMAGE, PREIMAGE_LIMIT)?;
            let replacement = new_owned
                .then(|| receipt(&f.binding, f.new_ca, Phase::Installed))
                .transpose()?;
            if let Some(bytes) = &replacement {
                save(&f.new, bytes)?;
            }
            publish_public_new(&f)?;
            commit_with(
                &f.active,
                &f.old,
                &f.new,
                f.binding.clone(),
                f.old_ca,
                f.new_ca,
                Host::Windows,
            )?;
            assert_eq!(read_private(&f.active, RECEIPT, LIMIT)?, replacement);
            assert_eq!(read_private(&f.old, PREIMAGE, PREIMAGE_LIMIT)?, archived);
            commit_with(
                &f.active,
                &f.old,
                &f.new,
                f.binding.clone(),
                f.old_ca,
                f.new_ca,
                Host::Windows,
            )?;
            let changed = receipt(&f.binding, f.new_ca, Phase::RemovePrepared)?;
            save(&f.active, &changed)?;
            assert!(
                commit_with(
                    &f.active,
                    &f.old,
                    &f.new,
                    f.binding.clone(),
                    f.old_ca,
                    f.new_ca,
                    Host::Windows
                )
                .is_err()
            );
            assert_eq!(read_private(&f.active, RECEIPT, LIMIT)?, Some(changed));
        }
    }
    Ok(())
}

#[test]
fn finalizer_removes_only_archived_owned_issuer_once_and_preserves_preexisting_old_issuer()
-> Result<()> {
    for owned in [false, true] {
        let f = fixture()?;
        if owned {
            save(&f.active, &receipt(&f.binding, f.old_ca, Phase::Installed)?)?;
        }
        snapshot_with(
            &f.active,
            &f.old,
            f.binding.clone(),
            f.old_ca,
            Host::Windows,
        )?;
        let active = read_private(&f.active, RECEIPT, LIMIT)?;
        let mut calls = 0;
        let mut remove =
            |root: &Path, binding: InstallationBinding, ca: Sha256Digest| -> Result<TrustStatus> {
                assert_eq!(root, f.old.as_path());
                assert!(binding == f.binding);
                assert!(ca == f.old_ca);
                calls += 1;
                save(root, &receipt(&binding, ca, Phase::Removed)?)?;
                Ok(TrustStatus {
                    schema: 1,
                    ca_sha256: ca,
                    platform: "fixture",
                    store: Some(PathBuf::from("CurrentUser/Root")),
                    present: false,
                    trusted: false,
                    owned: false,
                    outcome: "removed",
                    guidance: "fixture; no OS trust operation",
                })
            };
        assert_eq!(
            finalize_with(
                &f.old,
                f.binding.clone(),
                f.old_ca,
                Host::Windows,
                &mut remove
            )?
            .is_some(),
            owned
        );
        assert!(
            finalize_with(
                &f.old,
                f.binding.clone(),
                f.old_ca,
                Host::Windows,
                &mut remove
            )?
            .is_none()
        );
        assert_eq!(calls, usize::from(owned));
        assert_eq!(read_private(&f.active, RECEIPT, LIMIT)?, active);
    }
    Ok(())
}

#[test]
fn interrupted_or_crossed_receipts_preserve_every_preimage_without_os_replay() -> Result<()> {
    for phase in [Phase::Prepared, Phase::RemovePrepared] {
        let f = fixture()?;
        let unknown = receipt(&f.binding, f.old_ca, phase.clone())?;
        save(&f.active, &unknown)?;
        assert!(
            snapshot_with(
                &f.active,
                &f.old,
                f.binding.clone(),
                f.old_ca,
                Host::Windows
            )
            .is_err()
        );
        assert!(read_private(&f.old, PREIMAGE, PREIMAGE_LIMIT)?.is_none());
        save(&f.active, &receipt(&f.binding, f.old_ca, Phase::Installed)?)?;
        snapshot_with(
            &f.active,
            &f.old,
            f.binding.clone(),
            f.old_ca,
            Host::Windows,
        )?;
        let marker = read_private(&f.old, PREIMAGE, PREIMAGE_LIMIT)?;
        save(&f.new, &receipt(&f.binding, f.new_ca, phase)?)?;
        publish_public_new(&f)?;
        assert!(
            commit_with(
                &f.active,
                &f.old,
                &f.new,
                f.binding.clone(),
                f.old_ca,
                f.new_ca,
                Host::Windows
            )
            .is_err()
        );
        save(&f.old, &unknown)?;
        assert!(
            finalize_with(
                &f.old,
                f.binding.clone(),
                f.old_ca,
                Host::Windows,
                &mut |_, _, _| panic!("uncertain OS mutation replay")
            )
            .is_err()
        );
        assert_eq!(read_private(&f.old, PREIMAGE, PREIMAGE_LIMIT)?, marker);
        let mut crossed = f.binding.clone();
        crossed.installation = Sha256Digest::from_bytes([9; 32]);
        assert!(
            commit_with(
                &f.active,
                &f.old,
                &f.new,
                crossed,
                f.old_ca,
                f.new_ca,
                Host::Windows
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn commit_recovers_missing_archive_copy_only_from_immutable_preimage() -> Result<()> {
    let f = fixture()?;
    let original = receipt(&f.binding, f.old_ca, Phase::Installed)?;
    save(&f.active, &original)?;
    snapshot_with(
        &f.active,
        &f.old,
        f.binding.clone(),
        f.old_ca,
        Host::Windows,
    )?;
    std::fs::remove_file(f.old.join(RECEIPT))?;
    publish_public_new(&f)?;
    commit_with(
        &f.active,
        &f.old,
        &f.new,
        f.binding.clone(),
        f.old_ca,
        f.new_ca,
        Host::Windows,
    )?;
    assert_eq!(read_private(&f.old, RECEIPT, LIMIT)?, Some(original));
    assert!(read_private(&f.active, RECEIPT, LIMIT)?.is_none());
    Ok(())
}
#[test]
fn prepared_abort_preserves_active_issuer_and_all_preexisting_candidate_trust() -> Result<()> {
    for phase in [
        None,
        Some(Phase::Installed),
        Some(Phase::Removed),
        Some(Phase::Prepared),
        Some(Phase::RemovePrepared),
    ] {
        let f = fixture()?;
        let active = std::fs::read(f.active.join("tls-identity.json"))?;
        if let Some(phase) = &phase {
            save(&f.new, &receipt(&f.binding, f.new_ca, phase.clone())?)?;
        }
        let mut calls = 0;
        let mut remove =
            |root: &Path, binding: InstallationBinding, ca: Sha256Digest| -> Result<TrustStatus> {
                assert_eq!(root, f.new.as_path());
                assert!(binding == f.binding);
                assert!(ca == f.new_ca);
                calls += 1;
                save(root, &receipt(&binding, ca, Phase::Removed)?)?;
                Ok(TrustStatus {
                    schema: 1,
                    ca_sha256: ca,
                    platform: "fixture",
                    store: None,
                    present: false,
                    trusted: false,
                    owned: false,
                    outcome: "removed",
                    guidance: "fixture; no OS mutation",
                })
            };
        let result = abort_with(
            &f.new,
            f.binding.clone(),
            f.new_ca,
            Host::Windows,
            &mut remove,
        );
        match phase {
            Some(Phase::Prepared | Phase::RemovePrepared) => assert!(result.is_err()),
            Some(Phase::Installed) => assert!(result?.is_some()),
            _ => assert!(result?.is_none()),
        }
        assert_eq!(calls, usize::from(matches!(phase, Some(Phase::Installed))));
        assert_eq!(std::fs::read(f.active.join("tls-identity.json"))?, active);
        assert!(read_private(&f.active, RECEIPT, LIMIT)?.is_none());
    }
    Ok(())
}
