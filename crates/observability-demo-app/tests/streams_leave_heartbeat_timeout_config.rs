use std::process::Command;

fn demo() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_observability-demo-app"));
    command.env_clear();
    command
}

#[test]
fn environment_is_used_and_cli_wins_before_external_io() {
    let environment = demo()
        .args(["--role", "produce"])
        .env("KRABKA_DEMO_STREAMS_LEAVE_HEARTBEAT_TIMEOUT", "37ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!environment.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&environment.stderr)
            .contains("--streams-leave-heartbeat-timeout (37ms) is only valid with --role stream")
    );

    let cli = demo()
        .args([
            "--role",
            "produce",
            "--streams-leave-heartbeat-timeout",
            "41ms",
        ])
        .env("KRABKA_DEMO_STREAMS_LEAVE_HEARTBEAT_TIMEOUT", "37ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!cli.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&cli.stderr)
            .contains("--streams-leave-heartbeat-timeout (41ms) is only valid with --role stream")
    );
}

#[test]
fn zero_fails_early_and_help_lists_the_flag_once() {
    let zero = demo()
        .args(["--role", "stream"])
        .env("KRABKA_DEMO_STREAMS_LEAVE_HEARTBEAT_TIMEOUT", "0ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!zero.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&zero.stderr).contains("invalid value '0ms'")
    );

    let help = demo().arg("--help").output().expect("help");
    observability_demo_app::check!(help.status.success());
    let help = String::from_utf8(help.stdout).expect("UTF-8 help");
    observability_demo_app::check_eq!(
        help.split_whitespace()
            .filter(|token| *token == "--streams-leave-heartbeat-timeout")
            .count(),
        1
    );
}
