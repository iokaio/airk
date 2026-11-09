// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn explicit_cache_replan_retains_removed_images_and_adds_new_owned_caches() -> Result<()> {
    let fixture = Fixture::new()?;
    let initial = fixture.inspect(None, true)?;
    let old = initial["inspection_sha256"].as_str().unwrap();
    let extra = cache_at(&fixture.global, "1.2.7", b"Another inert owned old image")?;
    // Reproduce interrupted cleanup after one image disappeared, with its
    // original empty directory retained. Replan cannot invent that identity.
    fs::remove_file(&fixture.image)?;
    let next = reprepare(&fixture.root, &fixture.selected, &fixture.global, None, old)?;
    assert_ne!(next["inspection_sha256"], initial["inspection_sha256"]);
    assert_eq!(next["caches"].as_array().unwrap().len(), 2);
    assert!(extra.exists());
    assert!(!fixture.image.exists());
    assert_eq!(next["scope"], initial["scope"]);
    assert_eq!(next["caches"][0]["artifact_approved"], false);
    let resumed = reprepare(&fixture.root, &fixture.selected, &fixture.global, None, old)?;
    assert_eq!(resumed, next);
    // Simulate the durable archive preceding the current-journal replacement.
    let archived_path = fixture.root.join(format!(
        "legacy-cache-replan-{}-{old}.json",
        fixture.selected
    ));
    let archive: serde_json::Value = serde_json::from_slice(&fs::read(&archived_path)?)?;
    replace_json(&fixture.root, JOURNAL, &archive["previous"])?;
    assert_eq!(
        reprepare(&fixture.root, &fixture.selected, &fixture.global, None, old)?,
        next
    );
    fixture.assert_preserved()
}

#[test]
fn cache_replan_refuses_crossed_preimage_and_unknown_empty_leaf() -> Result<()> {
    let fixture = Fixture::new()?;
    let initial = fixture.inspect(None, true)?;
    let old = initial["inspection_sha256"].as_str().unwrap();
    assert!(
        reprepare(
            &fixture.root,
            &fixture.selected,
            &fixture.global,
            None,
            &"f".repeat(64)
        )
        .is_err()
    );
    let unknown = cache_at(&fixture.global, "1.2.7", b"Unaccepted empty leaf")?;
    fs::remove_file(&unknown)?;
    let original_journal = fs::read(fixture.root.join(JOURNAL))?;
    assert!(reprepare(&fixture.root, &fixture.selected, &fixture.global, None, old).is_err());
    assert_eq!(fs::read(fixture.root.join(JOURNAL))?, original_journal);
    assert!(fixture.image.exists());
    fixture.assert_preserved()
}

#[test]
fn cache_replan_bound_keeps_last_accepted_roster_and_unknown_new_image() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut accepted = fixture.inspect(None, true)?;
    for number in 0..4 {
        cache_at(
            &fixture.global,
            &format!("1.2.{}", number + 1),
            format!("Inert old image {number}").as_bytes(),
        )?;
        accepted = reprepare(
            &fixture.root,
            &fixture.selected,
            &fixture.global,
            None,
            accepted["inspection_sha256"].as_str().unwrap(),
        )?;
    }
    let extra = cache_at(&fixture.global, "1.2.9", b"Preserve after bounded replans")?;
    let before = fs::read(fixture.root.join(JOURNAL))?;
    assert!(
        reprepare(
            &fixture.root,
            &fixture.selected,
            &fixture.global,
            None,
            accepted["inspection_sha256"].as_str().unwrap()
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.root.join(JOURNAL))?, before);
    assert!(extra.exists());
    fixture.assert_preserved()
}

#[test]
fn cache_replan_resume_rejects_self_consistent_archive_that_drops_prior_proof() -> Result<()> {
    let fixture = Fixture::new()?;
    let initial = fixture.inspect(None, true)?;
    let old = initial["inspection_sha256"].as_str().unwrap();
    cache_at(&fixture.global, "1.2.7", b"Another bounded owned image")?;
    reprepare(&fixture.root, &fixture.selected, &fixture.global, None, old)?;
    let filename = format!("legacy-cache-replan-{}-{old}.json", fixture.selected);
    let mut archive: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join(&filename))?)?;
    replace_json(&fixture.root, JOURNAL, &archive["previous"])?;
    let before = fs::read(fixture.root.join(JOURNAL))?;
    let mut crossed: Inspection = serde_json::from_value(archive["next"]["inspection"].clone())?;
    crossed.caches.clear();
    archive["next"]["inspection_sha256"] = crossed.digest()?.into();
    archive["next"]["inspection"] = serde_json::to_value(crossed)?;
    replace_json(&fixture.root, &filename, &archive)?;
    assert!(reprepare(&fixture.root, &fixture.selected, &fixture.global, None, old).is_err());
    assert_eq!(fs::read(fixture.root.join(JOURNAL))?, before);
    fixture.assert_preserved()
}
