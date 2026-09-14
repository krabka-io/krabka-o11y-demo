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
        .env("KRABKA_DEMO_STREAMS_STATE_STORE_CACHE_MAX", "37B")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!environment.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&environment.stderr)
            .contains("--streams-state-store-cache-max (37B) is only valid with --role stream")
    );

    let cli = demo()
        .args([
            "--role",
            "produce",
            "--streams-state-store-cache-max",
            "41B",
        ])
        .env("KRABKA_DEMO_STREAMS_STATE_STORE_CACHE_MAX", "37B")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!cli.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&cli.stderr)
            .contains("--streams-state-store-cache-max (41B) is only valid with --role stream")
    );
}

#[test]
fn negative_fails_early_zero_is_parseable_and_help_lists_the_flag_once() {
    let negative = demo()
        .args(["--role", "stream"])
        .env("KRABKA_DEMO_STREAMS_STATE_STORE_CACHE_MAX", "-1B")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!negative.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&negative.stderr).contains("must be non-negative")
    );

    let zero = demo()
        .args(["--role", "produce"])
        .env("KRABKA_DEMO_STREAMS_STATE_STORE_CACHE_MAX", "0B")
        .output()
        .expect("run demo");
    observability_demo_app::check!(!zero.status.success());
    observability_demo_app::check!(
        String::from_utf8_lossy(&zero.stderr)
            .contains("--streams-state-store-cache-max (0B) is only valid with --role stream")
    );

    let help = demo().arg("--help").output().expect("help");
    observability_demo_app::check!(help.status.success());
    let help = String::from_utf8(help.stdout).expect("UTF-8 help");
    observability_demo_app::check_eq!(
        help.split_whitespace()
            .filter(|token| *token == "--streams-state-store-cache-max")
            .count(),
        1
    );
}
