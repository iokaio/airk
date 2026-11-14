// SPDX-License-Identifier: Apache-2.0
//! In-memory launch codes (A1, contracts §2.7) and admin sessions (§4.6).
//!
//! Pure state: every operation takes the injected clock reading and the
//! random bytes from its caller. Only SHA-256 digests of the secrets are
//! kept; the secret text exists only in the reply that issues it. Nothing
//! here implements `Debug`, so no secret or digest can reach a log.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::CatalogRevision,
    scoped_reads::SelectionScopeRef,
    service::WindowScopeSelection,
};
use airk_endpoint::admin_contract::{
    AdminCode, LAUNCH_CODE_TTL_MS, LaunchCodeFacts, MAX_REVIEWS_PER_SESSION, MAX_SESSIONS,
    SESSION_ABSOLUTE_MS, SESSION_IDLE_MS, SessionFacts,
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use subtle::ConstantTimeEq;

/// At most this many launch codes are outstanding; the oldest is evicted.
pub(super) const MAX_LAUNCH_CODES: usize = airk_endpoint::admin_contract::MAX_LAUNCH_CODES;

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
    pub(super) purpose: ReviewPurpose,
}

/// Pending custody and uncertain recovery cannot be evicted by ordinary
/// preview pressure. Nothing here changes the lifetime or the shared budget.
#[derive(Clone, Copy)]
pub(super) enum ReviewPurpose {
    Pending,
    Prepared,
    Acknowledged(Option<&'static str>),
    Ordinary,
    ReceiptOnly(&'static str),
}

#[derive(Clone)]
pub(super) enum PreparedState {
    Ready,
    Submitted,
    Known {
        status: u16,
        envelope: serde_json::Value,
    },
    Unknown,
    BodyUnavailable {
        refused: bool,
    },
}

#[derive(Clone)]
pub(super) struct PreparedMutation {
    pub(super) token: Incarnation,
    pub(super) operation: String,
    pub(super) method: airk_core::service::ControlMethod,
    pub(super) key: super::mutations::Key,
    pub(super) receipt_operation: Option<&'static str>,
    pub(super) receipt_method: Option<airk_core::service::ControlMethod>,
    pub(super) state: PreparedState,
}

struct Session {
    life: SessionLife,
    cookie: [u8; 32],
    csrf: [u8; 32],
    created_at_ms: u64,
    last_used_at_ms: u64,
    instance: Incarnation,
    grant: Option<Grant>,
    reviews: VecDeque<Review>,
    preparations: Vec<PreparedMutation>,
    acknowledgments: VecDeque<(Incarnation, String)>,
    streams: Vec<Arc<AtomicU8>>,
    event_cursors: VecDeque<(Incarnation, airk_core::service_events::EventCursor)>,
}

struct SessionLife(Arc<AtomicBool>);
impl Drop for SessionLife {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Pure native admission observation: it may be checked under owning service
/// gates without acquiring the session mutex or reversing its lock order.
#[derive(Clone)]
pub(super) struct SessionPermit {
    live: Arc<AtomicBool>,
    expires_at_ms: u64,
}
impl SessionPermit {
    pub(super) fn valid(&self, now_ms: u64) -> bool {
        self.live.load(Ordering::Acquire) && now_ms < self.expires_at_ms
    }
}

impl Session {
    fn end_streams(&self, reason: u8) {
        self.life.0.store(false, Ordering::Release);
        for stream in &self.streams {
            stream.store(reason, Ordering::SeqCst);
        }
    }
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
            session.end_streams(3);
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
                evicted.end_streams(2);
                self.drop_grant(evicted.grant);
            }
        }
        self.sessions.push_back(Session {
            life: SessionLife(Arc::new(AtomicBool::new(true))),
            cookie: digest(&minted.cookie),
            csrf: digest(&minted.csrf),
            created_at_ms: now_ms,
            last_used_at_ms: now_ms,
            instance: self.instance,
            grant: None,
            reviews: VecDeque::new(),
            preparations: Vec::new(),
            acknowledgments: VecDeque::new(),
            streams: Vec::new(),
            event_cursors: VecDeque::new(),
        });
        minted
    }

    fn find(&self, cookie: &str) -> Option<&Session> {
        let digest = digest(cookie);
        self.sessions
            .iter()
            .find(|session| bool::from(session.cookie[..].ct_eq(&digest[..])))
    }

    fn find_mut(&mut self, cookie: &str) -> Option<&mut Session> {
        let digest = digest(cookie);
        let instance = self.instance;
        self.sessions.iter_mut().find(|session| {
            session.instance == instance && bool::from(session.cookie[..].ct_eq(&digest[..]))
        })
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

    /// Validate actual cookie/CSRF facts before any owning gate, then renew
    /// idle lifetime once at native call admission and freeze its deadline.
    pub(super) fn native_permit(
        &mut self,
        cookie: &str,
        csrf: &str,
        now_ms: u64,
    ) -> Option<SessionPermit> {
        let facts = self.facts(&[cookie], &[csrf], now_ms)?;
        if !facts.same_instance || !facts.token_matches {
            return None;
        }
        self.touch(cookie, now_ms);
        let session = self.find(cookie)?;
        Some(SessionPermit {
            live: session.life.0.clone(),
            expires_at_ms: session.expires_at(),
        })
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

    /// Reserve custody before dispatch. Make room only by dropping the oldest
    /// ordinary token; pending and receipt-only identities remain pinned.
    pub(super) fn reserve_review(
        &mut self,
        cookie: &str,
        review: Review,
        now_ms: u64,
    ) -> Result<(), AdminCode> {
        self.purge(now_ms);
        let digest = digest(cookie);
        let session = self
            .sessions
            .iter_mut()
            .find(|session| bool::from(session.cookie[..].ct_eq(&digest[..])))
            .ok_or(AdminCode::AdminSessionInvalid)?;
        if session.instance != self.instance {
            return Err(AdminCode::AdminSessionInvalid);
        }
        if session
            .reviews
            .iter()
            .any(|stored| stored.token == review.token)
        {
            return Err(AdminCode::AdminInternal);
        }
        while session.reviews.len() >= MAX_REVIEWS_PER_SESSION {
            let oldest = session
                .reviews
                .iter()
                .position(|review| {
                    matches!(
                        review.purpose,
                        ReviewPurpose::Ordinary | ReviewPurpose::Acknowledged(_)
                    )
                })
                .ok_or(AdminCode::AdminReviewCapacity)?;
            if let Some(review) = session.reviews.remove(oldest) {
                session
                    .preparations
                    .retain(|prepared| prepared.token != review.token);
            }
        }
        session.reviews.push_back(review);
        Ok(())
    }

    /// Settle an exact pending reservation without changing its token/key.
    /// A key-only draft obtains its native caller from the response here.
    pub(super) fn finish_review(
        &mut self,
        cookie: &str,
        token: Incarnation,
        caller: Option<serde_json::Value>,
        purpose: ReviewPurpose,
        now_ms: u64,
    ) -> Option<()> {
        self.purge(now_ms);
        let cookie = digest(cookie);
        let instance = self.instance;
        let session = self.sessions.iter_mut().find(|session| {
            session.instance == instance && bool::from(session.cookie[..].ct_eq(&cookie[..]))
        })?;
        let review = session.reviews.iter_mut().find(|review| {
            review.token == token && matches!(review.purpose, ReviewPurpose::Pending)
        })?;
        review.caller = caller;
        review.purpose = purpose;
        Some(())
    }

    /// Release only an unused pending slot. A completed ordinary review or
    /// a pinned unknown identity cannot be removed by a late cleanup guard.
    pub(super) fn release_review(&mut self, cookie: &str, token: Incarnation, now_ms: u64) {
        self.purge(now_ms);
        let cookie = digest(cookie);
        let instance = self.instance;
        if let Some(session) = self.sessions.iter_mut().find(|session| {
            session.instance == instance && bool::from(session.cookie[..].ct_eq(&cookie[..]))
        }) {
            session.reviews.retain(|review| {
                review.token != token || !matches!(review.purpose, ReviewPurpose::Pending)
            });
        }
    }

    /// The identity `token` names in the live session `cookie`. Tokens are
    /// not consumed: a repeated apply carries the same key (U98).
    pub(super) fn review(
        &mut self,
        cookie: &str,
        token: &Incarnation,
        operation: &str,
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
            .find(|review| {
                review.token == *token
                    && match review.purpose {
                        ReviewPurpose::Ordinary => true,
                        ReviewPurpose::ReceiptOnly(receipt) => operation == receipt,
                        ReviewPurpose::Acknowledged(receipt) => receipt == Some(operation),
                        ReviewPurpose::Pending | ReviewPurpose::Prepared => false,
                    }
            })
            .cloned()
    }

    /// Convert an already budgeted pending review slot into acknowledged,
    /// immutable preparation custody. Both records share one pinned slot.
    pub(super) fn store_preparation(
        &mut self,
        cookie: &str,
        prepared: PreparedMutation,
        now_ms: u64,
    ) -> Result<(), AdminCode> {
        self.purge(now_ms);
        let session = self
            .find_mut(cookie)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        let review = session
            .reviews
            .iter_mut()
            .find(|review| {
                review.token == prepared.token && matches!(review.purpose, ReviewPurpose::Pending)
            })
            .ok_or(AdminCode::AdminInternal)?;
        review.purpose = ReviewPurpose::Prepared;
        session.preparations.push(prepared);
        Ok(())
    }

    /// Claim at most once under the same mutex as session lifetime validation.
    /// The returned Ready snapshot is the only permission to dispatch.
    pub(super) fn preparation(
        &mut self,
        cookie: &str,
        token: Incarnation,
        operation: &str,
        submit: bool,
        now_ms: u64,
    ) -> Result<PreparedMutation, AdminCode> {
        self.purge(now_ms);
        let session = self
            .find_mut(cookie)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        let prepared = session
            .preparations
            .iter_mut()
            .find(|prepared| prepared.token == token && prepared.operation == operation)
            .ok_or(AdminCode::AdminReviewUnknown)?;
        let snapshot = prepared.clone();
        if submit && matches!(prepared.state, PreparedState::Ready) {
            prepared.state = PreparedState::Submitted;
        }
        Ok(snapshot)
    }

    pub(super) fn settle_preparation(
        &mut self,
        cookie: &str,
        token: Incarnation,
        state: PreparedState,
        recovered: bool,
        now_ms: u64,
    ) -> Result<(), AdminCode> {
        self.purge(now_ms);
        let session = self
            .find_mut(cookie)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        let prepared = session
            .preparations
            .iter_mut()
            .find(|prepared| prepared.token == token)
            .ok_or(AdminCode::AdminReviewUnknown)?;
        if matches!(prepared.state, PreparedState::Submitted)
            || (recovered
                && matches!(
                    prepared.state,
                    PreparedState::Unknown | PreparedState::BodyUnavailable { .. }
                ))
        {
            prepared.state = state;
        }
        Ok(())
    }

    pub(super) fn acknowledge_preparation(
        &mut self,
        cookie: &str,
        token: Incarnation,
        operation: &str,
        now_ms: u64,
    ) -> Result<(), AdminCode> {
        self.purge(now_ms);
        let session = self
            .find_mut(cookie)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        if session
            .acknowledgments
            .iter()
            .any(|(stored, stored_operation)| *stored == token && stored_operation == operation)
        {
            return Ok(());
        }
        let prepared = session
            .preparations
            .iter()
            .find(|prepared| prepared.token == token && prepared.operation == operation)
            .ok_or(AdminCode::AdminReviewUnknown)?;
        let refused = match &prepared.state {
            PreparedState::Known { envelope, .. } => {
                envelope.get("ok") == Some(&serde_json::Value::Bool(false))
            }
            PreparedState::BodyUnavailable { refused } => *refused,
            PreparedState::Ready | PreparedState::Submitted | PreparedState::Unknown => {
                return Err(AdminCode::AdminReviewUnknown);
            }
        };
        let receipt = prepared.receipt_operation;
        if refused {
            session
                .preparations
                .retain(|prepared| prepared.token != token);
            session.reviews.retain(|review| review.token != token);
        } else {
            let review = session
                .reviews
                .iter_mut()
                .find(|review| review.token == token)
                .ok_or(AdminCode::AdminInternal)?;
            review.purpose = ReviewPurpose::Acknowledged(receipt);
        }
        while session.acknowledgments.len() >= MAX_REVIEWS_PER_SESSION {
            session.acknowledgments.pop_front();
        }
        session
            .acknowledgments
            .push_back((token, operation.to_owned()));
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn preparation_count(&self, cookie: &str) -> usize {
        self.find(cookie)
            .map_or(0, |session| session.preparations.len())
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
            session.end_streams(2);
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
            session.end_streams(1);
            self.drop_grant(session.grant);
        }
    }

    pub(super) fn reserve_stream(
        &mut self,
        cookie: &str,
        cursor: Option<Incarnation>,
        now_ms: u64,
    ) -> Result<
        (
            Arc<AtomicU8>,
            Option<airk_core::service_events::EventCursor>,
            bool,
        ),
        AdminCode,
    > {
        self.purge(now_ms);
        if self
            .sessions
            .iter()
            .map(|session| session.streams.len())
            .sum::<usize>()
            >= airk_endpoint::admin_contract::MAX_STREAMS
        {
            return Err(AdminCode::AdminStreamLimit);
        }
        let session = self
            .find_mut(cookie)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        if session.streams.len() >= airk_endpoint::admin_contract::MAX_STREAMS_PER_SESSION {
            return Err(AdminCode::AdminStreamLimit);
        }
        let native = cursor.and_then(|token| {
            session
                .event_cursors
                .iter()
                .find(|(stored, _)| *stored == token)
                .map(|(_, cursor)| cursor.clone())
        });
        let lost = cursor.is_some() && native.is_none();
        let lease = Arc::new(AtomicU8::new(0));
        session.streams.push(lease.clone());
        Ok((lease, native, lost))
    }

    pub(super) fn stream_live(&mut self, cookie: &str, now_ms: u64) -> bool {
        self.purge(now_ms);
        self.find(cookie).is_some()
    }

    pub(super) fn release_stream(&mut self, cookie: &str, lease: &Arc<AtomicU8>) {
        if let Some(session) = self.find_mut(cookie) {
            session.streams.retain(|stored| !Arc::ptr_eq(stored, lease));
        }
    }

    pub(super) fn store_event_cursor(
        &mut self,
        cookie: &str,
        token: Incarnation,
        cursor: airk_core::service_events::EventCursor,
        now_ms: u64,
    ) -> Result<(), AdminCode> {
        self.purge(now_ms);
        let session = self
            .find_mut(cookie)
            .ok_or(AdminCode::AdminSessionInvalid)?;
        while session.event_cursors.len() >= airk_endpoint::admin_contract::STREAM_BUFFER_EVENTS {
            session.event_cursors.pop_front();
        }
        session.event_cursors.push_back((token, cursor));
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Observe the shared review budget in handler regressions without
    /// exposing any stored identity or token.
    #[cfg(test)]
    pub(super) fn review_count(&self, cookie: &str) -> usize {
        let cookie = digest(cookie);
        self.sessions
            .iter()
            .find(|session| bool::from(session.cookie[..].ct_eq(&cookie[..])))
            .map_or(0, |session| session.reviews.len())
    }
}
