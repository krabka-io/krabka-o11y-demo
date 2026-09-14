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
        .env("KRABKA_DEMO_CONSUMER_ASSIGNOR", "cooperative-sticky")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!environment.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&environment.stderr)
            .contains("--consumer-assignor is only valid with --role consume")
    );

    let cli = demo()
        .args(["--role", "stream", "--consumer-assignor", "range"])
        .env("KRABKA_DEMO_CONSUMER_ASSIGNOR", "invalid")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!cli.status.success());
    let stderr = String::from_utf8_lossy(&cli.stderr);
    observability_demo_app::check!(
        stderr.contains("--consumer-assignor is only valid with --role consume")
    );
    observability_demo_app::check!(!stderr.contains("invalid assignor"));
}

#[test]
fn invalid_values_fail_and_help_lists_each_flag_once() {
    let invalid = demo()
        .args([
            "--role",
            "consume",
            "--consumer-isolation-level",
            "read_committed",
        ])
        .output()
        .expect("run demo");
    observability_demo_app::check!(!invalid.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&invalid.stderr).contains("invalid isolation level")
    );

    let help = demo().arg("--help").output().expect("run help");
    observability_demo_app::check!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    for flag in [
        "--consumer-auto-offset-reset",
        "--consumer-isolation-level",
        "--consumer-assignor",
    ] {
        observability_demo_app::check_eq!(stdout.matches(flag).count(), 1, "{flag}");
    }
}
