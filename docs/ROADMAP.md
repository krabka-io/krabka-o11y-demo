# Roadmap

krabka-o11y-demo is the top of the krabka stack. It holds one Rust crate, `crates/observability-demo-app`, and a 29-service compose stack under `demo/observability` that runs krabka's metrics, traces, logs and profiles backends behind Grafana. The parts work and the seams do not: the `stream` role emits no business metrics and no spans, nothing counts a failure, three services have a healthcheck, the four Grafana datasources carry no cross-signal links, and CI runs Bazel only although both READMEs say Cargo is gated too. Two `#[allow(clippy::...)]` and 47 plain `assert_eq!` calls sit in `src/main.rs` against rules that `CLAUDE.md` states without exception, and no test anywhere touches the distributed trace the README calls the headline. This roadmap makes each claim true one layer at a time: first the gates that catch drift, then the telemetry the demo promises, then the stack that displays it, and last the documents that teach it. At the end a reader clones the repository, starts the stack, and follows one order from a metric to its trace to its logs to its flamegraph.

The work is tracked as GitHub issues in this repository. Each epic below is an issue that
holds its child issues as sub-issues, so the progress bar on the epic is the real count.
Each milestone answers one question, and it draws its issues from several epics. The
milestones are sequential: a milestone needs the one before it.

## Milestones

| Milestone | The question it answers | Due | Issues |
| :--- | :--- | :--- | ---: |
| [M1 · Close the gates](https://github.com/krabka-io/krabka-o11y-demo/milestone/2) | Does CI check what this repository claims it checks? | 2026-09-26 | 9 |
| [M2 · Make the pipeline tell the truth](https://github.com/krabka-io/krabka-o11y-demo/milestone/3) | Does the demo app emit the full-signal telemetry it promises? | 2026-11-07 | 12 |
| [M3 · Make the stack observable and safe to run](https://github.com/krabka-io/krabka-o11y-demo/milestone/4) | Is every container watched, correlated, pinned and safe to start on a laptop? | 2027-01-02 | 13 |
| [M4 · Make the demo teachable](https://github.com/krabka-io/krabka-o11y-demo/milestone/5) | Can a first-time reader understand what they just started? | 2027-02-27 | 5 |

### [M1 · Close the gates](https://github.com/krabka-io/krabka-o11y-demo/milestone/2)

Today it does not. `.github/workflows/ci.yml` invokes no Cargo command; the only one in the tree is `cargo generate-lockfile` in `sync-siblings.yml`. `README.md` and `CLAUDE.md` both say Cargo is gated. `clippy::pedantic` is enforced nowhere. `cargo deny` is never run. `sync-siblings.yml` exits 1 for one sibling on every scheduled run and never sees a fifth. Renovate automerges compose and lockfile changes into checks that do read those files but cannot see what the bump changed.

This milestone builds the gates before anything else changes, so later work lands on a floor that catches drift. It also lands the two test rewrites that every later change depends on: the assert2 conversion, and the parsing rewrite of the 899-line compose and dashboard suite whose whitespace-exact needles would otherwise break on every stack edit. The accuracy sweep and the contributor scaffolding land here too, so a reader arriving at a repaired repository is not misled by the documents.

**Done when:** every pull request runs `cargo build --locked`, `cargo nextest run`, `cargo clippy -- -D warnings` and `cargo deny check`; all five siblings get a bump proposal; no plain assertion macro remains under `crates/`; the config suite survives a reformat of any file it reads; and no Markdown link in the tree is broken.

### [M2 · Make the pipeline tell the truth](https://github.com/krabka-io/krabka-o11y-demo/milestone/3)

The `stream` role emits no business metrics and no spans, because `run_stream` is the one role runner `main()` never hands `&DemoMetrics`. Nothing counts a failure: `src/metrics.rs` registers six families and none is an error counter. Every latency histogram has one populated bucket, because `stage()` sleeps a constant. Two roles cannot reach `telemetry.shutdown()` at all. The distributed trace the README calls the headline is asserted by no test.

This milestone fixes the source. It opens with the split of `src/main.rs`, which moves the order pipeline into the library where a test and the mutation sweep can reach it, and deletes both `#[allow(clippy::too_many_arguments)]`. Everything else in the milestone builds on that split, so the six pipeline issues run in sequence rather than in parallel: they all edit one file.

**Done when:** `demo-stream` exports a non-empty `krabka_demo_*` series and appears in Tempo; a deserialize failure, a value-less record and a send failure each raise a counter and mark their span; the stage histograms populate more than one bucket; all three roles stop on SIGTERM; and a test asserts one trace id spanning the producer and consumer spans in process.

### [M3 · Make the stack observable and safe to run](https://github.com/krabka-io/krabka-o11y-demo/milestone/4)

Three of the 29 services have a healthcheck, so `docker compose up --wait` cannot be used. The four Grafana datasources carry no cross-signal links, so the pivot the demo exists to show needs a trace id copied by hand. Alloy, schema-registry and Grafana are unscraped. The alert set never touches the memory caps the whole `.env` is written around. One string holds off a self-observation loop that once reached 62M spans, and nothing watches the rate. About twelve ports bind every interface in front of an anonymous-Admin Grafana. Nothing states which krabka revision the stack runs.

This milestone makes the stack deterministic to start, fully watched, correlated, pinned and honest about what it exposes. Healthchecks land first, because the smoke check and the nightly CI job need a readiness signal to wait for. The image digest pin lands before the compose gate becomes required.

**Done when:** `docker compose up --wait` succeeds twice from a cold start; every container is scraped and alerted on; a viewer pivots between all four signals by clicking; `smoke.sh` reports all four signals plus gres and runs nightly; every port binds the loopback address; and the stack reports the revision it runs.

### [M4 · Make the demo teachable](https://github.com/krabka-io/krabka-o11y-demo/milestone/5)

The repository's product is comprehension, and today there is no path through it. `crates/observability-demo-app` has no README and no design doc. There is no image anywhere in the tree, so the 25 bullet lines of "What you should see" are the only description of the Grafana UI. The order trace the README calls the headline has no dashboard, and the RED metrics that `traces-metrics-generator` writes continuously are read by nothing.

This milestone is last because it depends on everything before it. The dashboards need the datasource correlation from M3. The walkthrough's screenshots need those dashboards. The crate README and design doc need the stream-role question settled in M2, so the documents describe one behaviour rather than two.

**Done when:** a reader goes from `docker compose up -d` to one distributed trace opened in Tempo by following `docs/walkthrough.md`, with an image for each of the four signals; the orders-trace and service-RED dashboards are provisioned and cross-linked; the crate carries its README, its design doc and a doc comment on every public item; and the demo README has prerequisites, tuning and troubleshooting sections that name real knobs.

## Epics

### [#10 Deepen the demo pipeline's signals and split main.rs](https://github.com/krabka-io/krabka-o11y-demo/issues/10)

`area:pipeline` · lands mostly in M2 · Make the pipeline tell the truth

`crates/observability-demo-app` is the only code in this repository that generates telemetry. The 29-service stack under `demo/observability` exists to display what this one crate emits. This epic makes the crate emit the four-signal pipeline the README describes, and brings it back inside the rules that `CLAUDE.md` states without exception.

| Issue | Milestone | Size |
| :--- | :--- | :--- |
| [#11 Split main.rs: extract the order pipeline and group role config](https://github.com/krabka-io/krabka-o11y-demo/issues/11) | M2 | L |
| [#12 Stop cleanly on SIGTERM and SIGINT in all three roles](https://github.com/krabka-io/krabka-o11y-demo/issues/12) | M2 | M |
| [#13 Count and mark every failure the pipeline can hit](https://github.com/krabka-io/krabka-o11y-demo/issues/13) | M2 | M |
| [#14 Give per-order processing a real latency spread](https://github.com/krabka-io/krabka-o11y-demo/issues/14) | M2 | M |
| [#15 Make the stream role emit telemetry of its own](https://github.com/krabka-io/krabka-o11y-demo/issues/15) | M2 | L |
| [#16 Record the demo headers and the missing order attributes on the consumer span](https://github.com/krabka-io/krabka-o11y-demo/issues/16) 🌱 | M2 | S |

### [#17 Harden the observability stack: startup, coverage, correlation, alerting](https://github.com/krabka-io/krabka-o11y-demo/issues/17)

`area:stack` · lands mostly in M3 · Make the stack observable and safe to run

`demo/observability` holds a 29-service compose stack: four krabka telemetry backends, one broker acting as event bus and self-observed subject, Grafana Alloy, RustFS, the krabka schema registry, krabka-gres, cAdvisor, and 11 provisioned dashboards. The individual parts are strong. The seams between them are not.

| Issue | Milestone | Size |
| :--- | :--- | :--- |
| [#18 Give every long-running service a healthcheck and fix startup ordering](https://github.com/krabka-io/krabka-o11y-demo/issues/18) | M3 | M |
| [#19 Scrape Alloy, schema-registry and Grafana](https://github.com/krabka-io/krabka-o11y-demo/issues/19) 🌱 | M3 | S |
| [#20 Wire cross-signal correlation into the Grafana datasources](https://github.com/krabka-io/krabka-o11y-demo/issues/20) | M3 | M |
| [#21 Close the alerting gaps: saturation, liveness, pipeline stall, routing](https://github.com/krabka-io/krabka-o11y-demo/issues/21) | M3 | M |
| [#22 Guard the telemetry feedback loop and the signal cost it runs near](https://github.com/krabka-io/krabka-o11y-demo/issues/22) | M3 | M |
| [#23 Add the orders-trace and span-derived RED dashboards](https://github.com/krabka-io/krabka-o11y-demo/issues/23) | M4 | L |

### [#24 Make CI gate what the documents say it gates](https://github.com/krabka-io/krabka-o11y-demo/issues/24)

`area:ci` · lands mostly in M1 · Close the gates

CI in this repository is Bazel-only. `.github/workflows/ci.yml` runs `aspect format`, `aspect lint`, `aspect build //...`, `aspect test //...`, `bazel coverage`, a rustdoc build, and a nightly `cargo_mutants_test` matrix. `grep -rn cargo .github/workflows/` matches three comment lines, one `bazel query` selector and `cargo generate-lockfile` inside `sync-siblings.yml`.

| Issue | Milestone | Size |
| :--- | :--- | :--- |
| [#25 Repair sync-siblings: it cannot bump two of the five pinned repositories](https://github.com/krabka-io/krabka-o11y-demo/issues/25) | M1 | S |
| [#26 Add the Cargo CI lane and make cargo-deny describe this graph](https://github.com/krabka-io/krabka-o11y-demo/issues/26) | M1 | M |
| [#27 Scope Renovate's automerge to what CI actually checks](https://github.com/krabka-io/krabka-o11y-demo/issues/27) 🌱 | M1 | S |
| [#28 Enforce the workspace clippy lint table under Bazel](https://github.com/krabka-io/krabka-o11y-demo/issues/28) | M2 | M |
| [#29 Pin what CI fetches: workflow actions and the Bazel module graph](https://github.com/krabka-io/krabka-o11y-demo/issues/29) | M3 | M |
| [#30 Make the nightly mutation sweep leave something readable behind](https://github.com/krabka-io/krabka-o11y-demo/issues/30) | M3 | S |

### [#31 Prove the demo's signals with tests, not by looking at Grafana](https://github.com/krabka-io/krabka-o11y-demo/issues/31)

`area:testing` · lands mostly in M2 · Make the pipeline tell the truth

Coverage in `crates/observability-demo-app` is lopsided. `src/lib.rs` and `src/metrics.rs` have careful, mutation-aware unit tests. Every interesting behaviour of the demo sits in `src/main.rs`, which `.cargo/mutants.toml` excludes and which no unit test can reach. Fourteen of the 15 suites under `tests/` are CLI-rejection tests written against the std assertion macros, each with its own copy of a `demo()` helper. The fifteenth, `observability_demo_config.rs`, validates a 29-service stack by whitespace-exact substring searches over raw YAML and JSON.

| Issue | Milestone | Size |
| :--- | :--- | :--- |
| [#32 Convert every assertion to assert2 and put the CLI suites on one harness](https://github.com/krabka-io/krabka-o11y-demo/issues/32) | M1 | L |
| [#33 Cover --streams-fetch-min, the one knob with no test at all](https://github.com/krabka-io/krabka-o11y-demo/issues/33) 🌱 | M2 | S |
| [#34 Assert produce-to-consume trace continuation in process](https://github.com/krabka-io/krabka-o11y-demo/issues/34) | M2 | M |
| [#35 Validate the demo stack structurally and gate its config in CI](https://github.com/krabka-io/krabka-o11y-demo/issues/35) | M1 | L |
| [#36 Tie the demo dashboard to the metric registry the app exposes](https://github.com/krabka-io/krabka-o11y-demo/issues/36) | M2 | M |
| [#37 Add demo/observability/smoke.sh and run it nightly in CI](https://github.com/krabka-io/krabka-o11y-demo/issues/37) | M3 | M |

### [#38 Make the demo comprehensible to a first-time reader](https://github.com/krabka-io/krabka-o11y-demo/issues/38)

`area:docs` · lands mostly in M4 · Make the demo teachable

This repository's product is comprehension. It exists so a reader can start a stack and understand what krabka's four telemetry signals show. The reference material is strong: `demo/observability/README.md` is 336 lines of accurate operational detail. What is missing is a path through it, a picture of anything, a crate README, a design doc, and a set of claims that hold.

| Issue | Milestone | Size |
| :--- | :--- | :--- |
| [#39 One accuracy sweep: fix every claim, path and figure this repository does not support](https://github.com/krabka-io/krabka-o11y-demo/issues/39) | M1 | M |
| [#40 Add the contributor scaffolding: CONTRIBUTING, SECURITY, templates, CODEOWNERS](https://github.com/krabka-io/krabka-o11y-demo/issues/40) | M1 | M |
| [#41 Write the crate README, the design doc and the doc comments they depend on](https://github.com/krabka-io/krabka-o11y-demo/issues/41) | M4 | L |
| [#42 Add the operator guide: prerequisites, ports, tuning, troubleshooting](https://github.com/krabka-io/krabka-o11y-demo/issues/42) | M4 | M |
| [#43 Add a guided first-run walkthrough that follows one order end to end](https://github.com/krabka-io/krabka-o11y-demo/issues/43) | M4 | L |

### [#44 Know which krabka is running, and what it exposes](https://github.com/krabka-io/krabka-o11y-demo/issues/44)

`area:provenance` · lands mostly in M3 · Make the stack observable and safe to run

Two independent things in this repository call themselves krabka, and neither knows the other's version. This epic makes the running stack state its own provenance, pins what it pulls, finishes the namespace rename, and says what the demo exposes on the host.

| Issue | Milestone | Size |
| :--- | :--- | :--- |
| [#45 Report the krabka revision the demo runs and the revisions it builds against](https://github.com/krabka-io/krabka-o11y-demo/issues/45) | M3 | M |
| [#46 Pin the demo image to a digest and delegate its build documentation](https://github.com/krabka-io/krabka-o11y-demo/issues/46) | M3 | M |
| [#47 Bind published ports to the loopback address and say what the demo exposes](https://github.com/krabka-io/krabka-o11y-demo/issues/47) | M3 | S |
| [#48 Finish the crabka to krabka rename, including the README templates](https://github.com/krabka-io/krabka-o11y-demo/issues/48) 🌱 | M1 | S |

## Good first issues

These need no prior context in this repository. Each one is self-contained and bounded.

- [#16 Record the demo headers and the missing order attributes on the consumer span](https://github.com/krabka-io/krabka-o11y-demo/issues/16)
- [#19 Scrape Alloy, schema-registry and Grafana](https://github.com/krabka-io/krabka-o11y-demo/issues/19)
- [#27 Scope Renovate's automerge to what CI actually checks](https://github.com/krabka-io/krabka-o11y-demo/issues/27)
- [#33 Cover --streams-fetch-min, the one knob with no test at all](https://github.com/krabka-io/krabka-o11y-demo/issues/33)
- [#48 Finish the crabka to krabka rename, including the README templates](https://github.com/krabka-io/krabka-o11y-demo/issues/48)

## Labels

Three label families, and one label outside them.

| Prefix | What it says | Values |
| :--- | :--- | :--- |
| `type:` | The kind of work | `epic`, `feature`, `bug`, `chore`, `docs`, `test` |
| `area:` | The part of the repository it touches | `pipeline`, `stack`, `ci`, `testing`, `docs`, `provenance` |
| `size:` | The expected effort | `S` under a day, `M` a few days, `L` about a week |

`good first issue` marks the issues in [Good first issues](#good-first-issues).

## The project board

The board is a GitHub Projects v2 board, which the GitHub REST API cannot create.
Run [`tools/roadmap/create-project-board.sh`](../tools/roadmap/create-project-board.sh)
with the `gh` CLI to create it and to add every issue in this document to it.

## How to change this document

This file records what the issues say. The issues are the source of truth. If you close,
split or add an issue, update the table that holds it. Do not add work here that has no
issue behind it.
