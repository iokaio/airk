// SPDX-License-Identifier: Apache-2.0
//! In-memory launch codes (A1, contracts §2.7) and admin sessions (§4.6).
//!
//! Pure state: every operation takes the injected clock reading and the
//! random bytes from its caller. Only SHA-256 digests of the secrets are
//! kept; the secret text exists only in the reply that issues it. Nothing
//! here implements `Debug`, so no secret or digest can reach a log.
use crate::admin_contract::{
    LAUNCH_CODE_TTL_MS, LaunchCodeFacts, MAX_REVIEWS_PER_SESSION, MAX_SESSIONS,
    SESSION_ABSOLUTE_MS, SESSION_IDLE_MS, SessionFacts,
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::CatalogRevision,
    scoped_reads::SelectionScopeRef,
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
    /// The in-memory read authority of this grant (selection authority
    /// §3.1), whose nonce is live with the scoped-read admission gate while
    /// the grant is stored; `None` when the plane has no selection authority.
    /// Never sent to the browser or the pipe.
    pub(super) selection_ref: Option<SelectionScopeRef>,
}

/// A request identity a generate-mode row issued (contracts §4.3b, U98):
/// held only inside its session under a review token, so the browser never
/// holds a request key. Logout, expiry, eviction and restart drop it.
#[derive(Clone)]
pub(super) struct Review {
    pub(super) token: Incarnation,
    /// The caller beside the key (generated, or the native caller the
    /// result carried); `None` when the row names no caller.
    pub(super) caller: Option<serde_json::Value>,
    pub(super) key: serde_json::Value,
}

struct Session {
    cookie: [u8; 32],
    csrf: [u8; 32],
    created_at_ms: u64,
    last_used_at_ms: u64,
    instance: Incarnation,
    grant: Option<Grant>,
    reviews: VecDeque<Review>,
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
    /// The selection nonces of the grants dropped since the last
    /// [`Self::take_dropped`]: by replacement, logout, expiry, eviction, a
    /// store that found its session gone, or service end (§3.3, N4). The
    /// owner revokes them after it releases its lock (§3.7).
    dropped: Vec<Incarnation>,
}

impl Sessions {
    pub(super) fn new(instance: Incarnation) -> Self {
        Self {
            instance,
            sessions: VecDeque::new(),
            dropped: Vec::new(),
        }
    }

    fn drop_grant(&mut self, grant: Option<Grant>) {
        if let Some(selection) = grant.and_then(|grant| grant.selection_ref) {
            self.dropped.push(selection.grant_nonce);
        }
    }

    /// The nonces dropped since the last call; the caller revokes each one
    /// with no lock held.
    #[must_use = "revoke every dropped selection nonce with no lock held"]
    pub(super) fn take_dropped(&mut self) -> Vec<Incarnation> {
        std::mem::take(&mut self.dropped)
    }

    /// Service end: every session and its grant ends.
    pub(super) fn end(&mut self) {
        while let Some(session) = self.sessions.pop_front() {
            self.drop_grant(session.grant);
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
            if let Some(evicted) = self.sessions.pop_front() {
                self.drop_grant(evicted.grant);
            }
        }
        self.sessions.push_back(Session {
            cookie: digest(&minted.cookie),
            csrf: digest(&minted.csrf),
            created_at_ms: now_ms,
            last_used_at_ms: now_ms,
            instance: self.instance,
            grant: None,
            reviews: VecDeque::new(),
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
    /// A grant that is not stored, and the grant it replaces, are dropped
    /// (their nonces are revoked by the owner).
    pub(super) fn set_grant(&mut self, cookie: &str, grant: Grant, now_ms: u64) -> Option<u64> {
        self.purge(now_ms);
        let instance = self.instance;
        let digest = digest(cookie);
        let session = self.sessions.iter_mut().find(|session| {
            bool::from(session.cookie[..].ct_eq(&digest[..])) && session.instance == instance
        });
        let Some(session) = session.filter(|_| grant.instance == instance) else {
            self.drop_grant(Some(grant));
            return None;
        };
        let replaced = session.grant.replace(grant);
        let expires_at = session.expires_at();
        self.drop_grant(replaced);
        Some(expires_at)
    }

    /// The grant of the live session `cookie`, if this instance checked it.
    pub(super) fn grant(&mut self, cookie: &str, now_ms: u64) -> Option<Grant> {
        self.purge(now_ms);
        let session = self.find(cookie)?;
        let grant = session.grant.as_ref()?;
        (session.instance == self.instance && grant.instance == self.instance)
            .then(|| grant.clone())
    }

    /// Keep `review` in the live session `cookie` (oldest dropped past
    /// `MAX_REVIEWS_PER_SESSION`); `None` when the session ended meanwhile.
    pub(super) fn add_review(&mut self, cookie: &str, review: Review, now_ms: u64) -> Option<()> {
        self.purge(now_ms);
        let digest = digest(cookie);
        let session = self
            .sessions
            .iter_mut()
            .find(|session| bool::from(session.cookie[..].ct_eq(&digest[..])))?;
        if session.instance != self.instance {
            return None;
        }
        while session.reviews.len() >= MAX_REVIEWS_PER_SESSION {
            session.reviews.pop_front();
        }
        session.reviews.push_back(review);
        Some(())
    }

    /// The identity `token` names in the live session `cookie`. Tokens are
    /// not consumed: a repeated apply carries the same key (U98).
    pub(super) fn review(
        &mut self,
        cookie: &str,
        token: &Incarnation,
        now_ms: u64,
    ) -> Option<Review> {
        self.purge(now_ms);
        let session = self.find(cookie)?;
        if session.instance != self.instance {
            return None;
        }
        session
            .reviews
            .iter()
            .find(|review| review.token == *token)
            .cloned()
    }

    /// End one session; whether it existed.
    pub(super) fn remove(&mut self, cookie: &str) -> bool {
        let digest = digest(cookie);
        let (ended, kept) = std::mem::take(&mut self.sessions)
            .into_iter()
            .partition::<VecDeque<_>, _>(|session| {
                bool::from(session.cookie[..].ct_eq(&digest[..]))
            });
        self.sessions = kept;
        let existed = !ended.is_empty();
        for session in ended {
            self.drop_grant(session.grant);
        }
        existed
    }

    /// Drop sessions past their idle or absolute expiry.
    pub(super) fn purge(&mut self, now_ms: u64) {
        let (expired, live) = std::mem::take(&mut self.sessions)
            .into_iter()
            .partition::<VecDeque<_>, _>(|session| session.expired(now_ms));
        self.sessions = live;
        for session in expired {
            self.drop_grant(session.grant);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.sessions.len()
    }
}
