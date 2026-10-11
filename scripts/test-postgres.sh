#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -n "${LGTM_TEST_POSTGRES_URL:-}" ]]; then
  go test -race ./internal/storage ./internal/review ./internal/hosting ./internal/transport
  exit
fi
pg_bin="${LGTM_PG_BIN:-}"
if [[ -z "$pg_bin" ]] && command -v pg_ctl >/dev/null; then pg_bin="$(dirname "$(command -v pg_ctl)")"; fi
if [[ -z "$pg_bin" && -x /opt/homebrew/opt/postgresql@17/bin/pg_ctl ]]; then pg_bin=/opt/homebrew/opt/postgresql@17/bin; fi
if [[ ! -x "$pg_bin/pg_ctl" ]]; then
  echo 'Install PostgreSQL 17+ or set LGTM_PG_BIN to its bin directory.' >&2
  exit 1
fi
# Use an isolated temporary cluster and Unix socket. No persistent service,
# existing database, or public TCP listener is changed.
pg_temp="$(mktemp -d "${TMPDIR:-/tmp}/lgtm-pg.XXXXXX")"
cleanup() { "$pg_bin/pg_ctl" -D "$pg_temp/data" -m fast -w stop >/dev/null 2>&1 || true; rm -rf "$pg_temp"; }
trap cleanup EXIT
"$pg_bin/initdb" -D "$pg_temp/data" -U lgtm --auth=trust --encoding=UTF8 --locale=C >/dev/null
"$pg_bin/pg_ctl" -D "$pg_temp/data" -l "$pg_temp/postgres.log" -o "-k $pg_temp -c listen_addresses=''" -w start >/dev/null
export LGTM_TEST_POSTGRES_URL="postgresql:///postgres?host=$pg_temp&user=lgtm&sslmode=disable"
go test -race ./internal/storage ./internal/review ./internal/hosting ./internal/transport
