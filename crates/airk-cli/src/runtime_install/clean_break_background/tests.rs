// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn clean_break_background_accepts_only_installed_single_image_shapes() {
    let root = Path::new("fixture-install");
    let payload = root.join("runtimes").join("a".repeat(64));
    for tail in ["airk.exe", "airk", "AIRK.app/Contents/MacOS/airk"] {
        assert_eq!(
            installed_root(&payload.join(tail)),
            Some(root.to_path_buf())
        );
    }
    for tail in [
        "airk-service.exe",
        "airk-source",
        "arbitrary.exe",
        "AIRK.app/Other/MacOS/airk",
    ] {
        assert_eq!(installed_root(&payload.join(tail)), None);
    }
    assert_eq!(installed_root(&root.join("debug/airk.exe")), None);
    assert_eq!(
        installed_root(&root.join("runtimes").join("A".repeat(64)).join("airk.exe")),
        None
    );
}

#[test]
fn clean_break_background_cadence_uses_injected_monotonic_time() -> anyhow::Result<()> {
    let now = Instant::now();
    let mut continuation = Continuation::new(None, now);
    continuation.tick_with(Path::new("unused"), now, |_, _| unreachable!())?;
    assert_eq!(continuation.next, now + Duration::from_secs(1));
    continuation.tick_with(
        Path::new("unused"),
        now + Duration::from_millis(999),
        |_, _| unreachable!(),
    )?;
    assert_eq!(continuation.next, now + Duration::from_secs(1));
    continuation.tick_with(
        Path::new("unused"),
        now + Duration::from_secs(1),
        |_, _| unreachable!(),
    )?;
    assert_eq!(continuation.next, now + Duration::from_secs(2));
    continuation.pending = true;
    assert!(
        continuation.tick_with(
            Path::new("unused"),
            now + Duration::from_millis(1500),
            |_, _| unreachable!()
        )?,
        "accepted cleanup must stay charged between bounded maintenance turns"
    );
    Ok(())
}
