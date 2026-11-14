// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn native_session_permit_validates_real_facts_and_renews_idle_only_after_admission() {
    let mut sessions = secrets::Sessions::new(Incarnation::from_bytes([1; 16]));
    let minted = sessions.create(&[2; 32], &[3; 32], 0);
    assert!(
        sessions
            .native_permit(&minted.cookie, &secrets::secret_text(&[4; 32]), 100)
            .is_none()
    );
    assert_eq!(
        sessions
            .facts(&[&minted.cookie], &[&minted.csrf], 100)
            .unwrap()
            .last_used_at_ms,
        0
    );
    let permit = sessions
        .native_permit(&minted.cookie, &minted.csrf, 100)
        .unwrap();
    assert_eq!(
        sessions
            .facts(&[&minted.cookie], &[&minted.csrf], 100)
            .unwrap()
            .last_used_at_ms,
        100
    );
    assert!(permit.valid(100 + SESSION_IDLE_MS - 1));
    assert!(!permit.valid(100 + SESSION_IDLE_MS));
}

#[test]
fn native_session_permit_never_extends_the_absolute_session_lifetime() {
    let mut sessions = secrets::Sessions::new(Incarnation::from_bytes([1; 16]));
    let minted = sessions.create(&[2; 32], &[3; 32], 0);
    for now in (0..SESSION_ABSOLUTE_MS).step_by((SESSION_IDLE_MS / 2) as usize) {
        assert!(
            sessions
                .native_permit(&minted.cookie, &minted.csrf, now)
                .unwrap()
                .valid(now)
        );
    }
    let permit = sessions
        .native_permit(&minted.cookie, &minted.csrf, SESSION_ABSOLUTE_MS - 1)
        .unwrap();
    assert!(permit.valid(SESSION_ABSOLUTE_MS - 1));
    assert!(!permit.valid(SESSION_ABSOLUTE_MS));
}

#[test]
fn native_session_permit_invalidation_is_shared_on_close_expiry_eviction_and_owner_drop() {
    for ending in ["close", "expiry", "eviction", "end", "drop"] {
        let mut sessions = secrets::Sessions::new(Incarnation::from_bytes([1; 16]));
        let minted = sessions.create(&[2; 32], &[3; 32], 0);
        let permit = sessions
            .native_permit(&minted.cookie, &minted.csrf, 0)
            .unwrap();
        let copy = permit.clone();
        assert!(permit.valid(0));
        match ending {
            "close" => {
                assert!(sessions.remove(&minted.cookie));
            }
            "expiry" => sessions.purge(SESSION_IDLE_MS),
            "eviction" => {
                for n in 10..10 + MAX_SESSIONS as u8 {
                    sessions.create(&[n; 32], &[n + 1; 32], 0);
                }
            }
            "end" => sessions.end(),
            "drop" => drop(sessions),
            _ => unreachable!(),
        }
        // Rewinding this observation must not revive an already ended owner.
        assert!(!permit.valid(0), "{ending}");
        assert!(!copy.valid(0), "{ending}");
    }
}
