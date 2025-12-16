pub mod izettle_bridge_poll;
pub mod izettle_bridge_result;
pub mod izettle_transaction;
pub mod izettle_transaction_poll;

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::timeout;

/// A shared state struct that enables long-polling for izettle transactions
#[derive(Default)]
pub struct IZettleNotifier {
    inner: Arc<InnerNotifier>,
}

#[derive(Default)]
struct InnerNotifier {
    ticker: AtomicU64,
    notifier: Notify,
}

impl IZettleNotifier {
    /// Call notify to notify waiters that a pending transaction is ready
    pub fn notify(&self) {
        self.inner.ticker.fetch_add(1, Ordering::SeqCst);
        self.inner.notifier.notify_waiters();
    }

    /// Wait for a maximum of `millis` for a pending transaction
    ///
    /// Returns true is the wait was ended by a call to notify, otherwise returns false
    pub fn wait(&self, duration: Duration) -> impl Future<Output = bool> + 'static {
        let state = Arc::clone(&self.inner);

        let start_tick = state.ticker.load(Ordering::SeqCst);

        async move {
            let notified = state.notifier.notified();

            let has_ticked = || state.ticker.load(Ordering::SeqCst) != start_tick;

            if has_ticked() {
                true
            } else {
                timeout(duration, notified).await.is_ok() || has_ticked()
            }
        }
    }
}
