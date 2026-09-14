//! Reusable order-pipeline policy and Kafka header helpers.

use std::time::Duration;

use bytes::Bytes;
use krabka_client_consumer::Header as ConsumerHeader;
use krabka_client_producer::Header;

use crate::Order;

/// Input failures kept distinct so telemetry can route them to bounded labels.
#[derive(Debug, PartialEq, Eq)]
pub enum PipelineInputError<E> {
    MissingValue,
    Deserialize(E),
}

/// Require a record value and deserialize it with the caller's schema codec.
///
/// # Errors
///
/// Returns [`PipelineInputError::MissingValue`] when the Kafka record has no
/// value, or [`PipelineInputError::Deserialize`] when `decode` rejects it.
pub fn decode_order<E>(
    value: Option<&[u8]>,
    decode: impl FnOnce(&[u8]) -> Result<Order, E>,
) -> Result<Order, PipelineInputError<E>> {
    let value = value.ok_or(PipelineInputError::MissingValue)?;
    decode(value).map_err(PipelineInputError::Deserialize)
}

/// The work performed by one processing stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageWork {
    pub validate: Duration,
    pub enrich: Duration,
    pub fraud_check: Duration,
    pub fulfill: Duration,
}

/// Build propagation and business headers for an order.
#[must_use]
pub fn order_headers(
    trace_headers: impl IntoIterator<Item = (String, String)>,
    order: &Order,
) -> Vec<Header> {
    let mut headers: Vec<_> = trace_headers
        .into_iter()
        .map(|(key, value)| Header {
            key,
            value: Some(Bytes::from(value)),
        })
        .collect();
    headers.extend([
        Header {
            key: "x-demo-region".into(),
            value: Some(Bytes::from(order.region.clone())),
        },
        Header {
            key: "x-demo-tier".into(),
            value: Some(Bytes::from(order.customer_tier.clone())),
        },
    ]);
    headers
}

/// Decode a named consumer header. Missing, null, and non-UTF-8 values are absent.
#[must_use]
pub fn header_value<'a>(headers: &'a [ConsumerHeader], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.key == name)
        .and_then(|header| header.value.as_deref())
        .and_then(|value| std::str::from_utf8(value).ok())
}

/// Continue the trace context carried by consumer headers on `span`.
pub fn continue_trace(span: &tracing::Span, headers: &[ConsumerHeader]) {
    krabka_telemetry::propagation::set_remote_parent(
        span,
        headers.iter().map(|header| {
            (
                header.key.as_str(),
                header.value.as_deref().unwrap_or(&[][..]),
            )
        }),
    );
}

/// Return the delay for `stage`, applying deterministic slow-order injection.
#[must_use]
pub fn stage_delay(stage: &str, work: StageWork, order_index: u64, slow_fraction: f64) -> Duration {
    let base = match stage {
        "validate" => work.validate,
        "enrich" => work.enrich,
        "fraud_check" => work.fraud_check,
        "fulfill" => work.fulfill,
        _ => Duration::ZERO,
    };
    let slot = u32::try_from(order_index % 10_000).unwrap_or_default();
    let inject_latency =
        slow_fraction > 0.0 && f64::from(slot) / 10_000.0 < slow_fraction.clamp(0.0, 1.0);
    if inject_latency {
        base + Duration::from_millis(30)
    } else {
        base
    }
}

/// Visit every stage in processing order and return the terminal outcome.
pub async fn process_order<F, Fut>(order: &Order, mut run_stage: F) -> &'static str
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = ()>,
{
    for stage in ["validate", "enrich", "fraud_check", "fulfill"] {
        run_stage(stage).await;
    }
    crate::classify_outcome(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check, check_eq};

    #[test]
    fn headers_cover_trace_business_missing_and_invalid_values() {
        let order = crate::order_at(7);
        let produced = order_headers([("traceparent".into(), "00-trace-parent".into())], &order);
        check!(produced.iter().any(|header| header.key == "traceparent"));
        check!(produced.iter().any(|header| header.key == "x-demo-region"));

        let consumed = [
            ConsumerHeader {
                key: "valid".into(),
                value: Some(Bytes::from_static(b"value")),
            },
            ConsumerHeader {
                key: "invalid".into(),
                value: Some(Bytes::from_static(&[0xff])),
            },
            ConsumerHeader {
                key: "null".into(),
                value: None,
            },
        ];
        check_eq!(header_value(&consumed, "valid"), Some("value"));
        check_eq!(header_value(&consumed, "invalid"), None);
        check_eq!(header_value(&consumed, "null"), None);
        check_eq!(header_value(&consumed, "missing"), None);
    }

    #[test]
    fn delays_are_stage_specific_and_seeded_slow_orders_cross_25ms() {
        let work = StageWork {
            validate: Duration::from_millis(1),
            enrich: Duration::from_millis(2),
            fraud_check: Duration::from_millis(3),
            fulfill: Duration::from_millis(4),
        };
        check_eq!(
            stage_delay("validate", work, 9999, 0.0),
            Duration::from_millis(1)
        );
        check!(stage_delay("fulfill", work, 0, 0.01) > Duration::from_millis(25));
        check_eq!(stage_delay("unknown", work, 9999, 0.0), Duration::ZERO);
    }

    #[test]
    fn decode_distinguishes_missing_values_and_deserialization_failures() {
        check_eq!(
            decode_order::<&str>(None, |_| unreachable!()),
            Err(PipelineInputError::MissingValue)
        );
        check_eq!(
            decode_order(Some(b"bad"), |_| Err("invalid protobuf")),
            Err(PipelineInputError::Deserialize("invalid protobuf"))
        );
        check_eq!(
            decode_order::<&str>(Some(b"ok"), |_| Ok(crate::order_at(1))),
            Ok(crate::order_at(1))
        );
    }

    #[tokio::test]
    async fn processing_visits_all_stages_and_returns_outcome() {
        use std::sync::{Arc, Mutex};
        let visited = Arc::new(Mutex::new(Vec::new()));
        let target = Arc::clone(&visited);
        let outcome = process_order(&crate::order_at(1), move |stage| {
            let target = Arc::clone(&target);
            async move { target.lock().expect("stage list").push(stage) }
        })
        .await;
        check_eq!(
            *visited.lock().expect("stage list"),
            ["validate", "enrich", "fraud_check", "fulfill"]
        );
        check_eq!(outcome, "fulfilled");
    }
}
