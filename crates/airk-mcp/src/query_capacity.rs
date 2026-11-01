// SPDX-License-Identifier: Apache-2.0
//! Adjustable bounded capacity. Lowering a limit drains accepted work without revoking it.
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, TryAcquireError};

pub struct QueryCapacity {
    state: Mutex<(usize, usize, bool)>,
    changed: Notify,
    turn: tokio::sync::Mutex<()>,
}
pub struct QueryPermit(Arc<QueryCapacity>);
impl Drop for QueryPermit {
    fn drop(&mut self) {
        self.0.state.lock().unwrap_or_else(|e| e.into_inner()).1 -= 1;
        self.0.changed.notify_waiters();
    }
}
impl QueryCapacity {
    pub fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new((limit, 0, false)),
            changed: Notify::new(),
            turn: tokio::sync::Mutex::new(()),
        })
    }
    pub fn configure(&self, limit: usize) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).0 = limit;
        self.changed.notify_waiters();
    }
    pub fn available_permits(&self) -> usize {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.0.saturating_sub(state.1)
    }
    pub fn close(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).2 = true;
        self.changed.notify_waiters();
    }
    fn take(self: &Arc<Self>) -> Result<QueryPermit, TryAcquireError> {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.2 {
                return Err(TryAcquireError::Closed);
            }
            if state.1 >= state.0 {
                return Err(TryAcquireError::NoPermits);
            }
            state.1 += 1;
        }
        Ok(QueryPermit(self.clone()))
    }
    pub fn try_acquire_owned(self: Arc<Self>) -> Result<QueryPermit, TryAcquireError> {
        let _turn = self
            .turn
            .try_lock()
            .map_err(|_| TryAcquireError::NoPermits)?;
        self.take()
    }
    pub async fn acquire_owned(self: Arc<Self>) -> Result<QueryPermit, TryAcquireError> {
        let _turn = self.turn.lock().await;
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            match self.take() {
                Err(TryAcquireError::NoPermits) => changed.await,
                result => return result,
            }
        }
    }
}
impl std::fmt::Debug for QueryPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QueryPermit")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn configured_capacity_grows_and_shrinks_without_releasing_active_work() {
        for limit in [4, 8, 16, 32, 64] {
            let capacity = QueryCapacity::new(limit);
            let mut held: Vec<_> = (0..limit)
                .map(|_| capacity.clone().try_acquire_owned().unwrap())
                .collect();
            assert!(capacity.clone().try_acquire_owned().is_err());
            capacity.configure(4);
            while held.len() >= 4 {
                drop(held.pop());
            }
            assert_eq!(capacity.available_permits(), 1);
            let last = capacity.clone().acquire_owned().await.unwrap();
            capacity.configure(64);
            assert_eq!(capacity.available_permits(), 60);
            capacity.close();
            assert!(capacity.clone().acquire_owned().await.is_err());
            drop(last);
            drop(held);
        }
    }
}
