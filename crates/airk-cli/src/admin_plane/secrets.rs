// SPDX-License-Identifier: Apache-2.0
//! In-memory launch codes (A1, contracts §2.7) and admin sessions (§4.6).
//!
//! Pure state: every operation takes the injected clock reading and the
//! random bytes from its caller. Only SHA-256 digests of the secrets are
//! kept; the secret text exists only in the reply that issues it. Nothing
//! here implements `Debug`, so no secret or digest can reach a log.
use crate::admin_contract::{
    LAUNCH_CODE_TTL_MS, LaunchCodeFacts, MAX_SESSIONS, SESSION_ABSOLUTE_MS, SESSION_IDLE_MS,
    SessionFacts,
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::CatalogRevision,
    service::WindowScopeSelection,
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use subtle::ConstantTimeEq;

/// At most this many launch codes are outstanding; the oldest is evicted.
pub(super) const MAX_LAUNCH_CODES: usize = 8;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Unpadded base64url of 32 bytes: 43 characters, canonical final character.
pub(super) fn secret_text(bytes: &[u8; 32]) -> String {
    let mut text = String::with_capacity(43);
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (index, byte)| {
            word | u32::from(*byte) << (16 - 8 * index)
        });
        let symbols = chunk.len() + 1;
        for index in 0..symbols {
            let shift = 18 - 6 * index;
            text.push(char::from(ALPHABET[((word >> shift) & 0x3f) as usize]));
        }
    }
    text
}

/// SHA-256 of the secret's exact ASCII text: a non-canonical spelling of the
/// same bits is a different, unknown secret.
pub(super) fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

struct StoredCode {
    digest: [u8; 32],
    issued_at_ms: u64,
    instance: Incarnation,
}

/// Outstanding launch codes of this service instance.
pub(super) struct LaunchCodes {
    instance: Incarnation,
    codes: VecDeque<StoredCode>,
}

impl LaunchCodes {
    pub(super) fn new(instance: Incarnation) -> Self {
        Self {
            instance,
            codes: VecDeque::new(),
        }
    }

    /// Issue one code from 32 random bytes; returns its text and lifetime.
    pub(super) fn issue(&mut self, random: &[u8; 32], now_ms: u64) -> (String, u64) {
        let text = secret_text(random);
        let digest = digest(&text);
        self.codes.retain(|code| code.digest != digest);
        while self.codes.len() >= MAX_LAUNCH_CODES {
            self.codes.pop_front();
        }
        self.codes.push_back(StoredCode {
            digest,
            issued_at_ms: now_ms,
            instance: self.instance,
        });
        (text, LAUNCH_CODE_TTL_MS)
    }

    /// The atomic compare-and-remove of §4.5 step 3: the stored code whose
    /// digest equals the presented text's, removed whether or not it is
    /// still live, so a code is consumed at most once.
    pub(super) fn consume(&mut self, presented: &str) -> Option<LaunchCodeFacts> {
        let digest = digest(presented);
        let position = self
            .codes
            .iter()
            .position(|code| bool::from(code.digest[..].ct_eq(&digest[..])))?;
        let code = self.codes.remove(position)?;
        Some(LaunchCodeFacts {
            issued_at_ms: code.issued_at_ms,
            same_instance: code.instance == self.instance,
        })
    }

    #[cfg(test)]
    pub(super) fn outstanding(&self) -> usize {
        self.codes.len()
    }

    #[cfg(test)]
    pub(super) fn stored_digests(&self) -> Vec<([u8; 32], u64)> {
        self.codes
            .iter()
            .map(|code| (code.digest, code.issued_at_ms))
            .collect()
    }
}

/// An admin session's read grant (A2; contracts §4.6, U77): the checked
/// selection, held only in memory inside its session, so logout, expiry,
/// eviction and restart drop it. It never leaves the service.
#[derive(Clone)]
pub(super) struct Grant {
    pub(super) selection: WindowScopeSelection,
    pub(super) scope_identity: Sha256Digest,
    pub(super) catalog_revision: CatalogRevision,
    /// The service instance that checked it.
    pub(super) instance: Incarnation,
}

struct Session {
    cookie: [u8; 32],
    csrf: [u8; 32],
    created_at_ms: u64,
    last_used_at_ms: u64,
    instance: Incarnation,
    grant: Option<Grant>,
}

impl Session {
    fn expired(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_used_at_ms) >= SESSION_IDLE_MS
            || now_ms.saturating_sub(self.created_at_ms) >= SESSION_ABSOLUTE_MS
    }

    /// When the session (and so its grant) ends without further use.
    fn expires_at(&self) -> u64 {
        self.last_used_at_ms
            .saturating_add(SESSION_IDLE_MS)
            .min(self.created_at_ms.saturating_add(SESSION_ABSOLUTE_MS))
    }
}

/// A freshly created session's secrets, returned once.
pub(super) struct Minted {
    pub(super) cookie: String,
    pub(super) csrf: String,
}

/// Admin sessions of this service instance (≤ 16, oldest evicted).
pub(super) struct Sessions {
    instance: Incarnation,
    sessions: VecDeque<Session>,
}

impl Sessions {
    pub(super) fn new(instance: Incarnation) -> Self {
        Self {
            instance,
            sessions: VecDeque::new(),
        }
    }

    /// A fresh cookie and an independent anti-forgery token (M2 rule 2).
    pub(super) fn create(&mut self, cookie: &[u8; 32], csrf: &[u8; 32], now_ms: u64) -> Minted {
        self.purge(now_ms);
        let minted = Minted {
            cookie: secret_text(cookie),
            csrf: secret_text(csrf),
        };
        while self.sessions.len() >= MAX_SESSIONS {
            self.sessions.pop_front();
        }
        self.sessions.push_back(Session {
            cookie: digest(&minted.cookie),
            csrf: digest(&minted.csrf),
            created_at_ms: now_ms,
            last_used_at_ms: now_ms,
            instance: self.instance,
            grant: None,
        });
        minted
    }

    fn find(&self, cookie: &str) -> Option<&Session> {
        let digest = digest(cookie);
        self.sessions
            .iter()
            .find(|session| bool::from(session.cookie[..].ct_eq(&digest[..])))
    }

    /// The facts admission needs. `cookies` are every presented admin
    /// cookie value; only exactly one can name a session. The token
    /// comparison is constant time over digests.
    pub(super) fn facts(
        &mut self,
        cookies: &[&str],
        csrf: &[&str],
        now_ms: u64,
    ) -> Option<SessionFacts> {
        self.purge(now_ms);
        let [cookie] = cookies else {
            return None;
        };
        let session = self.find(cookie)?;
        let token_matches = match csrf {
            [token] => bool::from(session.csrf[..].ct_eq(&digest(token)[..])),
            _ => false,
        };
        Some(SessionFacts {
            created_at_ms: session.created_at_ms,
            last_used_at_ms: session.last_used_at_ms,
            same_instance: session.instance == self.instance,
            token_matches,
        })
    }

    /// Renew the idle timer of an admitted `call` (L4).
    pub(super) fn touch(&mut self, cookie: &str, now_ms: u64) {
        let digest = digest(cookie);
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| bool::from(session.cookie[..].ct_eq(&digest[..])))
        {
            session.last_used_at_ms = session.last_used_at_ms.max(now_ms);
        }
    }

    /// Replace the grant of the live session `cookie`; the session's expiry,
    /// or `None` when the session ended meanwhile. A grant of another
    /// service instance is never stored.
    pub(super) fn set_grant(&mut self, cookie: &str, grant: Grant, now_ms: u64) -> Option<u64> {
        self.purge(now_ms);
        if grant.instance != self.instance {
            return None;
        }
        let digest = digest(cookie);
        let session = self
            .sessions
            .iter_mut()
            .find(|session| bool::from(session.cookie[..].ct_eq(&digest[..])))?;
        if session.instance != self.instance {
            return None;
        }
        session.grant = Some(grant);
        Some(session.expires_at())
    }

    /// The grant of the live session `cookie`, if this instance checked it.
    pub(super) fn grant(&mut self, cookie: &str, now_ms: u64) -> Option<Grant> {
        self.purge(now_ms);
        let session = self.find(cookie)?;
        let grant = session.grant.as_ref()?;
        (session.instance == self.instance && grant.instance == self.instance)
            .then(|| grant.clone())
    }

    /// End one session; whether it existed.
    pub(super) fn remove(&mut self, cookie: &str) -> bool {
        let digest = digest(cookie);
        let before = self.sessions.len();
        self.sessions
            .retain(|session| !bool::from(session.cookie[..].ct_eq(&digest[..])));
        self.sessions.len() != before
    }

    /// Drop sessions past their idle or absolute expiry.
    pub(super) fn purge(&mut self, now_ms: u64) {
        self.sessions.retain(|session| !session.expired(now_ms));
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.sessions.len()
    }
}
