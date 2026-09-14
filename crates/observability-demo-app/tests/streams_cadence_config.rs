use std::process::Command;

fn demo() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_observability-demo-app"));
    command
        .env_remove("KRABKA_DEMO_STREAMS_BROKER_DNS_TIMEOUT")
        .env_remove("KRABKA_DEMO_STREAMS_POLL_INTERVAL")
        .env_remove("KRABKA_DEMO_STREAMS_COMMIT_INTERVAL");
    command
}

#[test]
fn environment_is_used_and_cli_wins_before_external_io() {
    let environment = demo()
        .args(["--role", "produce"])
        .env("KRABKA_DEMO_STREAMS_POLL_INTERVAL", "37ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!environment.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&environment.stderr)
            .contains("--streams-poll-interval (37ms) is only valid with --role stream")
    );

    let cli = demo()
        .args(["--role", "produce", "--streams-poll-interval", "41ms"])
        .env("KRABKA_DEMO_STREAMS_POLL_INTERVAL", "37ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!cli.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&cli.stderr)
            .contains("--streams-poll-interval (41ms) is only valid with --role stream")
    );

    let commit = demo()
        .args(["--role", "consume"])
        .env("KRABKA_DEMO_STREAMS_COMMIT_INTERVAL", "43ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!commit.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&commit.stderr)
            .contains("--streams-commit-interval (43ms) is only valid with --role stream")
    );
}

#[test]
fn zero_values_are_rejected_and_help_lists_each_flag_once() {
    for (flag, environment) in [
        (
            "--streams-poll-interval",
            "KRABKA_DEMO_STREAMS_POLL_INTERVAL",
        ),
        (
            "--streams-commit-interval",
            "KRABKA_DEMO_STREAMS_COMMIT_INTERVAL",
        ),
    ] {
        let zero = demo()
            .args(["--role", "stream"])
            .env(environment, "0")
            .output()
            .expect("run demo");
        observability_demo_app::check!(!zero.status.success());
        observability_demo_app::check!(
            String::from_utf8_lossy(&zero.stderr).contains("invalid value '0'")
        );

        let help = demo().arg("--help").output().expect("help");
        observability_demo_app::check!(help.status.success());
        let help = String::from_utf8(help.stdout).expect("UTF-8 help");
        observability_demo_app::check_eq!(
            help.split_whitespace()
                .filter(|token| *token == flag)
                .count(),
            1
        );
    }
}
