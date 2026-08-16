use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;
use tokio::time::Instant;

use super::account::AccountId;
use super::error::QwenError;

#[derive(Default)]
struct SchedulerState {
    cursor: usize,
    busy: HashSet<AccountId>,
    shutting_down: bool,
}

struct SchedulerInner {
    state: Mutex<SchedulerState>,
    available: Notify,
}

/// Allocates account-local network leases in persisted display order.
pub struct RoundRobinScheduler {
    inner: Arc<SchedulerInner>,
}

impl std::fmt::Debug for RoundRobinScheduler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RoundRobinScheduler")
    }
}

impl Default for RoundRobinScheduler {
    fn default() -> Self {
        Self {
            inner: Arc::new(SchedulerInner {
                state: Mutex::new(SchedulerState::default()),
                available: Notify::new(),
            }),
        }
    }
}

impl RoundRobinScheduler {
    pub async fn acquire(
        &self,
        eligible_by_display_order: &[Option<AccountId>],
        deadline: Instant,
    ) -> Result<AccountLease, QwenError> {
        loop {
            // Register first so a release between the scan and wait cannot be lost.
            let notified = self.inner.available.notified();
            match self.try_acquire(eligible_by_display_order) {
                AcquireAttempt::Lease(lease) => return Ok(lease),
                AcquireAttempt::NoEligible => return Err(QwenError::no_healthy_account()),
                AcquireAttempt::AllBusy => {
                    if tokio::time::timeout_at(deadline, notified).await.is_err() {
                        return Err(QwenError::pool_busy_timeout());
                    }
                }
            }
        }
    }

    fn try_acquire(&self, eligible_by_display_order: &[Option<AccountId>]) -> AcquireAttempt {
        if eligible_by_display_order.is_empty()
            || eligible_by_display_order.iter().all(Option::is_none)
        {
            return AcquireAttempt::NoEligible;
        }

        let mut state = self.inner.state.lock().expect("round robin scheduler lock");
        if state.shutting_down {
            return AcquireAttempt::NoEligible;
        }
        let length = eligible_by_display_order.len();
        for offset in 0..length {
            let index = (state.cursor + offset) % length;
            let Some(account_id) = eligible_by_display_order[index].as_ref() else {
                continue;
            };
            if state.busy.insert(account_id.clone()) {
                return AcquireAttempt::Lease(AccountLease {
                    inner: Arc::clone(&self.inner),
                    account_id: account_id.clone(),
                    next_cursor: (index + 1) % length,
                    committed: false,
                });
            }
        }
        AcquireAttempt::AllBusy
    }

    pub fn is_busy(&self, account_id: &AccountId) -> bool {
        self.inner
            .state
            .lock()
            .expect("round robin scheduler lock")
            .busy
            .contains(account_id)
    }

    /// Shutdown invalidates every outstanding permit. No new operation may be scheduled by the
    /// pool after this point, and aborted tasks may release their already-removed permit safely.
    pub fn release_all(&self) {
        let mut state = self.inner.state.lock().expect("round robin scheduler lock");
        state.shutting_down = true;
        state.busy.clear();
        self.inner.available.notify_waiters();
    }

    /// Reserves an otherwise idle account for a local lifecycle operation. Fixed leases never
    /// own a cursor reservation, so management work cannot affect round-robin order.
    pub fn try_acquire_fixed(&self, account_id: &AccountId) -> Result<AccountLease, QwenError> {
        let mut state = self.inner.state.lock().expect("round robin scheduler lock");
        if state.shutting_down {
            return Err(QwenError::no_healthy_account());
        }
        if !state.busy.insert(account_id.clone()) {
            return Err(QwenError::account_busy());
        }
        Ok(AccountLease {
            inner: Arc::clone(&self.inner),
            account_id: account_id.clone(),
            next_cursor: 0,
            committed: true,
        })
    }

    pub fn try_acquire_fixed_if_available(
        &self,
        account_id: &AccountId,
        available: bool,
    ) -> Result<AccountLease, QwenError> {
        if !available {
            return Err(QwenError::no_healthy_account());
        }
        self.try_acquire_fixed(account_id)
    }
}

enum AcquireAttempt {
    Lease(AccountLease),
    NoEligible,
    AllBusy,
}

/// An account-local network-operation permit. Dropping it releases and wakes waiters.
pub struct AccountLease {
    inner: Arc<SchedulerInner>,
    account_id: AccountId,
    next_cursor: usize,
    committed: bool,
}

impl AccountLease {
    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    /// Records the next display position immediately before the real network send.
    pub fn commit_send(&mut self) {
        if self.committed {
            return;
        }
        self.inner
            .state
            .lock()
            .expect("round robin scheduler lock")
            .cursor = self.next_cursor;
        self.committed = true;
    }
}

impl Drop for AccountLease {
    fn drop(&mut self) {
        self.inner
            .state
            .lock()
            .expect("round robin scheduler lock")
            .busy
            .remove(&self.account_id);
        self.inner.available.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn accounts() -> Vec<Option<AccountId>> {
        vec![Some(AccountId::new()), Some(AccountId::new())]
    }

    #[tokio::test]
    async fn committed_leases_follow_display_order_a_b_a() {
        let scheduler = RoundRobinScheduler::default();
        let accounts = accounts();
        let deadline = Instant::now() + Duration::from_secs(1);

        let mut first = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(first.account_id(), accounts[0].as_ref().unwrap());
        first.commit_send();
        drop(first);

        let mut second = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(second.account_id(), accounts[1].as_ref().unwrap());
        second.commit_send();
        drop(second);

        let mut third = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(third.account_id(), accounts[0].as_ref().unwrap());
        third.commit_send();
    }

    #[tokio::test]
    async fn uncommitted_lease_does_not_advance_the_cursor() {
        let scheduler = RoundRobinScheduler::default();
        let accounts = accounts();
        let deadline = Instant::now() + Duration::from_secs(1);

        let first = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(first.account_id(), accounts[0].as_ref().unwrap());
        drop(first);

        let second = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(second.account_id(), accounts[0].as_ref().unwrap());
    }

    #[tokio::test]
    async fn unavailable_display_positions_are_skipped_without_changing_order() {
        let scheduler = RoundRobinScheduler::default();
        let accounts = vec![None, Some(AccountId::new()), Some(AccountId::new())];
        let deadline = Instant::now() + Duration::from_secs(1);

        let mut first = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(first.account_id(), accounts[1].as_ref().unwrap());
        first.commit_send();
        drop(first);

        let mut second = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(second.account_id(), accounts[2].as_ref().unwrap());
        second.commit_send();
        drop(second);

        let third = scheduler.acquire(&accounts, deadline).await.unwrap();
        assert_eq!(third.account_id(), accounts[1].as_ref().unwrap());
    }

    #[tokio::test]
    async fn dropped_lease_wakes_a_waiter_without_double_leasing_the_account() {
        let scheduler = Arc::new(RoundRobinScheduler::default());
        let account = vec![Some(AccountId::new())];
        let first = scheduler
            .acquire(&account, Instant::now() + Duration::from_secs(1))
            .await
            .unwrap();
        let waiting_scheduler = Arc::clone(&scheduler);
        let waiting_account = account.clone();
        let waiter = tokio::spawn(async move {
            waiting_scheduler
                .acquire(&waiting_account, Instant::now() + Duration::from_secs(1))
                .await
                .unwrap()
        });

        tokio::task::yield_now().await;
        drop(first);
        let second = waiter.await.unwrap();
        assert_eq!(second.account_id(), account[0].as_ref().unwrap());
    }

    #[tokio::test]
    async fn all_busy_returns_pool_timeout_code_at_the_operation_deadline() {
        let scheduler = RoundRobinScheduler::default();
        let account = vec![Some(AccountId::new())];
        let first = scheduler
            .acquire(&account, Instant::now() + Duration::from_secs(1))
            .await
            .unwrap();

        let error = match scheduler
            .acquire(&account, Instant::now() + Duration::from_millis(10))
            .await
        {
            Ok(_) => panic!("busy account must not receive a second lease"),
            Err(error) => error,
        };
        assert_eq!(error.code().as_str(), "QW-POOL-007");
        drop(first);
    }
}
