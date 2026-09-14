# Operator guide

The [architecture reference](../demo/observability/ARCHITECTURE.md) explains the
data, telemetry, storage, and query paths used by this guide.

## Prerequisites and startup

Use Docker Desktop 4.40 or newer with at least 12 GiB of memory and 6 CPUs.
From `demo/observability`, run:

```sh
docker compose pull
./verify-images.sh
docker compose up -d --wait --wait-timeout 600
./smoke.sh
./revision-report.sh
```

Grafana is at <http://127.0.0.1:3000>. Other published endpoints are listed in
the compose file and bind to `127.0.0.1` by default. Set `KRABKA_LISTEN_HOST`
to a trusted interface to opt into remote access.

## Tuning

`KRABKA_DEMO_ORDERS_PER_SEC` controls ingest load.
`KRABKA_DEMO_SLOW_ORDER_FRACTION` creates a deterministic latency tail.
Memory and CPU limits are grouped by tier in `.env`. Reduce the rate before
lowering distributor or trace-builder memory.

## Troubleshooting

| Symptom | Check | Recovery |
|---|---|---|
| `up --wait` times out | `docker compose ps -a` | Inspect the first unhealthy dependency and its logs. |
| Broker has no controller | `docker compose logs broker` | Remove stale demo volumes with `docker compose down -v` and restart. |
| Schema fetch fails | `curl 127.0.0.1:8081/subjects` | Wait for `schema-registry-ready`; verify broker health. |
| No order traces | Run `./smoke.sh traces cross-signal` | Confirm Alloy and trace distributor are running and the sampling ratio is nonzero. |
| Metrics are absent | Query `up` in Explore | Check the Alloy scrape target and the metrics distributor. |
| Profiles are empty | Wait for a full 60-second scrape | Create workload and inspect the profile distributor logs. |
| Gres rejects login | Check `gres-setup` completed | Set the same `KRABKA_GRES_PASSWORD` for setup and client. |
| A container restarted | Run `./check-services.sh` | Read the named service log; raise its documented memory tier if it was OOM-killed. |

## Shutdown and upgrades

`docker compose stop` sends SIGTERM; the demo roles leave their groups before
telemetry flushes. For an upgrade, save `revision-report.sh` output, change only
digest-pinned image variables, run `docker compose up -d --wait`, and rerun the
smoke test. Retain the old report and digests so `docker compose` can roll back
without deleting volumes.
