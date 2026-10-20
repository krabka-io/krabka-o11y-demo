//! Bounded producer scheduling. A key stays occupied until all retries finish.

use std::{collections::HashSet, num::NonZeroUsize, time::Duration};

use observability_demo_app::{Order, order_at};
use tokio::{task::JoinSet, time::Instant};

#[derive(Debug, Default)]
pub(super) struct SendReport {
    pub acknowledged: u64,
    pub failed: u64,
}

impl SendReport {
    fn record(&mut self, success: bool) {
        if success {
            self.acknowledged += 1;
        } else {
            self.failed += 1;
        }
    }

    fn achieved_rate(&self, elapsed: Duration) -> f64 {
        // Keep the counter integral; convert through Duration for rate arithmetic.
        Duration::from_secs(self.acknowledged).as_secs_f64()
            / elapsed.as_secs_f64().max(f64::EPSILON)
    }

    fn log(&self, period: Duration, elapsed: Duration, in_flight: usize) {
        tracing::info!(
            requested_orders_per_sec = 1.0 / period.as_secs_f64(),
            achieved_orders_per_sec = self.achieved_rate(elapsed),
            acknowledged = self.acknowledged,
            failed = self.failed,
            in_flight,
            "producer delivery rates"
        );
    }
}

pub(super) async fn send_orders<F>(
    period: Duration,
    max_in_flight: NonZeroUsize,
    shutdown: impl Future<Output = ()>,
    mut send: impl FnMut(Order) -> F,
) -> Result<SendReport, tokio::task::JoinError>
where
    F: Future<Output = bool> + Send + 'static,
{
    let start = Instant::now();
    let mut tick = tokio::time::interval(period);
    // Backpressure must not cause a burst of catch-up orders.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut reporting =
        tokio::time::interval_at(start + Duration::from_secs(5), Duration::from_secs(5));
    let mut pending = JoinSet::new();
    let mut active_keys = HashSet::new();
    let mut report = SendReport::default();
    let mut index = 0;
    let mut next = order_at(index);
    let mut task_error = None;
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            biased;
            () = &mut shutdown => break,
            result = pending.join_next(), if !pending.is_empty() => {
                match result.expect("nonempty send set") {
                    Ok((key, success)) => {
                        active_keys.remove(&key);
                        report.record(success);
                    }
                    Err(error) => {
                        task_error = Some(error);
                        break;
                    }
                }
            }
            _ = reporting.tick() => report.log(period, start.elapsed(), pending.len()),
            _ = tick.tick(), if pending.len() < max_in_flight.get()
                && !active_keys.contains(&next.category) => {
                let key = next.category.clone();
                active_keys.insert(key.clone());
                next.ts_ms = i64::try_from(index).unwrap_or(i64::MAX);
                let delivery = send(next);
                pending.spawn(async move { (key, delivery.await) });
                index += 1;
                next = order_at(index);
            }
        }
    }
    // Stop admitting orders, but keep every admitted send and retry alive.
    while let Some(result) = pending.join_next().await {
        match result {
            Ok((_, success)) => report.record(success),
            Err(error) => task_error = Some(error),
        }
    }
    report.log(period, start.elapsed(), 0);
    task_error.map_or(Ok(report), Err)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct FakeProducer {
        active: HashSet<String>,
        maximum: usize,
        started: Vec<Order>,
        completed: Vec<Order>,
    }

    fn delayed_send(
        state: Arc<Mutex<FakeProducer>>,
        order: Order,
        delay: Duration,
        success: bool,
    ) -> impl Future<Output = bool> + Send {
        {
            let mut state = state.lock().unwrap();
            assert2::assert!(state.active.insert(order.category.clone()));
            state.maximum = state.maximum.max(state.active.len());
            state.started.push(order.clone());
        }
        async move {
            tokio::time::sleep(delay).await;
            let mut state = state.lock().unwrap();
            assert2::assert!(state.active.remove(&order.category));
            state.completed.push(order);
            success
        }
    }

    #[tokio::test(start_paused = true)]
    async fn delayed_producer_bounds_concurrency_and_reaches_requested_rate() {
        for limit in [1, 2, 5, 10] {
            let state = Arc::new(Mutex::new(FakeProducer::default()));
            let start = Instant::now();
            let report = send_orders(
                Duration::from_millis(20), // requested 50 orders/s
                NonZeroUsize::new(limit).unwrap(),
                tokio::time::sleep(Duration::from_secs(1)),
                |order| delayed_send(Arc::clone(&state), order, Duration::from_millis(50), true),
            )
            .await
            .unwrap();
            let state = state.lock().unwrap();
            assert2::assert!(state.maximum == limit.min(3));
            assert2::assert!(state.active.is_empty());
            assert2::assert!(state.started.len() == state.completed.len());
            assert2::assert!(report.failed == 0);
            if limit == 1 {
                assert2::assert!(report.achieved_rate(start.elapsed()) <= 20.0);
            } else if limit >= 5 {
                assert2::assert!(report.acknowledged == 50);
                assert2::assert!(report.achieved_rate(start.elapsed()) > 48.0);
            }
            for (index, actual) in state.started.iter().enumerate() {
                let mut expected = order_at(u64::try_from(index).unwrap());
                expected.ts_ms = i64::try_from(index).unwrap();
                assert2::assert!(actual == &expected);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn slow_key_stays_ordered_and_shutdown_drains_failures_and_successes() {
        let state = Arc::new(Mutex::new(FakeProducer::default()));
        let start = Instant::now();
        let report = send_orders(
            Duration::from_millis(5),
            NonZeroUsize::new(5).unwrap(),
            tokio::time::sleep(Duration::from_millis(225)),
            |order| {
                let delay = if order.category == "books" { 200 } else { 20 };
                let success = order.category != "books";
                delayed_send(
                    Arc::clone(&state),
                    order,
                    Duration::from_millis(delay),
                    success,
                )
            },
        )
        .await
        .unwrap();
        let state = state.lock().unwrap();
        assert2::assert!(state.active.is_empty());
        assert2::assert!(state.maximum <= 5);
        // The second books order starts only after the first completes at 200ms.
        // Shutdown at 225ms waits for this send to fail at 400ms.
        assert2::assert!(start.elapsed() == Duration::from_millis(400));
        assert2::assert!(state.started.len() == 10);
        assert2::assert!(state.completed.len() == 10);
        assert2::assert!(report.acknowledged == 8);
        assert2::assert!(report.failed == 2);
        assert2::assert!((report.achieved_rate(Duration::from_secs(2)) - 4.0).abs() < f64::EPSILON);
        for category in observability_demo_app::CATEGORIES {
            let ids = |orders: &[Order]| {
                orders
                    .iter()
                    .filter(|order| order.category == *category)
                    .map(|order| order.order_id.clone())
                    .collect::<Vec<_>>()
            };
            assert2::assert!(ids(&state.started) == ids(&state.completed));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ready_shutdown_admits_no_orders() {
        let report = send_orders(
            Duration::from_millis(20),
            NonZeroUsize::new(5).unwrap(),
            std::future::ready(()),
            |_| async { panic!("send after shutdown") },
        )
        .await
        .unwrap();
        assert2::assert!(report.acknowledged == 0 && report.failed == 0);
    }
}
