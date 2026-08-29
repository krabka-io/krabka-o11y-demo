//! Demo-app business metrics for Prometheus, with the prefix `krabka_demo`.
//!
//! This module mirrors the metric shape of the observability services. It holds
//! a `prometheus-client` [`Registry`] in an `Arc<Mutex<…>>`, a
//! cheaply-cloneable [`DemoMetrics`] bundle that hands out counter and
//! histogram handles, and a `/metrics` router. `serve_admin_from_env_with`
//! merges that router onto the admin port `:9404`.
//!
//! The producer role increments the `orders_produced`, value, and latency
//! handles. The traced consumer role increments the `orders_processed` and
//! per-stage handles. These metrics give the demo dashboards a business-level
//! view next to the RED metrics that the backend services already expose. That
//! view holds orders by category, region, and payment method, the order value
//! distribution, and the per-stage processing latency.
//!
//! `prometheus-client` appends `_total` to counters at encode time, so the
//! code registers counter names WITHOUT the suffix.

use std::sync::Arc;

use krabka_units::{Time, convert::TimeExt as _};
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, family::Family, histogram::Histogram},
    registry::Registry,
};
use tokio::sync::Mutex;

pub type SharedRegistry = Arc<Mutex<Registry>>;

/// Produced-order label: business dimensions carried on every produced order.
#[derive(Debug, Clone, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ProducedLabel {
    pub category: String,
    pub region: String,
    pub payment_method: String,
}

/// Processed-order label: the cross-product of category, region, and terminal
/// outcome (`fulfilled|fraud_rejected|anomalous`).
#[derive(Debug, Clone, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ProcessedLabel {
    pub category: String,
    pub region: String,
    pub outcome: String,
}

/// Per-stage latency label (`stage="validate|enrich|fraud_check|fulfill"`).
#[derive(Debug, Clone, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct StageLabel {
    pub stage: String,
}

/// Cheaply-clonable bundle of demo metric handles plus the shared registry.
#[derive(Clone)]
pub struct DemoMetrics {
    pub registry: SharedRegistry,
    // PRODUCE side.
    pub orders_produced: Family<ProducedLabel, Counter>,
    pub order_value_dollars: Histogram,
    pub produce_latency: Histogram,
    // PROCESS (traced consumer) side.
    pub orders_processed: Family<ProcessedLabel, Counter>,
    pub process_stage_latency: Family<StageLabel, Histogram>,
    pub order_processing_latency: Histogram,
}

impl DemoMetrics {
    /// Build a fresh registry, register every metric, and return the bundle.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Registry::with_prefix("krabka_demo");

        let orders_produced = Family::<ProducedLabel, Counter>::default();
        let order_value_dollars =
            Histogram::new([1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 150.0, 200.0, 500.0]);
        let produce_latency = Histogram::new([
            0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5,
        ]);
        let orders_processed = Family::<ProcessedLabel, Counter>::default();
        let process_stage_latency = Family::<StageLabel, Histogram>::new_with_constructor(|| {
            Histogram::new([0.0001, 0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05])
        });
        let order_processing_latency =
            Histogram::new([0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25]);

        registry.register(
            "orders_produced",
            "Orders produced by category, region and payment method",
            orders_produced.clone(),
        );
        registry.register(
            "order_value_dollars",
            "Distribution of produced order value in dollars",
            order_value_dollars.clone(),
        );
        registry.register(
            "produce_latency_seconds",
            "Producer send() latency in seconds",
            produce_latency.clone(),
        );
        registry.register(
            "orders_processed",
            "Orders processed by the traced consumer, by category, region and outcome",
            orders_processed.clone(),
        );
        registry.register(
            "process_stage_latency_seconds",
            "Per-stage processing latency in seconds (validate|enrich|fraud_check|fulfill)",
            process_stage_latency.clone(),
        );
        registry.register(
            "order_processing_latency_seconds",
            "End-to-end consumer processing latency per order in seconds",
            order_processing_latency.clone(),
        );

        Self {
            registry: Arc::new(Mutex::new(registry)),
            orders_produced,
            order_value_dollars,
            produce_latency,
            orders_processed,
            process_stage_latency,
            order_processing_latency,
        }
    }

    /// Record one produced order. The method increments the counter for the
    /// (category, region, payment) triple, and it observes the value and the
    /// send latency.
    pub fn record_produced(
        &self,
        category: &str,
        region: &str,
        payment_method: &str,
        value_dollars: f64,
        latency: Time,
    ) {
        self.orders_produced
            .get_or_create(&ProducedLabel {
                category: category.into(),
                region: region.into(),
                payment_method: payment_method.into(),
            })
            .inc();
        self.order_value_dollars.observe(value_dollars);
        self.produce_latency.observe(latency.secs_f64());
    }

    /// Observe one processing-stage latency.
    pub fn record_stage(&self, stage: &str, latency: Time) {
        self.process_stage_latency
            .get_or_create(&StageLabel {
                stage: stage.into(),
            })
            .observe(latency.secs_f64());
    }

    /// Record one processed order's terminal outcome and total latency.
    pub fn record_processed(&self, category: &str, region: &str, outcome: &str, total: Time) {
        self.orders_processed
            .get_or_create(&ProcessedLabel {
                category: category.into(),
                region: region.into(),
                outcome: outcome.into(),
            })
            .inc();
        self.order_processing_latency.observe(total.secs_f64());
    }
}

impl Default for DemoMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// `/metrics` router that serves the `OpenMetrics` text encoding of `registry`.
///
/// `serve_admin_from_env_with` merges it onto the admin port.
pub fn metrics_router(registry: SharedRegistry) -> axum::Router {
    axum::Router::new()
        .route("/metrics", axum::routing::get(export))
        .with_state(registry)
}

async fn export(
    axum::extract::State(reg): axum::extract::State<SharedRegistry>,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;

    let mut buf = String::new();
    let r = reg.lock().await;
    if let Err(e) = prometheus_client::encoding::text::encode(&mut buf, &r) {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("encode: {e}"),
        )
            .into_response();
    }
    (
        axum::http::StatusCode::OK,
        [(
            "content-type",
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        buf,
    )
        .into_response()
}

#[cfg(test)]
mod tests {

    use krabka_units::{micros, millis};

    use super::*;

    /// The `/metrics` route is the only thing that makes any of the recording
    /// above observable, and it is two pieces: a router that maps the path, and
    /// a handler that encodes the registry. Driving a real request through both
    /// is what distinguishes them from an empty router and an empty response --
    /// each of which still builds, serves, and returns a 200 from somewhere.
    #[tokio::test]
    async fn metrics_route_serves_the_encoded_registry() {
        use tower::ServiceExt as _;

        let m = DemoMetrics::new();
        m.record_produced("books", "us-east", "card", 42.0, millis(2));

        let response = metrics_router(Arc::clone(&m.registry))
            .oneshot(
                axum::http::Request::builder()
                    .uri("/metrics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert2::assert!(response.status() == axum::http::StatusCode::OK);
        assert2::assert!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                == Some("application/openmetrics-text; version=1.0.0; charset=utf-8")
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert2::assert!(body.contains("krabka_demo_orders_produced_total"));
        assert2::assert!(body.contains("category=\"books\""));
    }

    /// A path the router does not map is a 404, which is what an empty router
    /// would return for `/metrics` too.
    #[tokio::test]
    async fn metrics_router_maps_only_the_metrics_path() {
        use tower::ServiceExt as _;

        let response = metrics_router(DemoMetrics::new().registry)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/not-metrics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert2::assert!(response.status() == axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn registry_has_demo_prefix_and_all_metrics() {
        let m = DemoMetrics::new();
        m.record_produced("books", "us-east", "card", 42.0, millis(2));
        m.record_stage("validate", micros(300));
        m.record_stage("fraud_check", micros(1_100));
        m.record_processed("books", "us-east", "fulfilled", millis(4));

        let mut buf = String::new();
        let r = m.registry.lock().await;
        prometheus_client::encoding::text::encode(&mut buf, &r).unwrap();

        for needle in [
            "krabka_demo_orders_produced_total",
            "krabka_demo_order_value_dollars",
            "krabka_demo_produce_latency_seconds",
            "krabka_demo_orders_processed_total",
            "krabka_demo_process_stage_latency_seconds",
            "krabka_demo_order_processing_latency_seconds",
        ] {
            assert2::assert!(buf.contains(needle));
        }
        assert2::assert!(buf.contains("category=\"books\""));
        assert2::assert!(buf.contains("region=\"us-east\""));
        assert2::assert!(buf.contains("payment_method=\"card\""));
        assert2::assert!(buf.contains("outcome=\"fulfilled\""));
        assert2::assert!(buf.contains("stage=\"validate\""));
    }

    #[test]
    fn produced_counter_accumulates_per_label() {
        let m = DemoMetrics::new();
        m.record_produced("toys", "eu-west", "wire", 10.0, millis(1));
        m.record_produced("toys", "eu-west", "wire", 20.0, millis(1));
        m.record_produced("toys", "ap-south", "wire", 30.0, millis(1));
        assert2::assert!(
            m.orders_produced
                .get_or_create(&ProducedLabel {
                    category: "toys".into(),
                    region: "eu-west".into(),
                    payment_method: "wire".into(),
                })
                .get()
                == 2
        );
        assert2::assert!(
            m.orders_produced
                .get_or_create(&ProducedLabel {
                    category: "toys".into(),
                    region: "ap-south".into(),
                    payment_method: "wire".into(),
                })
                .get()
                == 1
        );
    }
}
