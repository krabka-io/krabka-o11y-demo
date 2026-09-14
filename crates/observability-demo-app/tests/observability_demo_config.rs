use std::path::{Path, PathBuf};

use serde_yaml::Value;

fn root() -> PathBuf {
    std::env::var("CARGO_MANIFEST_DIR").map_or_else(
        |_| PathBuf::from("."),
        |dir| {
            Path::new(&dir)
                .ancestors()
                .nth(2)
                .expect("crate under repository root")
                .to_path_buf()
        },
    )
}

fn read(path: &str) -> String {
    std::fs::read_to_string(root().join(path)).expect("read repository file")
}

fn yaml(path: &str) -> Value {
    serde_yaml::from_str(&read(path)).expect("valid YAML")
}

fn compose() -> Value {
    yaml("demo/observability/docker-compose.yml")
}

fn service<'a>(compose: &'a Value, name: &str) -> &'a Value {
    let value = &compose["services"][name];
    observability_demo_app::check!(!value.is_null(), "missing service {name}");
    value
}

fn environment<'a>(service: &'a Value, name: &str) -> Option<&'a str> {
    service["environment"][name].as_str()
}

fn command(service: &Value) -> Vec<&str> {
    service["command"]
        .as_sequence()
        .expect("command list")
        .iter()
        .map(|value| value.as_str().expect("command string"))
        .collect()
}

#[test]
fn compose_is_structural_and_every_expected_service_exists() {
    let compose = compose();
    for name in [
        "broker",
        "rustfs",
        "alloy",
        "cadvisor",
        "grafana",
        "schema-registry",
        "metrics-distributor",
        "metrics-block-builder",
        "metrics-compactor",
        "metrics-querier",
        "traces-distributor",
        "traces-block-builder",
        "traces-metrics-generator",
        "traces-querier",
        "logs-distributor",
        "logs-block-builder",
        "logs-querier",
        "profiles-distributor",
        "profiles-block-builder",
        "profiles-querier",
        "demo-produce",
        "demo-stream",
        "demo-consume",
        "gres",
        "gres-workload",
    ] {
        service(&compose, name);
    }
}

#[test]
fn published_ports_bind_to_loopback_with_an_override() {
    let compose = read("demo/observability/docker-compose.yml");
    for line in compose
        .lines()
        .filter(|line| line.trim_start().starts_with("ports:"))
    {
        observability_demo_app::check!(line.contains("${KRABKA_LISTEN_HOST:-127.0.0.1}:"));
    }
}

#[test]
fn long_running_http_services_have_healthchecks() {
    let compose = compose();
    for name in ["broker", "rustfs", "alloy", "cadvisor", "grafana"] {
        observability_demo_app::check!(!service(&compose, name)["healthcheck"].is_null(), "{name}");
    }
    let source = read("demo/observability/docker-compose.yml");
    for anchor in ["x-demo-image", "x-schema-registry-image", "x-o11y-image"] {
        let offset = source.find(anchor).expect("image anchor");
        observability_demo_app::check!(
            source[offset..]
                .lines()
                .take(12)
                .any(|line| line.contains("healthcheck:")),
            "{anchor}"
        );
    }
}

#[test]
fn completed_dependencies_are_one_shots() {
    let compose = compose();
    for dependency in [
        "broker-permissions",
        "broker-format",
        "rustfs-permissions",
        "rustfs-setup",
        "observability-topic-setup",
        "topic-setup",
        "schema-registry-ready",
        "gres-setup",
    ] {
        observability_demo_app::check_eq!(
            service(&compose, dependency)["restart"].as_str(),
            Some("no"),
            "{dependency}"
        );
    }
}

#[test]
fn demo_role_configuration_is_scoped_to_its_owner() {
    let compose = compose();
    let produce = service(&compose, "demo-produce");
    let stream = service(&compose, "demo-stream");
    let consume = service(&compose, "demo-consume");
    observability_demo_app::check_eq!(
        environment(produce, "KRABKA_DEMO_ORDERS_PER_SEC"),
        Some("${KRABKA_DEMO_ORDERS_PER_SEC:-50Hz}")
    );
    observability_demo_app::check_eq!(
        environment(stream, "KRABKA_DEMO_STREAMS_FETCH_MIN"),
        Some("${KRABKA_DEMO_STREAMS_FETCH_MIN:-1B}")
    );
    observability_demo_app::check_eq!(
        environment(consume, "KRABKA_DEMO_CONSUMER_FETCH_MIN"),
        Some("${KRABKA_DEMO_CONSUMER_FETCH_MIN:-1B}")
    );
    observability_demo_app::check_eq!(
        environment(consume, "KRABKA_DEMO_SLOW_ORDER_FRACTION"),
        Some("${KRABKA_DEMO_SLOW_ORDER_FRACTION:-0.02}")
    );
    observability_demo_app::check!(
        environment(produce, "KRABKA_DEMO_CONSUMER_FETCH_MIN").is_none()
    );
    observability_demo_app::check!(environment(stream, "KRABKA_DEMO_ORDERS_PER_SEC").is_none());
}

#[test]
fn compaction_commands_match_the_pinned_image_contract() {
    let compose = compose();
    for (name, flag) in [
        ("metrics-compactor", "--target=compactor"),
        ("metrics-block-builder", "--target=block-builder"),
        ("traces-block-builder", "--target=block-builder"),
        ("profiles-block-builder", "--target=block-builder"),
    ] {
        observability_demo_app::check!(command(service(&compose, name)).contains(&flag), "{name}");
    }
}

#[test]
fn every_owned_image_default_is_digest_pinned() {
    let text = read("demo/observability/docker-compose.yml");
    for variable in [
        "KRABKA_DEMO_IMAGE",
        "KRABKA_SCHEMA_REGISTRY_IMAGE",
        "KRABKA_BROKER_IMAGE",
        "KRABKA_O11Y_IMAGE",
        "KRABKA_GRES_IMAGE",
    ] {
        let line = text
            .lines()
            .find(|line| line.contains(&format!("${{{variable}:-")))
            .expect("image variable");
        observability_demo_app::check!(line.contains("@sha256:"), "{variable}");
    }
}

#[test]
fn alloy_scrapes_every_krabka_role_and_support_service() {
    let alloy = read("demo/observability/alloy/config.alloy");
    for target in [
        "broker:9404",
        "schema-registry:9404",
        "alloy:12345",
        "grafana:3000",
        "demo-produce:9404",
        "demo-stream:9404",
        "demo-consume:9404",
        "metrics-distributor:9404",
        "traces-distributor:9404",
        "logs-distributor:9404",
        "profiles-distributor:9404",
    ] {
        observability_demo_app::check!(alloy.contains(target), "{target}");
    }
    observability_demo_app::check!(alloy.contains("container_memory_working_set_bytes"));
}

#[test]
fn grafana_datasources_define_cross_signal_links() {
    let config = yaml("demo/observability/grafana/provisioning/datasources/krabka.yaml");
    let datasources = config["datasources"].as_sequence().expect("datasources");
    let by_uid = |uid| {
        datasources
            .iter()
            .find(|source| source["uid"].as_str() == Some(uid))
            .expect("datasource")
    };
    observability_demo_app::check_eq!(
        by_uid("krabka-tempo")["jsonData"]["tracesToLogsV2"]["datasourceUid"].as_str(),
        Some("krabka-loki")
    );
    observability_demo_app::check_eq!(
        by_uid("krabka-tempo")["jsonData"]["tracesToProfiles"]["datasourceUid"].as_str(),
        Some("krabka-pyroscope")
    );
    observability_demo_app::check_eq!(
        by_uid("krabka-tempo")["jsonData"]["serviceMap"]["datasourceUid"].as_str(),
        Some("krabka-prom")
    );
    observability_demo_app::check_eq!(
        by_uid("krabka-loki")["jsonData"]["derivedFields"][0]["datasourceUid"].as_str(),
        Some("krabka-tempo")
    );
}

#[test]
fn dashboards_are_valid_json_with_unique_uids() {
    let directory = root().join("demo/observability/grafana/provisioning/dashboards");
    let mut uids = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(directory).expect("dashboard directory") {
        let path = entry.expect("dashboard entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let dashboard: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("dashboard text"))
                .expect("valid dashboard JSON");
        let uid = dashboard["uid"].as_str().expect("dashboard uid");
        observability_demo_app::check!(uids.insert(uid.to_string()), "duplicate {uid}");
        observability_demo_app::check!(
            dashboard["panels"]
                .as_array()
                .is_some_and(|panels| !panels.is_empty())
        );
    }
    for uid in ["krabka-demo", "krabka-orders-traces", "krabka-service-red"] {
        observability_demo_app::check!(uids.contains(uid), "{uid}");
    }
}

#[tokio::test]
async fn demo_dashboard_metric_names_come_from_the_encoded_registry() {
    use krabka_units::millis;

    let metrics = observability_demo_app::metrics::DemoMetrics::new();
    metrics.record_produced("books", "us-east", "card", 42.0, millis(1));
    metrics.record_stage("validate", millis(1));
    metrics.record_processed("books", "us-east", "fulfilled", millis(4));
    metrics.record_error(observability_demo_app::metrics::PipelineErrorKind::MissingValue);
    metrics.record_stream("books", 1);
    let mut encoded = String::new();
    let registry = metrics.registry.lock().await;
    prometheus_client::encoding::text::encode(&mut encoded, &registry).expect("encode registry");

    let dashboard: serde_json::Value = serde_json::from_str(&read(
        "demo/observability/grafana/provisioning/dashboards/krabka-demo.json",
    ))
    .expect("demo dashboard");
    for expression in dashboard["panels"]
        .as_array()
        .expect("panels")
        .iter()
        .flat_map(|panel| panel["targets"].as_array().into_iter().flatten())
        .filter_map(|target| target["expr"].as_str())
    {
        for metric in expression
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .filter(|token| token.starts_with("krabka_demo_"))
        {
            let base = metric.strip_suffix("_bucket").unwrap_or(metric);
            observability_demo_app::check!(encoded.contains(base), "dashboard metric {metric}");
        }
    }
}

#[test]
fn alerting_has_routing_and_platform_failure_rules() {
    let alerting = yaml("demo/observability/grafana/provisioning/alerting/platform-alerts.yaml");
    observability_demo_app::check!(
        alerting["contactPoints"]
            .as_sequence()
            .is_some_and(|items| !items.is_empty())
    );
    observability_demo_app::check!(
        alerting["policies"]
            .as_sequence()
            .is_some_and(|items| !items.is_empty())
    );
    let text = read("demo/observability/grafana/provisioning/alerting/platform-alerts.yaml");
    for uid in [
        "krabka-service-down",
        "krabka-memory-saturation",
        "krabka-pipeline-stalled",
    ] {
        observability_demo_app::check!(text.contains(uid));
    }
}

#[test]
fn smoke_and_release_qualification_cover_all_signals_and_gres() {
    let smoke = read("demo/observability/smoke.sh");
    for target in [
        "metrics",
        "logs",
        "traces",
        "profiles",
        "cross-signal",
        "gres",
    ] {
        observability_demo_app::check!(smoke.contains(target));
    }
    let qualification = read(".github/workflows/qualify-release.yml");
    for contract in ["linux/amd64", "linux/arm64", "SHA256SUMS", "retained-state"] {
        observability_demo_app::check!(qualification.contains(contract));
    }
}

#[test]
fn runtime_revision_is_available_to_instrumented_services() {
    let compose = read("demo/observability/docker-compose.yml");
    observability_demo_app::check!(
        compose.contains("KRABKA_REVISION: \"${KRABKA_REVISION:-unknown}\"")
    );
    observability_demo_app::check!(compose.matches("*otlp-env").count() >= 8);
    observability_demo_app::check!(
        read("demo/observability/revision-report.sh").contains("docker compose images")
    );
}

#[test]
fn trace_feedback_sampling_and_signal_cost_budgets_are_enforced() {
    let compose = compose();
    observability_demo_app::check_eq!(
        compose["x-otlp-env"]["KRABKA_OTLP_SAMPLE_RATIO"].as_str(),
        Some("0.05")
    );
    observability_demo_app::check_eq!(
        environment(service(&compose, "gres"), "KRABKA_OTLP_SAMPLE_RATIO"),
        Some("1.0")
    );

    let alerts = read("demo/observability/grafana/provisioning/alerting/platform-alerts.yaml");
    for uid in [
        "krabka-trace-feedback-growth",
        "krabka-trace-index-budget",
        "krabka-demo-cardinality-budget",
    ] {
        observability_demo_app::check!(alerts.contains(uid), "{uid}");
    }
}
