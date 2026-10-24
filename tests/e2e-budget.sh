#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
mkdir "$scratch/bin"

# fixtures ---------------------------------------------------------------------
cat > "$scratch/bin/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
index=$(cat "$LGTM_BUDGET_TEST_DIR/index")
index=$((index + 1))
printf '%s\n' "$index" > "$LGTM_BUDGET_TEST_DIR/index"
read -r remaining reset status < <(sed -n "${index}p" "$LGTM_BUDGET_TEST_DIR/responses")
printf 'HTTP/2 200\nx-ratelimit-remaining: %s\nx-ratelimit-reset: %s\n\n{}\n' "$remaining" "$reset"
exit "$status"
STUB
cat > "$scratch/bin/date" <<'STUB'
#!/usr/bin/env bash
cat "$LGTM_BUDGET_TEST_DIR/clock"
STUB
cat > "$scratch/bin/sleep" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
now=$(cat "$LGTM_BUDGET_TEST_DIR/clock")
printf '%s\n' "$((now + $1))" > "$LGTM_BUDGET_TEST_DIR/clock"
printf '%s\n' "$1" >> "$LGTM_BUDGET_TEST_DIR/sleeps"
STUB
chmod +x "$scratch/bin/gh" "$scratch/bin/date" "$scratch/bin/sleep"

run_case() {
  local name=$1 expected_status=$2 expected_sleeps=$3 actual_status=0
  cat > "$scratch/responses"
  printf '0\n' > "$scratch/index"
  printf '1000\n' > "$scratch/clock"
  : > "$scratch/sleeps"
  LGTM_BUDGET_TEST_DIR="$scratch" LGTM_E2E_GITHUB_TOKEN=unit-test-token \
    PATH="$scratch/bin:$PATH" bash "$root/mise-tasks/e2e-budget" > "$scratch/output" 2>&1 || actual_status=$?
  if [[ "$actual_status" != "$expected_status" ]] || \
    [[ $(wc -l < "$scratch/sleeps" | tr -d ' ') != "$expected_sleeps" ]]; then
    printf 'FAIL %s: status=%s, sleeps=%s\n' "$name" "$actual_status" "$(cat "$scratch/sleeps")" >&2
    cat "$scratch/output" >&2
    exit 1
  fi
  if grep -q unit-test-token "$scratch/output"; then
    echo 'The task exposed its token' >&2
    exit 1
  fi
  printf 'PASS %s\n' "$name"
}

# tests ------------------------------------------------------------------------
run_case should_start_when_budget_is_ready 0 0 <<'RESPONSES'
4999 4600 0
RESPONSES
run_case should_wait_again_when_budget_is_still_low_after_reset 0 2 <<'RESPONSES'
346 1005 0
3764 1015 0
4999 4615 0
RESPONSES
run_case should_wait_when_primary_budget_is_exhausted 0 1 <<'RESPONSES'
0 1005 1
4999 4600 0
RESPONSES
run_case should_recheck_a_stale_reset_without_busy_looping 0 1 <<'RESPONSES'
100 990 0
4999 4600 0
RESPONSES
run_case should_reject_a_reset_beyond_the_deadline 1 0 <<'RESPONSES'
100 5000 0
RESPONSES
run_case should_stop_when_another_reset_exceeds_the_remaining_time 1 1 <<'RESPONSES'
100 4590 0
100 8190 0
RESPONSES
run_case should_fail_access_errors_without_waiting 1 0 <<'RESPONSES'
4999 4600 1
RESPONSES
run_case should_fail_invalid_headers_without_waiting 1 0 <<'RESPONSES'
invalid 4600 0
RESPONSES
