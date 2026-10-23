// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    let state = crate::user_paths::canonical_location(&owner.path().join("state"))?;
    Ok((owner, root.canonicalize()?, state))
}

#[test]
fn separate_writable_state_is_pinned_without_creating_control_or_opening_an_index() -> Result<()> {
    let (_owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, state.join("control"));
    assert!(!state.join("control").exists());
    assert!(!root.join("control").exists());
    assert_eq!(inspect(&root)?["state_claimed"], true);
    initialize(&root, None, RuntimeChannel::Stable)?;
    assert!(
        initialize(
            &root,
            Some(&root.join("other-state")),
            RuntimeChannel::Stable
        )
        .is_err()
    );
    assert!(initialize(&root, None, RuntimeChannel::Preview).is_err());
    assert!(abort(&root).is_err());
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn interrupted_state_claim_never_falls_back_and_resumes_before_and_after_lock_creation()
-> Result<()> {
    let (_owner, root, state) = fixture()?;
    let layout = Layout {
        schema: 1,
        install: root.clone(),
        state: state.clone(),
        channel: RuntimeChannel::Stable,
        ready: false,
    };
    replace_json(&root, LAYOUT, &layout)?;
    assert!(control(&root).is_err());
    assert_eq!(inspect(&root)?["state_claimed"], false);
    local_ipc::prepare_root(&state)?;
    fs::write(state.join(CLAIM), [])?;
    assert!(control(&root).is_err());
    initialize(&root, None, RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn failed_or_abandoned_claim_preserves_foreign_state_and_permits_a_new_reviewed_location()
-> Result<()> {
    let (_owner, root, state) = fixture()?;
    local_ipc::prepare_root(&state)?;
    let source = state.join("original-document.txt");
    fs::write(&source, b"Preserve original unrelated bytes.")?;
    assert!(initialize(&root, Some(&state), RuntimeChannel::Stable).is_err());
    assert!(control(&root).is_err());
    assert!(!state.join(OWNER).exists());
    assert!(!state.join(CLAIM).exists());
    abort(&root)?;
    assert_eq!(fs::read(&source)?, b"Preserve original unrelated bytes.");
    let replacement = root.join("reviewed-state");
    initialize(&root, Some(&replacement), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, replacement.join("control"));
    Ok(())
}

#[test]
fn another_installation_or_channel_cannot_adopt_claimed_state() -> Result<()> {
    let (owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let before = fs::read(state.join(OWNER))?;
    let second = owner.path().join("second-install");
    local_ipc::prepare_root(&second)?;
    assert!(initialize(&second, Some(&state), RuntimeChannel::Stable).is_err());
    assert!(control(&second).is_err());
    abort(&second)?;
    assert_eq!(fs::read(state.join(OWNER))?, before);
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn lost_layout_requires_reviewed_claimed_state_instead_of_an_empty_ledger() -> Result<()> {
    let (_owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    fs::write(root.join("selected.json"), b"Existing selection marker.")?;
    fs::write(
        state.join("original-work-metadata"),
        b"Preserve accepted work.",
    )?;
    fs::remove_file(root.join(LAYOUT))?;
    assert!(control(&root).is_err());
    assert!(initialize(&root, None, RuntimeChannel::Stable).is_err());
    assert!(
        initialize(
            &root,
            Some(&root.join("different-state")),
            RuntimeChannel::Stable
        )
        .is_err()
    );
    assert!(!root.join(LAYOUT).exists());
    assert!(!root.join("control").exists());
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, state.join("control"));
    assert_eq!(
        fs::read(state.join("original-work-metadata"))?,
        b"Preserve accepted work."
    );
    Ok(())
}

#[test]
fn modified_or_aliased_state_ownership_is_preserved_and_never_redirects_control() -> Result<()> {
    let (owner, root, state) = fixture()?;
    initialize(&root, Some(&state), RuntimeChannel::Stable)?;
    let receipt = state.join(OWNER);
    let original = fs::read(&receipt)?;
    fs::write(&receipt, b"Unrecognized user metadata.")?;
    assert!(control(&root).is_err());
    assert!(initialize(&root, None, RuntimeChannel::Stable).is_err());
    assert_eq!(fs::read(&receipt)?, b"Unrecognized user metadata.");
    fs::write(&receipt, &original)?;
    let alias = owner.path().join("unrelated-receipt-alias");
    fs::hard_link(&receipt, &alias)?;
    assert!(control(&root).is_err());
    assert_eq!(fs::read(&alias)?, original);
    fs::remove_file(alias)?;
    assert_eq!(control(&root)?, state.join("control"));
    Ok(())
}

#[test]
fn existing_control_state_requires_explicit_transition_instead_of_silent_relocation() -> Result<()>
{
    let (_owner, root, state) = fixture()?;
    let old = root.join("control");
    local_ipc::prepare_root(&old)?;
    fs::write(
        old.join("original-owned-fixture"),
        b"Preserve existing control state.",
    )?;
    assert!(initialize(&root, Some(&state), RuntimeChannel::Stable).is_err());
    assert!(!root.join(LAYOUT).exists());
    assert!(!state.exists());
    initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    assert_eq!(control(&root)?, old);
    assert_eq!(
        fs::read(old.join("original-owned-fixture"))?,
        b"Preserve existing control state."
    );
    Ok(())
}
