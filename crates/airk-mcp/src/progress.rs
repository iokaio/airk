// SPDX-License-Identifier: Apache-2.0
//! MCP progress notifications for `airk_answer` (final plan 12.4 and 12.4.1
//! item 5). The Answer driver reports its part, round, stage and sub-needs
//! covered through an [`AnswerProgressSink`]; the reporter rate-limits them
//! and sends `notifications/progress` on the request's progress token.
//! Progress is advisory: it never changes admission, deadlines or the reply,
//! and a client that did not ask for progress (no token) gets none.
use rmcp::{
    Peer, RoleServer,
    model::{ProgressNotificationParam, ProgressToken, RequestMetaObject},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// At most one notification in this interval; a newer report replaces a
/// pending one, and the next heartbeat sends it.
pub const MIN_PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
/// While an Answer runs with no newer report, one notification is sent this
/// often so a client's idle timer sees the call advancing.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// The stage an Answer part is in (plan section 12.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerStage {
    Queued,
    Split,
    Plan,
    Gather,
    Coverage,
    FollowUp,
    Compose,
    Union,
}
impl AnswerStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Split => "split",
            Self::Plan => "plan",
            Self::Gather => "gather",
            Self::Coverage => "coverage",
            Self::FollowUp => "follow_up",
            Self::Compose => "compose",
            Self::Union => "union",
        }
    }
}

/// One progress report from the Answer driver. `part` and `round` count from
/// 1; 0 means not yet known (Split, or a stage across all parts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnswerProgress {
    pub part: u8,
    pub parts: u8,
    pub round: u8,
    pub stage: AnswerStage,
    pub sub_needs_covered: u8,
    pub sub_needs: u8,
}
impl AnswerProgress {
    /// The notification text, for example
    /// `part 2/4, round 1, coverage, sub-needs covered 3/4`.
    pub fn message(&self) -> String {
        let mut text = String::new();
        if self.part > 0 {
            text.push_str(&format!("part {}/{}, ", self.part, self.parts));
        }
        if self.round > 0 {
            text.push_str(&format!("round {}, ", self.round));
        }
        text.push_str(self.stage.as_str());
        if self.sub_needs > 0 {
            text.push_str(&format!(
                ", sub-needs covered {}/{}",
                self.sub_needs_covered, self.sub_needs
            ));
        }
        text
    }
}

/// The callback the Answer driver calls. It is synchronous, nonblocking and
/// infallible, so the driver may call it from any thread at any stage.
pub type AnswerProgressSink = Arc<dyn Fn(AnswerProgress) + Send + Sync>;

/// A sink that drops every report, for calls without a progress token.
pub fn no_progress() -> AnswerProgressSink {
    Arc::new(|_| {})
}

/// Sends one notification; never blocks.
pub type NotifyFn = Arc<dyn Fn(ProgressNotificationParam) + Send + Sync>;
/// The reporter's clock, injected for tests.
pub type ProgressClock = Arc<dyn Fn() -> Instant + Send + Sync>;

struct State {
    started: Instant,
    last_sent: Option<Instant>,
    pending: Option<AnswerProgress>,
    latest: Option<AnswerProgress>,
    step: u64,
}

/// Rate-limited progress for one request's progress token.
pub struct ProgressReporter {
    token: ProgressToken,
    send: NotifyFn,
    clock: ProgressClock,
    state: Mutex<State>,
}

impl ProgressReporter {
    /// The reporter for one MCP request, or `None` when the client sent no
    /// progress token. Must be called inside a Tokio runtime: notifications
    /// are sent in order by one task that ends when the reporter is dropped.
    pub fn from_request(meta: &RequestMetaObject, peer: Peer<RoleServer>) -> Option<Arc<Self>> {
        let token = meta.get_progress_token()?;
        let (sender, mut receiver) =
            tokio::sync::mpsc::unbounded_channel::<ProgressNotificationParam>();
        tokio::spawn(async move {
            while let Some(param) = receiver.recv().await {
                // A failed send (the client went away) never affects the call.
                if peer.notify_progress(param).await.is_err() {
                    break;
                }
            }
        });
        Some(Self::with_sender(
            token,
            Arc::new(move |param| {
                let _ = sender.send(param);
            }),
            Arc::new(Instant::now),
        ))
    }

    /// A reporter with an injected sender and clock (tests and other
    /// transports).
    pub fn with_sender(token: ProgressToken, send: NotifyFn, clock: ProgressClock) -> Arc<Self> {
        let started = clock();
        Arc::new(Self {
            token,
            send,
            clock,
            state: Mutex::new(State {
                started,
                last_sent: None,
                pending: None,
                latest: None,
                step: 0,
            }),
        })
    }

    /// The driver's callback for this reporter.
    pub fn sink(self: &Arc<Self>) -> AnswerProgressSink {
        let reporter = self.clone();
        Arc::new(move |progress| reporter.report(progress))
    }

    /// Send now, unless the last notification was less than
    /// [`MIN_PROGRESS_INTERVAL`] ago; then keep it for the next heartbeat.
    pub fn report(&self, progress: AnswerProgress) {
        let now = (self.clock)();
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.latest = Some(progress);
        if state
            .last_sent
            .is_some_and(|last| now.saturating_duration_since(last) < MIN_PROGRESS_INTERVAL)
        {
            state.pending = Some(progress);
            return;
        }
        state.pending = None;
        self.emit(&mut state, now, progress.message());
    }

    /// Send the pending report, or, when nothing was sent for a
    /// [`HEARTBEAT_INTERVAL`], the latest stage with the elapsed time.
    pub fn heartbeat(&self) {
        let now = (self.clock)();
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if let Some(progress) = state.pending.take() {
            if state
                .last_sent
                .is_none_or(|last| now.saturating_duration_since(last) >= MIN_PROGRESS_INTERVAL)
            {
                self.emit(&mut state, now, progress.message());
            } else {
                state.pending = Some(progress);
            }
            return;
        }
        if state
            .last_sent
            .is_some_and(|last| now.saturating_duration_since(last) < HEARTBEAT_INTERVAL)
        {
            return;
        }
        let elapsed = now.saturating_duration_since(state.started).as_secs();
        let message = match state.latest {
            Some(progress) => format!("{}; {elapsed} s elapsed", progress.message()),
            None => format!("Answer running; {elapsed} s elapsed"),
        };
        self.emit(&mut state, now, message);
    }

    /// Run `future`, sending a heartbeat every [`HEARTBEAT_INTERVAL`] (and any
    /// pending report each second) until it completes.
    pub async fn while_running<F: std::future::Future>(self: &Arc<Self>, future: F) -> F::Output {
        let mut ticks = tokio::time::interval(MIN_PROGRESS_INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticks.tick().await;
        tokio::pin!(future);
        loop {
            tokio::select! {
                biased;
                output = &mut future => return output,
                _ = ticks.tick() => self.heartbeat(),
            }
        }
    }

    fn emit(&self, state: &mut State, now: Instant, message: String) {
        // Progress increases with every notification, as the specification asks.
        state.step += 1;
        state.last_sent = Some(now);
        (self.send)(
            ProgressNotificationParam::new(self.token.clone(), state.step as f64)
                .with_message(message),
        );
    }
}

#[cfg(test)]
mod tests;
