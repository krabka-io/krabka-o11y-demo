use std::sync::{Arc, Mutex};

use krabka_client_consumer::Header as ConsumerHeader;
use observability_demo_app::pipeline::{continue_trace, order_headers, process_order};
use observability_demo_app::{check, check_eq, order_at};
use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use tracing_subscriber::prelude::*;

fn with_otel_subscriber(f: impl FnOnce()) {
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );
    let provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .build();
    let tracer = provider.tracer("demo-trace-contract");
    let subscriber =
        tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(tracer));
    tracing::subscriber::with_default(subscriber, f);
}

fn consumer_headers(headers: Vec<krabka_client_producer::Header>) -> Vec<ConsumerHeader> {
    headers
        .into_iter()
        .map(|header| ConsumerHeader {
            key: header.key,
            value: header.value,
        })
        .collect()
}

#[test]
fn producer_headers_continue_the_same_trace_and_preserve_business_context() {
    with_otel_subscriber(|| {
        let order = order_at(42);
        let producer = tracing::info_span!("produce_order");
        let producer_trace = {
            let _entered = producer.enter();
            let headers = order_headers(
                krabka_telemetry::propagation::current_trace_headers(),
                &order,
            );
            let traceparent = headers
                .iter()
                .find(|header| header.key == "traceparent")
                .and_then(|header| header.value.as_deref())
                .and_then(|value| std::str::from_utf8(value).ok())
                .expect("valid traceparent");
            check!(traceparent.starts_with("00-"));
            check_eq!(traceparent.len(), 55);
            check!(headers.iter().any(|header| {
                header.key == "x-demo-region"
                    && header.value.as_deref() == Some(order.region.as_bytes())
            }));
            check!(headers.iter().any(|header| {
                header.key == "x-demo-tier"
                    && header.value.as_deref() == Some(order.customer_tier.as_bytes())
            }));

            let consumer = tracing::info_span!("process_order");
            continue_trace(&consumer, &consumer_headers(headers));
            let producer_context = producer.context();
            let consumer_context = consumer.context();
            let producer_span = producer_context.span();
            let consumer_span = consumer_context.span();
            check_eq!(
                consumer_span.span_context().trace_id(),
                producer_span.span_context().trace_id()
            );
            producer_span.span_context().trace_id()
        };
        check!(producer_trace != opentelemetry::trace::TraceId::INVALID);
    });
}

#[test]
fn missing_traceparent_starts_a_fresh_trace() {
    with_otel_subscriber(|| {
        let consumer = tracing::info_span!("process_order");
        continue_trace(&consumer, &[]);
        check!(consumer.context().span().span_context().is_valid());
    });
}

#[tokio::test]
async fn stages_and_all_terminal_outcomes_are_observable() {
    let mut anomalous = order_at(17);
    anomalous.amount = 0.0;
    let mut fraud = order_at(1);
    fraud.payment_method = "crypto".into();
    fraud.amount = 200.0;
    let fulfilled = order_at(1);

    for (order, expected) in [
        (anomalous, "anomalous"),
        (fraud, "fraud_rejected"),
        (fulfilled, "fulfilled"),
    ] {
        let stages = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&stages);
        let outcome = process_order(&order, move |stage| {
            let recorded = Arc::clone(&recorded);
            async move { recorded.lock().expect("stage recorder").push(stage) }
        })
        .await;
        check_eq!(
            *stages.lock().expect("stage recorder"),
            ["validate", "enrich", "fraud_check", "fulfill"]
        );
        check_eq!(outcome, expected);
    }
}
