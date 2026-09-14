//! Runs `demo/observability/check-services.sh` against a stub `docker` command.
//!
//! The stub answers the four Docker calls that the script makes from fixture
//! files. Each case compares the whole exit status and the whole report.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use assert2::assert;

const O11Y_IMAGE: &str = "ghcr.io/krabka-io/krabka-o11y@sha256:3032e3ba2c11a6c014e499edabd7fb6f8939f9d7d40ae659bd4dfbdad1051efa";
const O11Y_DIGEST: &str = "sha256:3032e3ba2c11a6c014e499edabd7fb6f8939f9d7d40ae659bd4dfbdad1051efa";

const STUB_DOCKER: &str = r#"#!/bin/sh
case "$1 $2" in
  "compose config") cat "$STUB_FIXTURES/config.json" ;;
  "compose ps") cat "$STUB_FIXTURES/ids.txt" ;;
  "inspect "*) cat "$STUB_FIXTURES/inspect.json" ;;
  "image inspect") ;;
  "events "*) cat "$STUB_FIXTURES/events.txt" ;;
  *) echo "unexpected docker call: $*" >&2; exit 64 ;;
esac
"#;

fn repo_root() -> PathBuf {
    // The same resolution as the config suite: Cargo sets the variable, and
    // Bazel runs the test from the workspace root.
    match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => Path::new(&dir)
            .ancestors()
            .nth(2)
            .expect("crate lives under repo_root/crates/<name>")
            .to_path_buf(),
        Err(_) => PathBuf::from("."),
    }
}

/// One Compose service as `docker compose config --format json` renders it.
struct Service {
    name: &'static str,
    one_shot: bool,
    command: &'static [&'static str],
}

/// One container as `docker inspect` renders the fields the script reads.
struct Container {
    service: &'static str,
    state: &'static str,
    exit_code: u8,
    health: Option<&'static str>,
    restarts: u32,
    args: &'static [&'static str],
}

struct Case {
    name: &'static str,
    expected_services: Option<&'static str>,
    containers: Vec<Container>,
    last_die_exit_code: &'static str,
    status: i32,
    report: String,
}

const METRICS_COMPACTOR_OLD: &[&str] = &[
    "--target=compactor",
    "--object-store-url=s3://krabka-metrics",
    "--bootstrap=broker:9092",
    "--compactor-retention=1h",
    "--compactor-retention-sweep-interval=30s",
];
const METRICS_COMPACTOR: &[&str] = &[
    "--target=compactor",
    "--object-store-url=s3://krabka-metrics",
    "--compactor-interval=1m",
];
const LOGS_BLOCK_BUILDER: &[&str] = &[
    "--target=block-builder",
    "--wal-bootstrap-server=broker:9092",
    "--object-store-url=s3://krabka-logs",
    "--index-prefix=logs",
];
const TOPIC_SETUP: &[&str] = &["--bootstrap=broker:9092"];
const BROKER: &[&str] = &["--listen-addr=0.0.0.0:9092"];

fn services() -> Vec<Service> {
    vec![
        Service {
            name: "observability-topic-setup",
            one_shot: true,
            command: TOPIC_SETUP,
        },
        Service {
            name: "broker",
            one_shot: false,
            command: BROKER,
        },
        Service {
            name: "metrics-compactor",
            one_shot: false,
            command: METRICS_COMPACTOR,
        },
        Service {
            name: "logs-block-builder",
            one_shot: false,
            command: LOGS_BLOCK_BUILDER,
        },
    ]
}

fn executable(service: &str) -> &'static str {
    match service {
        "observability-topic-setup" => "krabka-o11y-bootstrap",
        "broker" => "krabka-broker",
        "metrics-compactor" => "krabka-metrics",
        _ => "krabka-observability",
    }
}

fn json_strings(values: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let quoted: Vec<String> = values
        .into_iter()
        .map(|value| format!("\"{}\"", value.as_ref()))
        .collect();
    format!("[{}]", quoted.join(","))
}

fn compose_config_json(services: &[Service]) -> String {
    let entries: Vec<String> = services
        .iter()
        .map(|service| {
            let restart = if service.one_shot {
                "no"
            } else {
                "unless-stopped"
            };
            let command =
                std::iter::once(executable(service.name)).chain(service.command.iter().copied());
            format!(
                "\"{}\":{{\"image\":\"{O11Y_IMAGE}\",\"restart\":\"{restart}\",\"command\":{}}}",
                service.name,
                json_strings(command)
            )
        })
        .collect();
    format!("{{\"services\":{{{}}}}}", entries.join(","))
}

fn container_id(index: usize) -> String {
    format!("container{index}")
}

fn inspect_json(containers: &[Container]) -> String {
    let entries: Vec<String> = containers
        .iter()
        .enumerate()
        .map(|(index, container)| {
            let health = container.health.map_or(String::new(), |status| {
                format!(",\"Health\":{{\"Status\":\"{status}\"}}")
            });
            format!(
                concat!(
                    "{{\"Id\":\"{id}\",",
                    "\"Config\":{{\"Labels\":{{\"com.docker.compose.service\":\"{service}\"}},\"Image\":\"{image}\"}},",
                    "\"State\":{{\"Status\":\"{state}\",\"ExitCode\":{exit_code}{health}}},",
                    "\"RestartCount\":{restarts},",
                    "\"Created\":\"2026-09-14T16:56:00.000000000Z\",",
                    "\"Image\":\"sha256:0000\",",
                    "\"Path\":\"{path}\",",
                    "\"Args\":{args}}}"
                ),
                id = container_id(index),
                service = container.service,
                image = O11Y_IMAGE,
                state = container.state,
                exit_code = container.exit_code,
                health = health,
                restarts = container.restarts,
                path = executable(container.service),
                args = json_strings(container.args),
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

fn running(service: &'static str, args: &'static [&'static str]) -> Container {
    Container {
        service,
        state: "running",
        exit_code: 0,
        health: None,
        restarts: 0,
        args,
    }
}

fn topic_setup_exited(exit_code: u8) -> Container {
    Container {
        service: "observability-topic-setup",
        state: "exited",
        exit_code,
        health: None,
        restarts: 0,
        args: TOPIC_SETUP,
    }
}

fn line(verdict: &str, service: &str, reason: &str, fields: &str, command: &str) -> String {
    format!(
        "{verdict} service={service} reason=\"{reason}\" {fields} digest={O11Y_DIGEST} command=\"{command}\"\n"
    )
}

fn cases() -> Vec<Case> {
    let healthy_broker = || Container {
        health: Some("healthy"),
        ..running("broker", BROKER)
    };
    vec![
        Case {
            name: "each service in its expected state",
            expected_services: None,
            containers: vec![
                topic_setup_exited(0),
                healthy_broker(),
                running("metrics-compactor", METRICS_COMPACTOR),
                running("logs-block-builder", LOGS_BLOCK_BUILDER),
            ],
            last_die_exit_code: "",
            status: 0,
            report: String::new(),
        },
        Case {
            name: "a compactor restarts in a loop on a rejected flag",
            expected_services: None,
            containers: vec![
                topic_setup_exited(0),
                healthy_broker(),
                Container {
                    state: "restarting",
                    exit_code: 2,
                    restarts: 7,
                    ..running("metrics-compactor", METRICS_COMPACTOR_OLD)
                },
                running("logs-block-builder", LOGS_BLOCK_BUILDER),
            ],
            last_die_exit_code: "2",
            status: 1,
            report: line(
                "FAILED",
                "metrics-compactor",
                "long-running service restarted",
                "state=restarting exit_code=2 health=none restarts=7",
                "krabka-metrics --target=compactor --object-store-url=s3://krabka-metrics --bootstrap=broker:9092 --compactor-retention=1h --compactor-retention-sweep-interval=30s",
            ),
        },
        Case {
            name: "a running service restarted once, and the die event keeps its exit code",
            expected_services: None,
            containers: vec![
                topic_setup_exited(0),
                healthy_broker(),
                running("metrics-compactor", METRICS_COMPACTOR),
                Container {
                    restarts: 1,
                    ..running("logs-block-builder", LOGS_BLOCK_BUILDER)
                },
            ],
            last_die_exit_code: "1",
            status: 1,
            report: line(
                "FAILED",
                "logs-block-builder",
                "long-running service restarted",
                "state=running exit_code=1 health=none restarts=1",
                "krabka-observability --target=block-builder --wal-bootstrap-server=broker:9092 --object-store-url=s3://krabka-logs --index-prefix=logs",
            ),
        },
        Case {
            name: "a long-running service exited",
            expected_services: None,
            containers: vec![
                topic_setup_exited(0),
                healthy_broker(),
                Container {
                    state: "exited",
                    exit_code: 137,
                    ..running("metrics-compactor", METRICS_COMPACTOR)
                },
                running("logs-block-builder", LOGS_BLOCK_BUILDER),
            ],
            last_die_exit_code: "",
            status: 1,
            report: line(
                "FAILED",
                "metrics-compactor",
                "long-running service is not running",
                "state=exited exit_code=137 health=none restarts=0",
                "krabka-metrics --target=compactor --object-store-url=s3://krabka-metrics --compactor-interval=1m",
            ),
        },
        Case {
            name: "a one-shot service failed",
            expected_services: None,
            containers: vec![
                topic_setup_exited(1),
                healthy_broker(),
                running("metrics-compactor", METRICS_COMPACTOR),
                running("logs-block-builder", LOGS_BLOCK_BUILDER),
            ],
            last_die_exit_code: "",
            status: 1,
            report: line(
                "FAILED",
                "observability-topic-setup",
                "one-shot service exited with a non-zero code",
                "state=exited exit_code=1 health=none restarts=0",
                "krabka-o11y-bootstrap --bootstrap=broker:9092",
            ),
        },
        Case {
            name: "a healthcheck has not passed yet",
            expected_services: None,
            containers: vec![
                topic_setup_exited(0),
                Container {
                    health: Some("starting"),
                    ..running("broker", BROKER)
                },
                running("metrics-compactor", METRICS_COMPACTOR),
                running("logs-block-builder", LOGS_BLOCK_BUILDER),
            ],
            last_die_exit_code: "",
            status: 2,
            report: line(
                "PENDING",
                "broker",
                "long-running service is not healthy",
                "state=running exit_code=0 health=starting restarts=0",
                "krabka-broker --listen-addr=0.0.0.0:9092",
            ),
        },
        Case {
            name: "an expected service has no container",
            expected_services: None,
            containers: vec![
                topic_setup_exited(0),
                healthy_broker(),
                running("metrics-compactor", METRICS_COMPACTOR),
            ],
            last_die_exit_code: "",
            status: 1,
            report: line(
                "FAILED",
                "logs-block-builder",
                "expected service has no container",
                "state=missing exit_code=none health=none restarts=0",
                "krabka-observability --target=block-builder --wal-bootstrap-server=broker:9092 --object-store-url=s3://krabka-logs --index-prefix=logs",
            ),
        },
        Case {
            name: "KRABKA_SMOKE_SERVICES limits the services that must have a container",
            expected_services: Some("broker metrics-compactor"),
            containers: vec![
                topic_setup_exited(0),
                healthy_broker(),
                running("metrics-compactor", METRICS_COMPACTOR),
            ],
            last_die_exit_code: "",
            status: 0,
            report: String::new(),
        },
    ]
}

fn fixture_dir(case_index: usize) -> PathBuf {
    let base = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = base.join(format!(
        "check-services-gate-{}-{case_index}",
        std::process::id()
    ));
    fs::create_dir_all(dir.join("bin")).expect("create fixture dir");
    dir
}

fn run_case(case_index: usize, case: &Case) -> (i32, String) {
    let dir = fixture_dir(case_index);
    let docker = dir.join("bin/docker");
    fs::write(&docker, STUB_DOCKER).expect("write stub docker");
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).expect("chmod stub docker");
    fs::write(dir.join("config.json"), compose_config_json(&services())).expect("write config");
    let ids: Vec<String> = (0..case.containers.len()).map(container_id).collect();
    fs::write(dir.join("ids.txt"), ids.join("\n")).expect("write ids");
    fs::write(dir.join("inspect.json"), inspect_json(&case.containers)).expect("write inspect");
    fs::write(dir.join("events.txt"), case.last_die_exit_code).expect("write events");

    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned())
    );
    let mut command = Command::new("sh");
    command
        .arg(repo_root().join("demo/observability/check-services.sh"))
        .env("PATH", path)
        .env("STUB_FIXTURES", &dir)
        .env_remove("KRABKA_SMOKE_SERVICES");
    if let Some(expected) = case.expected_services {
        command.env("KRABKA_SMOKE_SERVICES", expected);
    }
    let output = command.output().expect("run check-services.sh");
    assert!(
        output.stderr.is_empty(),
        "{}: {}",
        case.name,
        String::from_utf8_lossy(&output.stderr)
    );
    (
        output.status.code().expect("exit status"),
        String::from_utf8(output.stdout).expect("UTF-8 report"),
    )
}

#[test]
fn check_services_reports_each_service_that_is_not_in_its_expected_state() {
    for (index, case) in cases().iter().enumerate() {
        let actual = run_case(index, case);
        assert!(
            actual == (case.status, case.report.clone()),
            "{}",
            case.name
        );
    }
}
