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
        .env("KRABKA_DEMO_STREAMS_REBALANCE_TIMEOUT", "37s")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!environment.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&environment.stderr)
            .contains("--streams-rebalance-timeout (37s) is only valid with --role stream")
    );

    let cli = demo()
        .args(["--role", "produce", "--streams-rebalance-timeout", "41s"])
        .env("KRABKA_DEMO_STREAMS_REBALANCE_TIMEOUT", "37s")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!cli.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&cli.stderr)
            .contains("--streams-rebalance-timeout (41s) is only valid with --role stream")
    );
}

#[test]
fn invalid_values_fail_early_and_help_lists_the_flag_once() {
    let zero = demo()
        .args(["--role", "stream"])
        .env("KRABKA_DEMO_STREAMS_REBALANCE_TIMEOUT", "0ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!zero.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&zero.stderr).contains("invalid value '0ms'")
    );

    let overflow = demo()
        .args(["--role", "stream"])
        .env("KRABKA_DEMO_STREAMS_REBALANCE_TIMEOUT", "2147483648ms")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!overflow.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&overflow.stderr).contains("streams rebalance timeout")
    );

    let help = demo().arg("--help").output().expect("help");
    observability_demo_app::check!(help.status.success());
    let help = String::from_utf8(help.stdout).expect("UTF-8 help");
    observability_demo_app::check_eq!(
        help.split_whitespace()
            .filter(|token| *token == "--streams-rebalance-timeout")
            .count(),
        1
    );
}
